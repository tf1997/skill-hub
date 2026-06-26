use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, OptionalExtension};
use tauri::State;
use zip::ZipArchive;

use crate::{
    admin_config,
    db::{
        app_bootstrap, canonical_display_path, enforce_compiled_source, insert_audit,
        list_bindings_inner, list_cached_packages_inner, list_cached_plugin_versions_inner,
        list_cached_versions_inner, list_local_skills_inner, list_market_plugins_inner,
        list_market_skills_inner, list_plugin_bindings_inner, list_projects_inner,
        list_sources_inner, list_target_roots_inner, list_update_candidates_inner,
        market_project_cache_path, new_id, now, AppState, COMPILED_SOURCE_BUCKET,
        COMPILED_SOURCE_ENDPOINT, COMPILED_SOURCE_REGION, LOCAL_SOURCE_ID,
    },
    models::{
        AdminAuditLog, AdminDraftPlugin, AdminDraftPreviewRequest, AdminDraftSkill, AdminSession,
        AdminUnlockRequest, AppBootstrap, ArchiveMarketPluginRequest, ArchiveMarketSkillRequest,
        CachedSkillPackage, CatalogDoc, CategoriesDoc, Category, CommandError,
        DeleteCachedPluginRequest, DeleteCachedSkillRequest, DeleteLocalSkillRequest,
        DeleteMarketCategoryRequest, DeleteMarketProjectRequest, ImportLocalSkillRequest,
        InstallCachedSkillRequest, InstallPluginRequest, InstallSkillRequest,
        ListAdminAuditLogsRequest, LocalPlugin, LocalSkill, MarketPlugin, MarketProject,
        MarketSkill, PackageInfo, PluginBinding, PluginCatalogDoc, PluginManifest,
        PluginPackageRef, PluginPreviewRequest, PluginSourceMeta, PluginVersion,
        PluginVersionPackages, Project, ProjectsDoc, PublishDraftRequest, PublishMeta,
        PublishPluginDraftRequest, QuickRepublishRequest, SaveMarketCategoryRequest,
        SaveMarketProjectRequest, SaveProjectRequest, SavePublishMetaRequest, SaveSourceRequest,
        SaveTargetRootRequest, SetBindingEnabledRequest, SetLocalSkillEnabledRequest,
        SetPluginBindingEnabledRequest, SkillBinding, SkillManifest, SkillPreview,
        SkillPreviewFile, SkillPreviewFileEntry, SkillPreviewRequest, SkillVersion, Source,
        TargetRoot, UninstallPluginRequest, UpdateCandidate, UpgradeBindingRequest,
        UpgradePluginBindingRequest,
    },
    process_util::external_command,
    services::{local, object_store, package, preview, validation},
};

type CommandResult<T> = std::result::Result<T, CommandError>;

const DRAFT_GITLAB_PREFIX: &str = "draft/gitlab/skills/";
const PLUGIN_DRAFT_PREFIX: &str = "draft/gitlab/plugins/";
const DRAFT_ADMIN_PREFIX: &str = "draft/admin/gitlab/skills/";
const PLUGIN_ADMIN_PREFIX: &str = "draft/admin/gitlab/plugins/";
const ARCHIVED_ADMIN_PREFIX: &str = "draft/admin/archived/skills/";
const PROJECTS_OBJECT: &str = "projects.v1.json";
const CATALOG_OBJECT: &str = "catalog.v1.json";
const PLUGIN_CATALOG_OBJECT: &str = "plugin-catalog.v1.json";
const CATEGORIES_OBJECT: &str = "categories.v1.json";
const FIXED_PUBLISH_NAMESPACE: &str = "DT";

#[tauri::command]
pub async fn bootstrap(state: State<'_, AppState>) -> CommandResult<AppBootstrap> {
    let metadata_sync_error = refresh_catalog_best_effort(&state).await;
    map_result(app_bootstrap(&state, metadata_sync_error))
}

#[tauri::command]
pub async fn list_market_skills(state: State<'_, AppState>) -> CommandResult<Vec<MarketSkill>> {
    let _metadata_sync_error = refresh_catalog_best_effort(&state).await;
    map_result((|| {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let bindings = list_bindings_inner(&conn)?;
        let mut skills = list_market_skills_inner(&conn)?;

        for skill in &mut skills {
            skill.installed_bindings = bindings
                .iter()
                .filter(|binding| {
                    binding.namespace == skill.namespace && binding.skill_id == skill.id
                })
                .cloned()
                .collect();
            skill.cached_versions = list_cached_versions_inner(
                &conn,
                skill.source_id.as_deref(),
                &skill.namespace,
                &skill.id,
            )?;
        }

        Ok(skills)
    })())
}

#[tauri::command]
pub async fn list_market_plugins(state: State<'_, AppState>) -> CommandResult<Vec<MarketPlugin>> {
    let _metadata_sync_error = refresh_catalog_best_effort(&state).await;
    map_result((|| {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let bindings = list_plugin_bindings_inner(&conn)?;
        let mut plugins = list_market_plugins_inner(&conn)?;

        for plugin in &mut plugins {
            plugin.installed_bindings = bindings
                .iter()
                .filter(|binding| {
                    binding.namespace == plugin.namespace && binding.plugin_id == plugin.id
                })
                .cloned()
                .collect();
            plugin.cached_versions = list_cached_plugin_versions_inner(
                &conn,
                plugin.source_id.as_deref(),
                &plugin.namespace,
                &plugin.id,
            )?;
        }

        Ok(plugins)
    })())
}

#[tauri::command]
pub async fn list_sources(state: State<'_, AppState>) -> CommandResult<Vec<Source>> {
    map_result((|| {
        let conn = state.conn.lock().expect("db mutex poisoned");
        enforce_compiled_source(&conn)?;
        list_sources_inner(&conn)
    })())
}

#[tauri::command]
pub async fn save_source(
    _request: SaveSourceRequest,
    _state: State<'_, AppState>,
) -> CommandResult<Source> {
    map_result(Err(anyhow!("数据源由代码配置强制控制，客户端不能修改")))
}

#[tauri::command]
pub async fn unlock_admin_mode(
    request: AdminUnlockRequest,
    state: State<'_, AppState>,
) -> CommandResult<AdminSession> {
    map_result(unlock_admin_mode_inner(request, &state.local_macs).await)
}

async fn unlock_admin_mode_inner(
    request: AdminUnlockRequest,
    local_macs: &[String],
) -> Result<AdminSession> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;

    Ok(AdminSession {
        enabled: true,
        endpoint: COMPILED_SOURCE_ENDPOINT.to_string(),
        bucket: COMPILED_SOURCE_BUCKET.to_string(),
        region: COMPILED_SOURCE_REGION.map(ToString::to_string),
        role: authorization.role,
        projects: authorization.projects,
        mac_address: authorization.mac_address,
        name: authorization.name,
    })
}

async fn ensure_admin_allowed(
    admin_key: &str,
    local_macs: &[String],
) -> Result<admin_config::AdminAuthorization> {
    if !admin_config::is_admin_key_valid(admin_key) {
        return Err(anyhow!("管理员密钥错误"));
    }

    let allowlist = object_store::fetch_admin_mac_allowlist().await?;
    authorize_admin_from_allowlist(admin_key, local_macs, &allowlist)
}

fn authorize_admin_from_allowlist(
    admin_key: &str,
    local_macs: &[String],
    allowlist: &admin_config::MacAllowlist,
) -> Result<admin_config::AdminAuthorization> {
    if !admin_config::is_admin_key_valid(admin_key) {
        return Err(anyhow!("管理员密钥错误"));
    }

    let Some(authorization) = admin_config::admin_authorization(&local_macs, &allowlist) else {
        let detected = if local_macs.is_empty() {
            "未识别到本机 MAC".to_string()
        } else {
            local_macs.join(", ")
        };
        return Err(anyhow!(
            "本机 MAC 地址未获得管理员授权；本机识别到: {detected}"
        ));
    };

    Ok(authorization)
}

#[tauri::command]
pub async fn list_admin_drafts(
    admin_key: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<AdminDraftSkill>> {
    map_result(list_admin_drafts_inner(&admin_key, &state.local_macs).await)
}

#[tauri::command]
pub async fn list_admin_plugin_drafts(
    admin_key: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<AdminDraftPlugin>> {
    map_result(list_admin_plugin_drafts_inner(&admin_key, &state.local_macs).await)
}

#[tauri::command]
pub async fn list_admin_audit_logs(
    request: ListAdminAuditLogsRequest,
    state: State<'_, AppState>,
) -> CommandResult<Vec<AdminAuditLog>> {
    map_result(list_admin_audit_logs_inner(request, &state.local_macs).await)
}

#[tauri::command]
pub async fn preview_admin_draft(
    request: AdminDraftPreviewRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillPreview> {
    map_result(preview_admin_draft_inner(request, &state.local_macs).await)
}

#[tauri::command]
pub async fn preview_admin_plugin_draft(
    request: AdminDraftPreviewRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillPreview> {
    map_result(preview_admin_plugin_draft_inner(request, &state.local_macs).await)
}

#[tauri::command]
pub async fn save_publish_meta(
    request: SavePublishMetaRequest,
    state: State<'_, AppState>,
) -> CommandResult<PublishMeta> {
    map_result(save_publish_meta_inner(request, &state.local_macs).await)
}

#[tauri::command]
pub async fn save_market_project_remote(
    request: SaveMarketProjectRequest,
    state: State<'_, AppState>,
) -> CommandResult<Vec<MarketProject>> {
    map_result(save_market_project_remote_inner(request, &state, &state.local_macs).await)
}

#[tauri::command]
pub async fn delete_market_project_remote(
    request: DeleteMarketProjectRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        delete_market_project_remote_inner(request, &state, &state.local_macs).await?;
        refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn save_market_category_remote(
    request: SaveMarketCategoryRequest,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Category>> {
    map_result(save_market_category_remote_inner(request, &state.local_macs).await)
}

#[tauri::command]
pub async fn delete_market_category_remote(
    request: DeleteMarketCategoryRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        delete_market_category_remote_inner(request, &state.local_macs).await?;
        refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn archive_market_skill(
    request: ArchiveMarketSkillRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        archive_market_skill_inner(request, &state.local_macs).await?;
        refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn archive_market_plugin(
    request: ArchiveMarketPluginRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        archive_market_plugin_inner(request, &state.local_macs).await?;
        refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn publish_draft(
    request: PublishDraftRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        publish_draft_inner(request, &state.local_macs).await?;
        refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn publish_plugin_draft(
    request: PublishPluginDraftRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        publish_plugin_draft_inner(request, &state.local_macs).await?;
        refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn quick_republish_archived_skill(
    request: QuickRepublishRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        quick_republish_archived_skill_inner(request, &state.local_macs).await?;
        refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

async fn list_admin_drafts_inner(
    admin_key: &str,
    local_macs: &[String],
) -> Result<Vec<AdminDraftSkill>> {
    ensure_admin_allowed(admin_key, local_macs).await?;
    let client = object_store::AdminObjectClient::new();
    let objects = client.list_objects(DRAFT_GITLAB_PREFIX).await?;
    let mut drafts = Vec::new();
    let mut seen_sources = HashSet::new();

    for object in objects
        .iter()
        .filter(|object| object.ends_with("/SKILL.md") && object.starts_with(DRAFT_GITLAB_PREFIX))
    {
        let source_path = object
            .trim_start_matches(DRAFT_GITLAB_PREFIX)
            .trim_end_matches("/SKILL.md")
            .to_string();
        if source_path.trim().is_empty() {
            continue;
        }
        seen_sources.insert(source_path.clone());

        let skill_md = client.get_text(object).await.unwrap_or_default();
        let draft_metadata = parse_skill_frontmatter(&skill_md);
        let version = draft_metadata.version.clone();
        let author = draft_metadata.author.clone();
        let meta_path = admin_object_path(&source_path, "publish-meta.v1.json")?;
        let state_path = admin_object_path(&source_path, "state.v1.json")?;
        let validation_path = format!("{}{}/validation.json", DRAFT_GITLAB_PREFIX, source_path);
        let default_meta = default_publish_meta_from_draft(&source_path, &draft_metadata);
        let publish_meta = client
            .get_optional_json::<PublishMeta>(&meta_path)
            .await?
            .map(|meta| {
                merge_publish_meta_defaults(
                    normalize_publish_meta_for_source(meta, &source_path),
                    default_meta.clone(),
                )
            })
            .or(Some(default_meta));
        let state_json = client
            .get_optional_json::<serde_json::Value>(&state_path)
            .await?;
        let published_version = state_json
            .as_ref()
            .and_then(|value| {
                value
                    .get("publishedVersion")
                    .or_else(|| value.get("published_version"))
            })
            .and_then(|value| value.as_str())
            .map(ToString::to_string);
        let state_status = state_json
            .as_ref()
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string);
        let validation_status = client
            .get_optional_json::<serde_json::Value>(&validation_path)
            .await?
            .and_then(|value| validation_status_from_json(&value));

        let status = draft_status(
            version.as_deref(),
            published_version.as_deref(),
            state_status.as_deref(),
            publish_meta.as_ref(),
            validation_status.as_deref(),
        );
        let draft_location = parse_gitlab_source_path(&source_path);

        drafts.push(AdminDraftSkill {
            gitlab_source_path: source_path,
            draft_slug: draft_location.draft_slug.clone(),
            gitlab_category_code: draft_location.category_code(),
            gitlab_category_path: draft_location.category_path,
            source_available: true,
            version,
            author,
            status,
            validation_status,
            publish_meta,
            published_version,
            updated_at: None,
        });
    }

    let archived_objects = client.list_objects(ARCHIVED_ADMIN_PREFIX).await?;
    for object in archived_objects.iter().filter(|object| {
        object.ends_with("/state.v1.json") && object.starts_with(ARCHIVED_ADMIN_PREFIX)
    }) {
        let state_json = client
            .get_optional_json::<serde_json::Value>(object)
            .await?
            .unwrap_or_default();
        let source_path = state_json
            .get("gitlabSourcePath")
            .or_else(|| state_json.get("gitlab_source_path"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                object
                    .trim_start_matches(ARCHIVED_ADMIN_PREFIX)
                    .trim_end_matches("/state.v1.json")
                    .to_string()
            });
        if seen_sources.contains(&source_path) {
            continue;
        }

        let namespace = state_json
            .get("namespace")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let skill_id = state_json
            .get("skillId")
            .or_else(|| state_json.get("skill_id"))
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let state_status = state_json
            .get("status")
            .and_then(|value| value.as_str())
            .map(ToString::to_string);
        let name = state_json
            .get("name")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(skill_id);
        let summary = state_json
            .get("summary")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("已从市场下架，等待重新接入 GitLab 草稿后发布");
        let archived_at = state_json
            .get("archivedAt")
            .or_else(|| state_json.get("archived_at"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string);

        let mut draft_location = parse_gitlab_source_path(&source_path);
        if draft_location.draft_slug.is_none() && !skill_id.is_empty() {
            draft_location.draft_slug = Some(skill_id.to_string());
        }
        if draft_location.category_path.is_empty() && !namespace.is_empty() {
            draft_location.category_path.push(namespace.to_string());
        }

        drafts.push(AdminDraftSkill {
            gitlab_source_path: source_path,
            draft_slug: draft_location.draft_slug.clone(),
            gitlab_category_code: draft_location.category_code(),
            gitlab_category_path: draft_location.category_path,
            source_available: false,
            version: None,
            author: state_json
                .get("archivedBy")
                .or_else(|| state_json.get("archived_by"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string),
            status: draft_status(None, None, state_status.as_deref(), None, None),
            validation_status: None,
            publish_meta: Some(PublishMeta {
                namespace: namespace.to_string(),
                skill_id: skill_id.to_string(),
                version: None,
                name: name.to_string(),
                summary: summary.to_string(),
                tags: Vec::new(),
                targets: Vec::new(),
                levels: vec!["personal".to_string(), "project".to_string()],
                publish_scope: "project".to_string(),
                publish_category_slug: None,
                publish_project_slug: None,
                changelog: String::new(),
                updated_at: archived_at.clone(),
                updated_by: state_json
                    .get("archivedBy")
                    .or_else(|| state_json.get("archived_by"))
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
            }),
            published_version: None,
            updated_at: archived_at,
        });
    }

    drafts.sort_by(|a, b| a.gitlab_source_path.cmp(&b.gitlab_source_path));
    Ok(drafts)
}

async fn list_admin_plugin_drafts_inner(
    admin_key: &str,
    local_macs: &[String],
) -> Result<Vec<AdminDraftPlugin>> {
    ensure_admin_allowed(admin_key, local_macs).await?;
    let client = object_store::AdminObjectClient::new();
    let objects = client.list_objects(PLUGIN_DRAFT_PREFIX).await?;
    let mut drafts = Vec::new();

    for source_path in collect_plugin_draft_source_paths(&objects) {
        if source_path.trim().is_empty() {
            continue;
        }

        let state_path = plugin_admin_object_path(&source_path, "state.v1.json")?;
        let state_json = client
            .get_optional_json::<serde_json::Value>(&state_path)
            .await?;
        let published_version = state_json
            .as_ref()
            .and_then(|value| {
                value
                    .get("publishedVersion")
                    .or_else(|| value.get("published_version"))
            })
            .and_then(|value| value.as_str())
            .map(ToString::to_string);
        let state_status = state_json
            .as_ref()
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string);
        let draft_location = parse_gitlab_source_path(&source_path);
        let gitlab_category_path = draft_location.category_path.clone();

        let draft_root = format!("{}{}/", PLUGIN_DRAFT_PREFIX, source_path);
        let draft_objects = client.list_objects(&draft_root).await?;
        let validation_path = format!("{draft_root}validation.json");
        let validation_status = client
            .get_optional_json::<serde_json::Value>(&validation_path)
            .await?
            .and_then(|value| validation_status_from_json(&value));
        let content_root = resolve_plugin_draft_content_prefix(&draft_root, &draft_objects);
        let draft_prefix = content_root
            .as_ref()
            .map(|content| content.prefix.clone())
            .unwrap_or_else(|| draft_root.clone());
        let source_available = has_plugin_source_files(&draft_prefix, &draft_objects);
        let readme_metadata =
            read_plugin_readme_metadata_from_objects(&client, &draft_prefix, &draft_objects)
                .await
                .ok();
        let readme_metadata_complete = readme_metadata
            .as_ref()
            .is_some_and(is_plugin_readme_metadata_complete);
        let pluginhub = match content_root {
            Some(content_root) => {
                client
                    .get_optional_json::<PluginSourceMeta>(&content_root.pluginhub_path)
                    .await?
            }
            None => None,
        };
        let readme_default_meta = readme_metadata
            .as_ref()
            .map(|metadata| default_plugin_publish_meta_from_readme(&source_path, metadata));
        let (
            namespace,
            plugin_id,
            name,
            summary,
            version,
            targets,
            scopes,
            components,
            risk_level,
            default_meta,
        ) = match pluginhub {
            Some(meta) => {
                let default_meta = merge_plugin_readme_defaults(
                    default_plugin_publish_meta(&meta),
                    readme_default_meta.clone(),
                );
                (
                    non_empty_string(meta.namespace),
                    non_empty_string(meta.id),
                    non_empty_string(meta.name),
                    non_empty_string(meta.summary),
                    non_empty_string(meta.version),
                    meta.targets,
                    meta.scopes,
                    meta.components,
                    meta.risk_level.filter(|value| !value.trim().is_empty()),
                    Some(default_meta),
                )
            }
            None => (
                readme_default_meta
                    .as_ref()
                    .map(|meta| meta.namespace.clone())
                    .and_then(non_empty_string),
                readme_default_meta
                    .as_ref()
                    .map(|meta| meta.skill_id.clone())
                    .and_then(non_empty_string),
                readme_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.name.clone())
                    .and_then(non_empty_string),
                readme_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.description.clone())
                    .and_then(non_empty_string),
                readme_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.version.clone())
                    .and_then(non_empty_string),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                readme_default_meta,
            ),
        };
        let meta_path = plugin_admin_object_path(&source_path, "publish-meta.v1.json")?;
        let publish_meta = match client.get_optional_json::<PublishMeta>(&meta_path).await? {
            Some(meta) => Some(normalize_plugin_publish_meta(meta, default_meta.as_ref())),
            None => default_meta,
        };
        let version = version.or_else(|| {
            publish_meta
                .as_ref()
                .and_then(|meta| meta.version.clone())
                .and_then(non_empty_string)
        });
        let targets = if targets.is_empty() {
            publish_meta
                .as_ref()
                .map(|meta| meta.targets.clone())
                .unwrap_or_default()
        } else {
            targets
        };
        let scopes = if scopes.is_empty() {
            publish_meta
                .as_ref()
                .map(|meta| meta.levels.clone())
                .unwrap_or_default()
        } else {
            scopes
        };
        let components = if components.is_empty() {
            infer_plugin_components_from_object_paths(&draft_prefix, &draft_objects)
        } else {
            components
        };
        let namespace =
            namespace.or_else(|| publish_meta.as_ref().map(|meta| meta.namespace.clone()));
        let plugin_id =
            plugin_id.or_else(|| publish_meta.as_ref().map(|meta| meta.skill_id.clone()));
        let name = name.or_else(|| publish_meta.as_ref().map(|meta| meta.name.clone()));
        let summary = summary.or_else(|| publish_meta.as_ref().map(|meta| meta.summary.clone()));

        let status = plugin_draft_status(
            source_available,
            readme_metadata_complete,
            version.as_deref(),
            published_version.as_deref(),
            state_status.as_deref(),
            &targets,
            publish_meta.as_ref(),
            validation_status.as_deref(),
        );

        drafts.push(AdminDraftPlugin {
            gitlab_source_path: source_path,
            draft_slug: draft_location.draft_slug,
            gitlab_category_path,
            source_available,
            readme_metadata_complete,
            namespace,
            plugin_id,
            name,
            summary,
            version,
            targets,
            scopes,
            components,
            risk_level,
            status,
            validation_status,
            publish_meta,
            published_version,
            updated_at: None,
        });
    }

    drafts.sort_by(|a, b| a.gitlab_source_path.cmp(&b.gitlab_source_path));
    Ok(drafts)
}

async fn list_admin_audit_logs_inner(
    request: ListAdminAuditLogsRequest,
    local_macs: &[String],
) -> Result<Vec<AdminAuditLog>> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;
    ensure_can_view_admin_audit(&authorization)?;
    let limit = request.limit.unwrap_or(100).clamp(1, 200);
    let client = object_store::AdminObjectClient::new();
    let mut objects = client.list_objects("admin/audit/").await?;
    objects.retain(|object| object.ends_with(".json"));
    objects.sort_by(|a, b| b.cmp(a));

    let mut records = Vec::new();
    for object in objects {
        let Some(text) = client.get_optional_text(&object).await? else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        records.push(admin_audit_log_from_value(&object, value)?);
    }

    records.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.object_path.cmp(&a.object_path))
    });
    records.truncate(limit);
    Ok(records)
}

async fn preview_admin_draft_inner(
    request: AdminDraftPreviewRequest,
    local_macs: &[String],
) -> Result<SkillPreview> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;
    let client = object_store::AdminObjectClient::new();
    let source_path = validation::normalize_relative_object_path(&request.gitlab_source_path)?;
    let selected_path = preview::normalize_preview_file_path(request.file_path.as_deref())?;
    let draft_prefix = format!("{}{}/", DRAFT_GITLAB_PREFIX, source_path);
    let skill_md_path = format!("{draft_prefix}SKILL.md");
    let objects = client.list_objects(&draft_prefix).await?;
    if !objects.iter().any(|object| object == &skill_md_path) {
        return Err(anyhow!("Draft SKILL.md not found: {source_path}"));
    }

    let skill_md = client.get_text(&skill_md_path).await?;
    let draft_slug = parse_gitlab_source_path(&source_path).draft_slug;
    let meta_path = admin_object_path(&source_path, "publish-meta.v1.json")?;
    let meta = client
        .get_optional_json::<PublishMeta>(&meta_path)
        .await?
        .map(normalize_publish_meta);
    if let Some(meta) = meta.as_ref() {
        ensure_can_manage_publish_target(&authorization, meta)?;
    }
    let title = meta
        .as_ref()
        .map(|value| value.name.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| parse_skill_markdown_field(&skill_md, "name"))
        .or_else(|| draft_slug.clone())
        .unwrap_or_else(|| source_path.clone());
    let target = meta
        .as_ref()
        .map(|value| {
            if value.publish_scope == "project" {
                format!(
                    "project / {}",
                    value
                        .publish_project_slug
                        .as_deref()
                        .unwrap_or("unselected")
                )
            } else {
                format!(
                    "public / {}",
                    value
                        .publish_category_slug
                        .as_deref()
                        .unwrap_or("unselected")
                )
            }
        })
        .unwrap_or_else(|| "publish metadata missing".to_string());
    let mut file_list = collect_draft_preview_file_list(&draft_prefix, &objects);
    file_list.sort_by(|a, b| a.path.cmp(&b.path));
    let files = collect_draft_preview_files(
        &client,
        &draft_prefix,
        &file_list,
        selected_path.as_deref(),
        meta.as_ref(),
    )
    .await?;

    Ok(SkillPreview {
        title,
        root_path: format!(
            "minio://{}/{}{}",
            COMPILED_SOURCE_BUCKET, DRAFT_GITLAB_PREFIX, source_path
        ),
        origin: format!("MinIO draft preview - {target}"),
        files,
        file_list,
    })
}

async fn preview_admin_plugin_draft_inner(
    request: AdminDraftPreviewRequest,
    local_macs: &[String],
) -> Result<SkillPreview> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;
    let client = object_store::AdminObjectClient::new();
    let source_path = validation::normalize_relative_object_path(&request.gitlab_source_path)?;
    let selected_path = preview::normalize_preview_file_path(request.file_path.as_deref())?;
    let draft_root = format!("{}{}/", PLUGIN_DRAFT_PREFIX, source_path);
    let objects = client.list_objects(&draft_root).await?;
    if objects.is_empty() {
        return Err(anyhow!("Plugin draft not found: {source_path}"));
    }
    let content_root = resolve_plugin_draft_content_prefix(&draft_root, &objects);
    let draft_prefix = content_root
        .as_ref()
        .map(|content| content.prefix.clone())
        .unwrap_or_else(|| draft_root.clone());

    let meta = match content_root.as_ref() {
        Some(content) => {
            client
                .get_optional_json::<PluginSourceMeta>(&content.pluginhub_path)
                .await?
        }
        None => None,
    };
    let meta_path = plugin_admin_object_path(&source_path, "publish-meta.v1.json")?;
    let publish_meta = client
        .get_optional_json::<PublishMeta>(&meta_path)
        .await?
        .map(normalize_publish_meta);
    if let Some(meta) = meta.as_ref() {
        ensure_can_manage_plugin_publish_target(&authorization, meta)?;
    } else if let Some(meta) = publish_meta.as_ref() {
        ensure_can_manage_publish_target(&authorization, meta)?;
    }
    let title = meta
        .as_ref()
        .map(|value| value.name.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            publish_meta
                .as_ref()
                .map(|value| value.name.trim())
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
        .or_else(|| parse_gitlab_source_path(&source_path).draft_slug)
        .unwrap_or_else(|| source_path.clone());
    let target = meta
        .as_ref()
        .map(|value| {
            let scope = value.publish_scope.as_deref().unwrap_or("public");
            let project = value
                .publish_project_slug
                .as_deref()
                .unwrap_or("unselected");
            format!("{scope} / {project}")
        })
        .or_else(|| {
            publish_meta.as_ref().map(|value| {
                if value.publish_scope == "project" {
                    format!(
                        "project / {}",
                        value
                            .publish_project_slug
                            .as_deref()
                            .unwrap_or("unselected")
                    )
                } else {
                    format!(
                        "public / {}",
                        value
                            .publish_category_slug
                            .as_deref()
                            .unwrap_or("unselected")
                    )
                }
            })
        })
        .unwrap_or_else(|| "publish metadata missing".to_string());
    let mut file_list = collect_plugin_draft_preview_file_list(&draft_prefix, &objects);
    file_list.sort_by(|a, b| a.path.cmp(&b.path));
    let files = collect_draft_preview_files(
        &client,
        &draft_prefix,
        &file_list,
        selected_path.as_deref(),
        publish_meta.as_ref(),
    )
    .await?;

    Ok(SkillPreview {
        title,
        root_path: format!(
            "minio://{}/{}{}",
            COMPILED_SOURCE_BUCKET, PLUGIN_DRAFT_PREFIX, source_path
        ),
        origin: format!("MinIO plugin draft preview - {target}"),
        files,
        file_list,
    })
}

async fn save_publish_meta_inner(
    request: SavePublishMetaRequest,
    local_macs: &[String],
) -> Result<PublishMeta> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;
    if request
        .artifact_kind
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("plugin"))
    {
        return save_plugin_publish_meta_inner(request, &authorization).await;
    }

    let client = object_store::AdminObjectClient::new();
    let source_path = validation::normalize_relative_object_path(&request.gitlab_source_path)?;
    let mut meta = normalize_publish_meta_for_source(request.meta, &source_path);
    validation::validate_publish_meta(&meta)?;
    ensure_can_manage_publish_target(&authorization, &meta)?;
    validate_publish_target(&client, &meta).await?;
    meta.updated_at = Some(now());
    if meta.updated_by.as_deref().unwrap_or("").trim().is_empty() {
        meta.updated_by = Some(admin_actor(&authorization));
    }
    let path = admin_object_path(&source_path, "publish-meta.v1.json")?;
    client.put_json(&path, &meta).await?;
    write_admin_audit(
        &client,
        &authorization,
        "savePublishMeta",
        serde_json::json!({
            "gitlabSourcePath": source_path,
            "namespace": meta.namespace.clone(),
            "skillId": meta.skill_id.clone(),
            "publishScope": meta.publish_scope.clone(),
            "publishCategorySlug": meta.publish_category_slug.clone(),
            "publishProjectSlug": meta.publish_project_slug.clone()
        }),
    )
    .await?;
    Ok(meta)
}

async fn save_plugin_publish_meta_inner(
    request: SavePublishMetaRequest,
    authorization: &admin_config::AdminAuthorization,
) -> Result<PublishMeta> {
    let client = object_store::AdminObjectClient::new();
    let source_path = validation::normalize_relative_object_path(&request.gitlab_source_path)?;
    let draft_root = format!("{}{}/", PLUGIN_DRAFT_PREFIX, source_path);
    let draft_objects = client.list_objects(&draft_root).await?;
    let default_meta = match resolve_plugin_draft_content_prefix(&draft_root, &draft_objects) {
        Some(content_root) => client
            .get_optional_json::<PluginSourceMeta>(&content_root.pluginhub_path)
            .await?
            .as_ref()
            .map(default_plugin_publish_meta),
        None => None,
    };
    let mut meta = normalize_plugin_publish_meta(request.meta, default_meta.as_ref());
    validation::validate_publish_meta(&meta)?;
    ensure_can_manage_publish_target(authorization, &meta)?;
    validate_publish_target(&client, &meta).await?;
    meta.updated_at = Some(now());
    if meta.updated_by.as_deref().unwrap_or("").trim().is_empty() {
        meta.updated_by = Some(admin_actor(authorization));
    }
    let path = plugin_admin_object_path(&source_path, "publish-meta.v1.json")?;
    client.put_json(&path, &meta).await?;
    write_admin_audit(
        &client,
        authorization,
        "savePluginPublishMeta",
        serde_json::json!({
            "gitlabSourcePath": source_path,
            "namespace": meta.namespace.clone(),
            "pluginId": meta.skill_id.clone(),
            "publishScope": meta.publish_scope.clone(),
            "publishCategorySlug": meta.publish_category_slug.clone(),
            "publishProjectSlug": meta.publish_project_slug.clone()
        }),
    )
    .await?;
    Ok(meta)
}

async fn save_market_project_remote_inner(
    request: SaveMarketProjectRequest,
    state: &AppState,
    local_macs: &[String],
) -> Result<Vec<MarketProject>> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;
    let project_slug = request.project.slug.trim().to_string();
    validation::validate_object_segment("project slug", &project_slug)?;
    ensure_can_manage_project(&authorization, &project_slug)?;

    let client = object_store::AdminObjectClient::new();
    let mut projects = load_remote_projects(&client).await?;
    let mut project = request.project;
    project.slug = project.slug.trim().to_string();
    project.name = project.name.trim().to_string();
    project.description = project.description.trim().to_string();
    if project.name.is_empty() {
        project.name = project.slug.clone();
    }
    if project.order <= 0 {
        project.order = 10 + projects.len() as i64 * 10;
    }
    let timestamp = now();
    if project.created_at.is_none() {
        project.created_at = Some(timestamp.clone());
    }
    project.updated_at = Some(timestamp);
    if project.updated_by.is_none() {
        project.updated_by = Some(admin_actor(&authorization));
    }
    let audit_payload = serde_json::json!({
        "slug": project.slug.clone(),
        "name": project.name.clone(),
        "description": project.description.clone(),
        "order": project.order,
        "createdAt": project.created_at.clone(),
        "updatedAt": project.updated_at.clone()
    });

    projects.retain(|item| item.slug != project.slug);
    projects.push(project);
    projects = normalize_market_projects(projects);
    save_remote_projects(&client, &projects).await?;
    write_admin_audit(&client, &authorization, "saveMarketProject", audit_payload).await?;
    fs::write(
        market_project_cache_path(&state.app_dir),
        serde_json::to_string_pretty(&projects_doc(projects.clone()))?,
    )?;
    Ok(projects)
}

async fn delete_market_project_remote_inner(
    request: DeleteMarketProjectRequest,
    state: &AppState,
    local_macs: &[String],
) -> Result<()> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;
    let slug = request.slug.trim().to_string();
    validation::validate_object_segment("project slug", &slug)?;
    ensure_can_manage_project(&authorization, &slug)?;

    let client = object_store::AdminObjectClient::new();
    let mut catalog = load_remote_catalog(&client).await?;
    let project_category = format!("project:{slug}");
    if catalog
        .skills
        .iter()
        .any(|skill| skill.categories.contains(&project_category))
    {
        return Err(anyhow!("项目仍有关联 skill，请先下架该项目下的 skill"));
    }

    let mut projects = load_remote_projects(&client).await?;
    let before = projects.len();
    projects.retain(|project| project.slug != slug);
    if before == projects.len() {
        return Err(anyhow!("市场项目不存在: {slug}"));
    }

    catalog
        .categories
        .retain(|category| category != &project_category);
    catalog.generated_at = Some(now());
    save_remote_projects(&client, &projects).await?;
    write_all_market_indexes(&client, &catalog).await?;
    write_admin_audit(
        &client,
        &authorization,
        "deleteMarketProject",
        serde_json::json!({
            "slug": slug
        }),
    )
    .await?;
    fs::write(
        market_project_cache_path(&state.app_dir),
        serde_json::to_string_pretty(&projects_doc(projects))?,
    )?;
    Ok(())
}

async fn save_market_category_remote_inner(
    request: SaveMarketCategoryRequest,
    local_macs: &[String],
) -> Result<Vec<Category>> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;
    ensure_system_admin(&authorization)?;
    let category_id = request.category.id.trim().to_string();
    validation::validate_object_segment("category slug", &category_id)?;

    let client = object_store::AdminObjectClient::new();
    let mut categories = load_remote_categories(&client).await?;
    let mut category = request.category;
    category.id = category_id;
    category.name = category.name.trim().to_string();
    if category.name.is_empty() {
        category.name = category_name_from_slug(&category.id);
    }
    if category.order <= 0 {
        category.order = 10 + categories.items.len() as i64 * 10;
    }

    categories.items.retain(|item| item.id != category.id);
    categories.items.push(category.clone());
    categories = normalize_categories_doc(categories);
    categories.generated_at = Some(now());
    client.put_json(CATEGORIES_OBJECT, &categories).await?;
    write_admin_audit(
        &client,
        &authorization,
        "saveMarketCategory",
        serde_json::json!({
            "category": category
        }),
    )
    .await?;
    Ok(categories.items)
}

async fn delete_market_category_remote_inner(
    request: DeleteMarketCategoryRequest,
    local_macs: &[String],
) -> Result<()> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;
    ensure_system_admin(&authorization)?;
    let category_id = request.category_id.trim().to_string();
    validation::validate_object_segment("category slug", &category_id)?;
    let client = object_store::AdminObjectClient::new();
    let mut catalog = load_remote_catalog(&client).await?;
    if catalog.skills.iter().any(|skill| {
        skill
            .categories
            .iter()
            .any(|category| category == &category_id)
    }) {
        return Err(anyhow!("公共分类仍有关联 skill，请先下架相关 skill"));
    }

    let mut categories = load_remote_categories(&client).await?;
    categories
        .items
        .retain(|category| category.id != category_id);
    categories = normalize_categories_doc(categories);
    catalog
        .categories
        .retain(|category| category != &category_id);
    categories.generated_at = Some(now());
    catalog.generated_at = Some(now());
    client.put_json(CATEGORIES_OBJECT, &categories).await?;
    write_all_market_indexes(&client, &catalog).await?;
    write_admin_audit(
        &client,
        &authorization,
        "deleteMarketCategory",
        serde_json::json!({
            "categoryId": category_id
        }),
    )
    .await?;
    Ok(())
}

async fn publish_draft_inner(request: PublishDraftRequest, local_macs: &[String]) -> Result<()> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;
    let client = object_store::AdminObjectClient::new();
    let source_path = validation::normalize_relative_object_path(&request.gitlab_source_path)?;
    let skill_md_path = format!("{}{}/SKILL.md", DRAFT_GITLAB_PREFIX, source_path);
    let skill_md = client
        .get_text(&skill_md_path)
        .await
        .with_context(|| format!("读取草稿 SKILL.md 失败: {skill_md_path}"))?;
    let draft_metadata = parse_skill_frontmatter(&skill_md);
    let version = draft_metadata
        .version
        .clone()
        .ok_or_else(|| anyhow!("草稿 SKILL.md 缺少 version"))?;
    let author = draft_metadata
        .author
        .clone()
        .ok_or_else(|| anyhow!("草稿 SKILL.md 缺少 author"))?;

    let meta_path = admin_object_path(&source_path, "publish-meta.v1.json")?;
    let state_path = admin_object_path(&source_path, "state.v1.json")?;
    let default_meta = default_publish_meta_from_draft(&source_path, &draft_metadata);
    let meta = client
        .get_optional_json::<PublishMeta>(&meta_path)
        .await?
        .map(|meta| {
            merge_publish_meta_defaults(
                normalize_publish_meta_for_source(meta, &source_path),
                default_meta.clone(),
            )
        })
        .unwrap_or(default_meta);
    let state_json = client
        .get_optional_json::<serde_json::Value>(&state_path)
        .await?;
    let published_version = state_json
        .as_ref()
        .and_then(|value| {
            value
                .get("publishedVersion")
                .or_else(|| value.get("published_version"))
        })
        .and_then(|value| value.as_str());
    let state_archived = state_json
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("archived"));
    if !state_archived && published_version == Some(version.as_str()) {
        return Err(anyhow!("该草稿当前版本已发布，禁止重复发布"));
    }
    validation::validate_publish_meta(&meta)?;
    ensure_can_manage_publish_target(&authorization, &meta)?;
    validate_publish_target(&client, &meta).await?;
    let validation_path = format!("{}{}/validation.json", DRAFT_GITLAB_PREFIX, source_path);
    let validation_status = client
        .get_optional_json::<serde_json::Value>(&validation_path)
        .await?
        .and_then(|value| validation_status_from_json(&value));
    if validation_failed(validation_status.as_deref()) {
        return Err(anyhow!(
            "草稿 validation.json 未通过，禁止发布: {}",
            validation_status.unwrap_or_else(|| "unknown".to_string())
        ));
    }

    let draft_prefix = format!("{}{}/", DRAFT_GITLAB_PREFIX, source_path);
    let draft_objects = client.list_objects(&draft_prefix).await?;
    if draft_objects.is_empty() {
        return Err(anyhow!("草稿目录为空"));
    }

    let files = read_draft_files(&client, &draft_prefix, &draft_objects).await?;
    let source_fingerprint = draft_source_fingerprint(&files);
    let package_bytes = package::build_package_zip(&files)?;
    let package_hash = object_store::sha256_hex(&package_bytes);
    let package_size = package_bytes.len() as i64;
    let skill_json = build_skill_json(&meta, &version, &author, &package_hash, package_size);
    let job_id = new_id();
    let job_path = format!("admin/publish-jobs/{job_id}.json");

    let base = format!(
        "skills/{}/{}/versions/{}",
        meta.namespace, meta.skill_id, version
    );
    let skill_object = format!("{base}/skill.json");
    let package_object = format!("{base}/package.zip");
    let sha_object = format!("{base}/package.sha256");
    let changelog_object = format!("{base}/changelog.md");
    let manifest_object = format!("skills/{}/{}/manifest.json", meta.namespace, meta.skill_id);

    let mut manifest = client
        .get_optional_json::<SkillManifest>(&manifest_object)
        .await?
        .unwrap_or_else(|| SkillManifest {
            schema: "skillhub.skill-manifest.v1".to_string(),
            namespace: meta.namespace.clone(),
            id: meta.skill_id.clone(),
            name: meta.name.clone(),
            summary: meta.summary.clone(),
            categories: publish_categories(&meta),
            tags: meta.tags.clone(),
            targets: meta.targets.clone(),
            levels: meta.levels.clone(),
            latest_version: version.clone(),
            versions: Vec::new(),
            updated_at: Some(now()),
        });

    let mut catalog = load_remote_catalog(&client).await?;
    let version_exists = should_republish_existing_version(&manifest, &catalog, &meta, &version)?;

    manifest.name = meta.name.clone();
    manifest.summary = meta.summary.clone();
    manifest.categories = publish_categories(&meta);
    manifest.tags = meta.tags.clone();
    manifest.targets = meta.targets.clone();
    manifest.levels = meta.levels.clone();
    manifest.latest_version = version.clone();
    manifest.updated_at = Some(now());
    if !version_exists {
        manifest.versions.push(SkillVersion {
            version: version.clone(),
            skill_path: skill_object.clone(),
            package_path: package_object.clone(),
            sha256_path: sha_object.clone(),
            changelog_path: Some(changelog_object.clone()),
            signature_path: None,
            created_at: Some(now()),
            package: Some(PackageInfo {
                file: "package.zip".to_string(),
                sha256: package_hash.clone(),
                size: package_size,
            }),
        });
    }

    let old_categories = catalog
        .skills
        .iter()
        .find(|skill| skill.namespace == meta.namespace && skill.id == meta.skill_id)
        .map(|skill| skill.categories.clone())
        .unwrap_or_default();
    let new_categories = publish_categories(&meta);
    let affected_categories = merge_categories(old_categories, new_categories.clone());
    catalog
        .skills
        .retain(|skill| !(skill.namespace == meta.namespace && skill.id == meta.skill_id));
    catalog.generated_at = Some(now());
    catalog.skills.push(MarketSkill {
        namespace: meta.namespace.clone(),
        id: meta.skill_id.clone(),
        name: meta.name.clone(),
        summary: meta.summary.clone(),
        latest_version: version.clone(),
        categories: new_categories,
        tags: meta.tags.clone(),
        targets: meta.targets.clone(),
        levels: meta.levels.clone(),
        manifest_path: manifest_object.clone(),
        updated_at: Some(now()),
        source_id: None,
        installed_bindings: Vec::new(),
        cached_versions: Vec::new(),
    });
    catalog.skills.sort_by(|a, b| a.name.cmp(&b.name));
    catalog.categories = rebuild_catalog_categories(&catalog.skills);

    let categories = ensure_publish_category(load_remote_categories(&client).await?, &meta);
    let projects = load_remote_projects(&client).await?;
    let search_index = build_search_lite_index(&catalog);

    if !version_exists {
        client.put_json(&skill_object, &skill_json).await?;
        client
            .put_bytes(&package_object, package_bytes, "application/zip")
            .await?;
        client
            .put_text(
                &sha_object,
                &(package_hash.clone() + "\n"),
                "text/plain; charset=utf-8",
            )
            .await?;
        client
            .put_text(
                &changelog_object,
                &meta.changelog,
                "text/markdown; charset=utf-8",
            )
            .await?;
    }
    client.put_json(&manifest_object, &manifest).await?;
    client.put_json(CATEGORIES_OBJECT, &categories).await?;
    client
        .put_json(PROJECTS_OBJECT, &projects_doc(projects))
        .await?;
    write_market_indexes_for_categories(&client, &catalog, &affected_categories).await?;
    client
        .put_json("indexes/search-lite.v1.json", &search_index)
        .await?;

    let state = serde_json::json!({
        "gitlabSourcePath": source_path,
        "namespace": meta.namespace,
        "skillId": meta.skill_id,
        "publishedVersion": version,
        "publishedAt": now(),
        "publishedBy": admin_actor(&authorization),
        "publishScope": meta.publish_scope,
        "publishCategorySlug": meta.publish_category_slug,
        "publishProjectSlug": meta.publish_project_slug,
        "publishedSourceFingerprint": source_fingerprint,
        "lastPublishJobId": job_id,
        "status": "published",
        "updatedAt": now()
    });
    let publish_job = serde_json::json!({
        "schema": "skillhub.publish-job.v1",
        "jobId": state["lastPublishJobId"],
        "status": "succeeded",
        "gitlabSourcePath": state["gitlabSourcePath"],
        "namespace": state["namespace"],
        "skillId": state["skillId"],
        "version": state["publishedVersion"],
        "publishScope": state["publishScope"],
        "publishCategorySlug": state["publishCategorySlug"],
        "publishProjectSlug": state["publishProjectSlug"],
        "sourceFingerprint": state["publishedSourceFingerprint"],
        "createdAt": state["publishedAt"],
        "updatedAt": state["updatedAt"]
    });
    client.put_json(&state_path, &state).await?;
    client.put_json(&job_path, &publish_job).await?;
    write_admin_audit(
        &client,
        &authorization,
        "publishDraft",
        serde_json::json!({
            "job": publish_job,
            "state": state
        }),
    )
    .await?;

    client.put_json(CATALOG_OBJECT, &catalog).await?;
    Ok(())
}

async fn publish_plugin_draft_inner(
    request: PublishPluginDraftRequest,
    local_macs: &[String],
) -> Result<()> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;
    let client = object_store::AdminObjectClient::new();
    let source_path = validation::normalize_relative_object_path(&request.gitlab_source_path)?;
    let draft_root = format!("{}{}/", PLUGIN_DRAFT_PREFIX, source_path);
    let draft_objects = client.list_objects(&draft_root).await?;
    let source_prefix = resolve_plugin_draft_content_prefix(&draft_root, &draft_objects)
        .map(|content_root| content_root.prefix)
        .unwrap_or_else(|| draft_root.clone());

    let state_path = plugin_admin_object_path(&source_path, "state.v1.json")?;
    let state_json = client
        .get_optional_json::<serde_json::Value>(&state_path)
        .await?;
    let state_archived = state_json
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("archived"));

    let source_objects = draft_objects
        .iter()
        .filter(|object| object.starts_with(&source_prefix))
        .cloned()
        .collect::<Vec<_>>();
    if source_objects.is_empty() {
        return Err(anyhow!("PLUGIN_DRAFT_NOT_FOUND: plugin 草稿根目录为空"));
    }
    let validation_path = format!("{draft_root}validation.json");
    let validation_status = client
        .get_optional_json::<serde_json::Value>(&validation_path)
        .await?
        .and_then(|value| validation_status_from_json(&value));
    if validation_failed(validation_status.as_deref()) {
        return Err(anyhow!(
            "PLUGIN_VALIDATION_FAILED: 草稿 validation.json 未通过，禁止发布: {}",
            validation_status.unwrap_or_else(|| "unknown".to_string())
        ));
    }
    let files = read_plugin_draft_files(&client, &source_prefix, &source_objects).await?;
    if files.is_empty() {
        return Err(anyhow!("PLUGIN_DRAFT_NOT_FOUND: plugin 草稿根目录为空"));
    }
    let meta_path = plugin_admin_object_path(&source_path, "publish-meta.v1.json")?;
    let saved_meta = client.get_optional_json::<PublishMeta>(&meta_path).await?;
    let prepared = prepare_plugin_publish(&files, saved_meta)?;
    let meta = &prepared.meta;
    ensure_can_manage_plugin_publish_target(&authorization, meta)?;
    validate_plugin_publish_target(&client, meta).await?;

    let published_version = state_json
        .as_ref()
        .and_then(|value| {
            value
                .get("publishedVersion")
                .or_else(|| value.get("published_version"))
        })
        .and_then(|value| value.as_str());
    if !state_archived && published_version == Some(meta.version.as_str()) {
        return Err(anyhow!("PLUGIN_PUBLISH_OBJECT_EXISTS: 当前版本已发布"));
    }

    let base = format!(
        "plugins/{}/{}/versions/{}",
        meta.namespace, meta.id, meta.version
    );
    let plugin_object = format!("{base}/plugin.json");
    let inventory_object = format!("{base}/component-inventory.json");
    let risk_object = format!("{base}/risk-report.json");
    let changelog_object = format!("{base}/changelog.md");
    let manifest_object = format!("plugins/{}/{}/manifest.json", meta.namespace, meta.id);

    let mut manifest = client
        .get_optional_json::<PluginManifest>(&manifest_object)
        .await?
        .unwrap_or_else(|| PluginManifest {
            schema: "skillhub.plugin-manifest.v1".to_string(),
            namespace: meta.namespace.clone(),
            id: meta.id.clone(),
            name: meta.name.clone(),
            summary: meta.summary.clone(),
            categories: plugin_publish_categories(meta),
            tags: meta.tags.clone(),
            targets: meta.targets.clone(),
            scopes: meta.scopes.clone(),
            components: meta.components.clone(),
            risk_level: prepared.risk_level.clone(),
            latest_version: meta.version.clone(),
            versions: Vec::new(),
            updated_at: Some(now()),
        });
    let mut catalog = load_remote_plugin_catalog(&client).await?;
    let should_write_version = should_publish_plugin_version(&manifest, &catalog, meta)?;

    let old_categories = catalog
        .plugins
        .iter()
        .find(|plugin| plugin.namespace == meta.namespace && plugin.id == meta.id)
        .map(|plugin| plugin.categories.clone())
        .unwrap_or_default();
    let new_categories = plugin_publish_categories(meta);
    let affected_categories = merge_categories(old_categories, new_categories.clone());

    manifest.name = meta.name.clone();
    manifest.summary = meta.summary.clone();
    manifest.categories = new_categories.clone();
    manifest.tags = meta.tags.clone();
    manifest.targets = meta.targets.clone();
    manifest.scopes = meta.scopes.clone();
    manifest.components = meta.components.clone();
    manifest.risk_level = prepared.risk_level.clone();
    manifest.latest_version = meta.version.clone();
    manifest.updated_at = Some(now());

    let mut package_refs = PluginVersionPackages::default();
    for (target, package) in &prepared.packages {
        let package_object = format!("{base}/package.{target}.zip");
        let sha_object = format!("{base}/package.{target}.sha256");
        let package_ref = PluginPackageRef {
            package_path: package_object,
            sha256_path: sha_object,
            signature_path: None,
            package: Some(PackageInfo {
                file: format!("package.{target}.zip"),
                sha256: package.sha256.clone(),
                size: package.size,
            }),
        };
        match target.as_str() {
            "codex" => package_refs.codex = Some(package_ref),
            "claude" => package_refs.claude = Some(package_ref),
            _ => {}
        }
    }

    if should_write_version {
        manifest.versions.push(PluginVersion {
            version: meta.version.clone(),
            plugin_path: plugin_object.clone(),
            packages: package_refs,
            component_inventory_path: Some(inventory_object.clone()),
            risk_report_path: Some(risk_object.clone()),
            changelog_path: Some(changelog_object.clone()),
            created_at: Some(now()),
        });
    }

    catalog
        .plugins
        .retain(|plugin| !(plugin.namespace == meta.namespace && plugin.id == meta.id));
    catalog.generated_at = Some(now());
    catalog.plugins.push(MarketPlugin {
        namespace: meta.namespace.clone(),
        id: meta.id.clone(),
        name: meta.name.clone(),
        summary: meta.summary.clone(),
        latest_version: meta.version.clone(),
        categories: new_categories,
        tags: meta.tags.clone(),
        targets: meta.targets.clone(),
        scopes: meta.scopes.clone(),
        components: meta.components.clone(),
        risk_level: prepared.risk_level.clone(),
        manifest_path: manifest_object.clone(),
        updated_at: Some(now()),
        source_id: None,
        installed_bindings: Vec::new(),
        cached_versions: Vec::new(),
    });
    catalog.plugins.sort_by(|a, b| a.name.cmp(&b.name));

    if should_write_version {
        client
            .put_json(&plugin_object, &build_plugin_json(&prepared))
            .await?;
        for (target, package) in &prepared.packages {
            client
                .put_bytes(
                    &format!("{base}/package.{target}.zip"),
                    package.bytes.clone(),
                    "application/zip",
                )
                .await?;
            client
                .put_text(
                    &format!("{base}/package.{target}.sha256"),
                    &(package.sha256.clone() + "\n"),
                    "text/plain; charset=utf-8",
                )
                .await?;
        }
        client
            .put_json(&inventory_object, &prepared.component_inventory)
            .await?;
        client.put_json(&risk_object, &prepared.risk_report).await?;
        client
            .put_text(
                &changelog_object,
                &plugin_changelog(&files),
                "text/markdown; charset=utf-8",
            )
            .await?;
    }

    client.put_json(&manifest_object, &manifest).await?;
    write_plugin_market_indexes_for_categories(&client, &catalog, &affected_categories).await?;
    client
        .put_json(
            "indexes/plugin-search-lite.json",
            &build_plugin_search_lite_index(&catalog),
        )
        .await?;

    let source_fingerprint = draft_source_fingerprint(&files);
    let job_id = new_id();
    let job_path = format!("admin/publish-jobs/{job_id}.json");
    let state = serde_json::json!({
        "gitlabSourcePath": source_path,
        "namespace": meta.namespace,
        "pluginId": meta.id,
        "publishedVersion": meta.version,
        "publishedAt": now(),
        "publishedBy": admin_actor(&authorization),
        "publishScope": meta.publish_scope.as_deref().unwrap_or("public"),
        "publishCategorySlug": plugin_publish_category_slug(meta),
        "publishProjectSlug": meta.publish_project_slug,
        "publishedSourceFingerprint": source_fingerprint,
        "lastPublishJobId": job_id,
        "status": "published",
        "updatedAt": now()
    });
    let publish_job = serde_json::json!({
        "schema": "skillhub.plugin-publish-job.v1",
        "jobId": state["lastPublishJobId"],
        "status": "succeeded",
        "gitlabSourcePath": state["gitlabSourcePath"],
        "namespace": state["namespace"],
        "pluginId": state["pluginId"],
        "version": state["publishedVersion"],
        "publishScope": state["publishScope"],
        "publishCategorySlug": state["publishCategorySlug"],
        "publishProjectSlug": state["publishProjectSlug"],
        "sourceFingerprint": state["publishedSourceFingerprint"],
        "createdAt": state["publishedAt"],
        "updatedAt": state["updatedAt"]
    });
    client.put_json(&state_path, &state).await?;
    client.put_json(&job_path, &publish_job).await?;
    write_admin_audit(
        &client,
        &authorization,
        "publishPluginDraft",
        serde_json::json!({
            "job": publish_job,
            "state": state
        }),
    )
    .await?;

    // Catalog is the final write so clients never see a half-published plugin.
    client.put_json(PLUGIN_CATALOG_OBJECT, &catalog).await?;
    Ok(())
}

async fn quick_republish_archived_skill_inner(
    request: QuickRepublishRequest,
    local_macs: &[String],
) -> Result<()> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;
    let client = object_store::AdminObjectClient::new();
    let source_path = validation::normalize_relative_object_path(&request.gitlab_source_path)?;

    // 1. 读取保存的元数据（而不是 SKILL.md）
    // 尝试从 gitlab 路径和 archived 路径读取
    let meta_path_gitlab = admin_object_path(&source_path, "publish-meta.v1.json")?;
    let meta_path_archived = format!(
        "{}{}/publish-meta.v1.json",
        ARCHIVED_ADMIN_PREFIX, source_path
    );

    let meta = match client
        .get_optional_json::<PublishMeta>(&meta_path_gitlab)
        .await?
    {
        Some(m) => m,
        None => client
            .get_optional_json::<PublishMeta>(&meta_path_archived)
            .await?
            .ok_or_else(|| {
                anyhow!(
                    "未找到已保存的发布元数据。该 skill 可能未曾发布过，无法使用快速重新上架功能。"
                )
            })?,
    };

    // 2. 验证元数据完整性
    validation::validate_publish_meta(&meta)?;
    ensure_can_manage_publish_target(&authorization, &meta)?;

    // 3. 检查状态：必须是已下架状态
    // 同样尝试两个路径
    let state_path_gitlab = admin_object_path(&source_path, "state.v1.json")?;
    let state_path_archived = format!("{}{}/state.v1.json", ARCHIVED_ADMIN_PREFIX, source_path);

    let (state_json, actual_state_path) = match client
        .get_optional_json::<serde_json::Value>(&state_path_gitlab)
        .await?
    {
        Some(s) => (Some(s), state_path_gitlab),
        None => (
            client
                .get_optional_json::<serde_json::Value>(&state_path_archived)
                .await?,
            state_path_archived,
        ),
    };

    let state_archived = state_json
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("archived"));

    if !state_archived {
        return Err(anyhow!("该 skill 未处于已下架状态，请使用正常发布流程"));
    }

    let state_path = actual_state_path;
    let state_archived = state_json
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("archived"));

    if !state_archived {
        return Err(anyhow!("该 skill 未处于已下架状态，请使用正常发布流程"));
    }

    // 4. 尝试读取市场 manifest（如果存在）
    let manifest_object = format!("skills/{}/{}/manifest.json", meta.namespace, meta.skill_id);
    let manifest_opt = client
        .get_optional_json::<SkillManifest>(&manifest_object)
        .await?;

    // 获取版本信息：优先从 manifest，其次从 state.v1.json，最后从 publish-meta
    let latest_version = if let Some(ref manifest) = manifest_opt {
        // 5a. 如果 manifest 存在，验证包文件存在
        let version_info = manifest
            .versions
            .iter()
            .find(|v| v.version == manifest.latest_version)
            .ok_or_else(|| anyhow!("manifest 中未找到最新版本信息"))?;

        // 检查 skill 包是否存在
        let skill_json_exists = client
            .get_optional_text(&version_info.skill_path)
            .await?
            .is_some();
        if !skill_json_exists {
            return Err(anyhow!(
                "市场中的 skill 包文件不存在: {}。无法重新上架。",
                version_info.skill_path
            ));
        }

        manifest.latest_version.clone()
    } else {
        // 5b. 如果 manifest 不存在，尝试从 state.v1.json 读取版本
        state_json
            .as_ref()
            .and_then(|v| {
                v.get("publishedVersion")
                    .or_else(|| v.get("published_version"))
            })
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                anyhow!(
                    "无法确定 skill 版本信息。该 skill 的下架记录不完整，缺少版本号。\n\n\
                可能的原因：\n\
                1. 该 skill 从未成功发布过，只是创建了草稿\n\
                2. 下架时版本信息未被保存\n\
                3. 市场中的包文件已被完全删除\n\n\
                建议：请使用正常发布流程重新发布该 skill。"
                )
            })?
    };

    // 6. 重新加入市场目录
    let mut catalog = load_remote_catalog(&client).await?;
    let already_in_catalog = catalog
        .skills
        .iter()
        .any(|skill| skill.namespace == meta.namespace && skill.id == meta.skill_id);

    if already_in_catalog {
        return Err(anyhow!("该 skill 已在市场目录中，无需重新上架"));
    }

    // 添加到目录
    let new_categories = publish_categories(&meta);
    catalog.generated_at = Some(now());
    catalog.skills.push(MarketSkill {
        namespace: meta.namespace.clone(),
        id: meta.skill_id.clone(),
        name: meta.name.clone(),
        summary: meta.summary.clone(),
        latest_version: latest_version.clone(),
        categories: new_categories.clone(),
        tags: meta.tags.clone(),
        targets: meta.targets.clone(),
        levels: meta.levels.clone(),
        manifest_path: manifest_object.clone(),
        updated_at: Some(now()),
        source_id: None,
        installed_bindings: Vec::new(),
        cached_versions: Vec::new(),
    });
    catalog.skills.sort_by(|a, b| a.name.cmp(&b.name));
    catalog.categories = rebuild_catalog_categories(&catalog.skills);

    // 7. 确保分类和项目存在
    let categories = ensure_publish_category(load_remote_categories(&client).await?, &meta);
    let projects = load_remote_projects(&client).await?;

    // 8. 重建搜索索引
    let search_index = build_search_lite_index(&catalog);
    let affected_categories = new_categories;

    // 9. 更新状态为已发布
    let new_state = serde_json::json!({
        "gitlabSourcePath": source_path,
        "namespace": meta.namespace,
        "skillId": meta.skill_id,
        "publishedVersion": latest_version,
        "publishedAt": now(),
        "publishedBy": admin_actor(&authorization),
        "publishScope": meta.publish_scope,
        "publishCategorySlug": meta.publish_category_slug,
        "publishProjectSlug": meta.publish_project_slug,
        "status": "published",
        "republishedAt": now(),
        "updatedAt": now()
    });
    client.put_json(&state_path, &new_state).await?;

    // 10. 保存目录和索引
    client.put_json(CATEGORIES_OBJECT, &categories).await?;
    client
        .put_json(PROJECTS_OBJECT, &projects_doc(projects))
        .await?;
    write_market_indexes_for_categories(&client, &catalog, &affected_categories).await?;
    client
        .put_json("indexes/search-lite.v1.json", &search_index)
        .await?;
    client.put_json(CATALOG_OBJECT, &catalog).await?;

    // 11. 记录审计日志
    write_admin_audit(
        &client,
        &authorization,
        "quickRepublishArchivedSkill",
        serde_json::json!({
            "namespace": meta.namespace,
            "skillId": meta.skill_id,
            "version": latest_version,
            "state": new_state
        }),
    )
    .await?;

    Ok(())
}

async fn archive_market_skill_inner(
    request: ArchiveMarketSkillRequest,
    local_macs: &[String],
) -> Result<()> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;
    validation::validate_object_segment("namespace", &request.namespace)?;
    validation::validate_object_segment("skill id", &request.skill_id)?;

    let client = object_store::AdminObjectClient::new();
    let mut catalog = load_remote_catalog(&client).await?;
    let Some(skill) = catalog
        .skills
        .iter()
        .find(|skill| skill.namespace == request.namespace && skill.id == request.skill_id)
        .cloned()
    else {
        return Err(anyhow!(
            "市场 skill 不存在: {}/{}",
            request.namespace,
            request.skill_id
        ));
    };
    ensure_can_manage_skill_categories(&authorization, &skill.categories)?;

    catalog
        .skills
        .retain(|item| !(item.namespace == request.namespace && item.id == request.skill_id));
    let affected_categories = skill.categories.clone();
    catalog.categories = rebuild_catalog_categories(&catalog.skills);
    catalog.generated_at = Some(now());
    write_market_indexes_for_categories(&client, &catalog, &affected_categories).await?;
    client
        .put_json(
            "indexes/search-lite.v1.json",
            &build_search_lite_index(&catalog),
        )
        .await?;
    client.put_json(CATALOG_OBJECT, &catalog).await?;

    let source_path =
        find_draft_source_for_skill(&client, &request.namespace, &request.skill_id).await?;
    let archive_source_path = source_path
        .clone()
        .unwrap_or_else(|| format!("{}/{}", request.namespace, request.skill_id));
    let state = serde_json::json!({
        "gitlabSourcePath": archive_source_path,
        "namespace": request.namespace,
        "skillId": request.skill_id,
        "name": skill.name,
        "summary": skill.summary,
        "categories": skill.categories,
        "publishedVersion": skill.latest_version,
        "archivedAt": now(),
        "archivedBy": admin_actor(&authorization),
        "reason": request.reason.unwrap_or_default(),
        "status": "archived",
        "updatedAt": now()
    });
    let state_path = source_path
        .as_ref()
        .map(|path| admin_object_path(path, "state.v1.json"))
        .transpose()?
        .unwrap_or_else(|| {
            format!(
                "draft/admin/archived/skills/{}/{}/state.v1.json",
                request.namespace, request.skill_id
            )
        });
    client.put_json(&state_path, &state).await?;
    write_admin_audit(
        &client,
        &authorization,
        "archiveMarketSkill",
        serde_json::json!({
            "namespace": request.namespace,
            "skillId": request.skill_id,
            "categories": skill.categories,
            "statePath": state_path,
        }),
    )
    .await?;
    Ok(())
}

async fn archive_market_plugin_inner(
    request: ArchiveMarketPluginRequest,
    local_macs: &[String],
) -> Result<()> {
    let authorization = ensure_admin_allowed(&request.admin_key, local_macs).await?;
    validation::validate_object_segment("namespace", &request.namespace)?;
    validation::validate_object_segment("plugin id", &request.plugin_id)?;

    let client = object_store::AdminObjectClient::new();
    let mut catalog = load_remote_plugin_catalog(&client).await?;
    let Some(plugin) = catalog
        .plugins
        .iter()
        .find(|plugin| plugin.namespace == request.namespace && plugin.id == request.plugin_id)
        .cloned()
    else {
        return Err(anyhow!(
            "市场 plugin 不存在: {}/{}",
            request.namespace,
            request.plugin_id
        ));
    };
    ensure_can_manage_skill_categories(&authorization, &plugin.categories)?;

    catalog
        .plugins
        .retain(|item| !(item.namespace == request.namespace && item.id == request.plugin_id));
    let affected_categories = plugin.categories.clone();
    catalog.generated_at = Some(now());
    write_plugin_market_indexes_for_categories(&client, &catalog, &affected_categories).await?;
    client
        .put_json(
            "indexes/plugin-search-lite.json",
            &build_plugin_search_lite_index(&catalog),
        )
        .await?;
    client.put_json(PLUGIN_CATALOG_OBJECT, &catalog).await?;

    let source_path =
        find_draft_source_for_plugin(&client, &request.namespace, &request.plugin_id).await?;
    let archive_source_path = source_path
        .clone()
        .unwrap_or_else(|| format!("{}/{}", request.namespace, request.plugin_id));
    let state = serde_json::json!({
        "gitlabSourcePath": archive_source_path,
        "namespace": request.namespace,
        "pluginId": request.plugin_id,
        "name": plugin.name,
        "summary": plugin.summary,
        "categories": plugin.categories,
        "publishedVersion": plugin.latest_version,
        "archivedAt": now(),
        "archivedBy": admin_actor(&authorization),
        "reason": request.reason.unwrap_or_default(),
        "status": "archived",
        "updatedAt": now()
    });
    let state_path = source_path
        .as_ref()
        .map(|path| plugin_admin_object_path(path, "state.v1.json"))
        .transpose()?
        .unwrap_or_else(|| {
            format!(
                "draft/admin/archived/plugins/{}/{}/state.v1.json",
                request.namespace, request.plugin_id
            )
        });
    client.put_json(&state_path, &state).await?;
    write_admin_audit(
        &client,
        &authorization,
        "archiveMarketPlugin",
        serde_json::json!({
            "namespace": request.namespace,
            "pluginId": request.plugin_id,
            "categories": plugin.categories,
            "statePath": state_path,
        }),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn list_target_roots(state: State<'_, AppState>) -> CommandResult<Vec<TargetRoot>> {
    map_result((|| {
        let conn = state.conn.lock().expect("db mutex poisoned");
        list_target_roots_inner(&conn)
    })())
}

#[tauri::command]
pub async fn save_target_root(
    request: SaveTargetRootRequest,
    state: State<'_, AppState>,
) -> CommandResult<TargetRoot> {
    map_result((|| {
        validation::validate_target(&request.target)?;
        let path = request.personal_path.trim();
        if path.is_empty() {
            return Err(anyhow!("personal skill root is required"));
        }

        let updated_at = now();
        let conn = state.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "INSERT INTO target_roots (target, personal_path, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(target) DO UPDATE SET
               personal_path = excluded.personal_path,
               updated_at = excluded.updated_at",
            params![request.target, path, updated_at],
        )?;

        list_target_roots_inner(&conn)?
            .into_iter()
            .find(|root| root.target == request.target)
            .ok_or_else(|| anyhow!("failed to read target root after save"))
    })())
}

#[tauri::command]
pub async fn refresh_catalog(state: State<'_, AppState>) -> CommandResult<AppBootstrap> {
    let result = match refresh_catalog_inner(&state).await {
        Ok(_) => app_bootstrap(&state, None),
        Err(err) => Err(err),
    };
    map_result(result)
}

#[tauri::command]
pub async fn install_skill(
    request: InstallSkillRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillBinding> {
    map_result(install_skill_inner(request, &state).await)
}

#[tauri::command]
pub async fn install_plugin(
    request: InstallPluginRequest,
    state: State<'_, AppState>,
) -> CommandResult<PluginBinding> {
    map_result(install_plugin_inner(request, &state).await)
}

#[tauri::command]
pub async fn delete_cached_skill(
    request: DeleteCachedSkillRequest,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    map_result(delete_cached_skill_inner(request, &state))
}

#[tauri::command]
pub async fn delete_cached_plugin(
    request: DeleteCachedPluginRequest,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    map_result(delete_cached_plugin_inner(request, &state))
}

#[tauri::command]
pub async fn delete_local_skill(
    request: DeleteLocalSkillRequest,
    state: State<'_, AppState>,
) -> CommandResult<Vec<LocalSkill>> {
    map_result(delete_local_skill_inner(request, &state))
}

#[tauri::command]
pub async fn set_local_skill_enabled(
    request: SetLocalSkillEnabledRequest,
    state: State<'_, AppState>,
) -> CommandResult<Vec<LocalSkill>> {
    map_result(set_local_skill_enabled_inner(request, &state))
}

#[tauri::command]
pub async fn import_local_skill_to_cache(
    request: ImportLocalSkillRequest,
    state: State<'_, AppState>,
) -> CommandResult<CachedSkillPackage> {
    map_result(import_local_skill_to_cache_inner(request, &state))
}

#[tauri::command]
pub async fn install_cached_skill(
    request: InstallCachedSkillRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillBinding> {
    map_result(install_cached_skill_inner(request, &state))
}

async fn install_skill_inner(
    request: InstallSkillRequest,
    state: &AppState,
) -> Result<SkillBinding> {
    validation::validate_target(&request.target)?;
    validation::validate_level(&request.level)?;
    let _metadata_sync_error = refresh_catalog_best_effort(state).await;

    if request.level == "project" && request.project_path.as_deref().unwrap_or("").is_empty() {
        return Err(anyhow!("项目级启用必须选择项目目录"));
    }

    let (source_id, skill, source) = {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let source_id = request.source_id.clone().or_else(|| {
            preview::default_source_for_skill(&conn, &request.namespace, &request.skill_id)
                .ok()
                .flatten()
        });
        let skill = preview::find_market_skill(
            &conn,
            source_id.as_deref(),
            &request.namespace,
            &request.skill_id,
        )?;
        let source = source_id.as_deref().and_then(|id| {
            list_sources_inner(&conn)
                .ok()?
                .into_iter()
                .find(|item| item.id == id)
        });

        if request.enable {
            ensure_scope_can_enable(
                &conn,
                None,
                &request.namespace,
                &request.skill_id,
                &request.target,
                &request.level,
            )?;
            if let Some(existing) = find_same_scope_binding(
                &conn,
                &request.namespace,
                &request.skill_id,
                &request.target,
                &request.level,
                request.project_path.as_deref(),
            )? {
                return Ok(existing);
            }
        }

        (source_id, skill, source)
    };

    let version = request
        .version
        .clone()
        .unwrap_or_else(|| skill.latest_version.clone());
    let version_info = match source.as_ref() {
        Some(source) => Some(fetch_manifest_version(source, &skill.manifest_path, &version).await?),
        _ => None,
    };

    let package_path = prepare_package(
        state,
        source.as_ref(),
        &skill,
        &version,
        version_info.as_ref(),
    )
    .await?;

    let package_id = {
        let conn = state.conn.lock().expect("db mutex poisoned");
        ensure_package_record(
            &conn,
            source_id.as_deref(),
            &request.namespace,
            &request.skill_id,
            &version,
            &package_path,
            version_info
                .as_ref()
                .and_then(|info| info.package.as_ref().map(|package| package.sha256.as_str())),
        )?
    };

    let install_path = build_install_path(
        state,
        &request.target,
        &request.level,
        request.project_path.as_deref(),
        &request.namespace,
        &request.skill_id,
    )?;

    if !request.enable {
        let skill_ref = format!("{}/{}@{}", skill.namespace, skill.id, version);
        let conn = state.conn.lock().expect("db mutex poisoned");
        insert_audit(&conn, "cache", Some(&skill_ref), "success", None)?;

        return Ok(SkillBinding {
            id: package_id.clone(),
            package_id,
            source_id,
            namespace: request.namespace,
            skill_id: request.skill_id,
            skill_name: skill.name,
            version,
            target: request.target,
            level: request.level,
            project_path: request.project_path,
            install_path: canonical_display_path(&install_path),
            enabled: false,
            install_mode: "cache".to_string(),
            update_policy: request
                .update_policy
                .unwrap_or_else(|| "follow_latest".to_string()),
            status: "cached".to_string(),
            created_at: now(),
            updated_at: now(),
        });
    }

    if request.enable {
        fs::create_dir_all(&install_path).context("创建安装目录失败")?;
        package::copy_package_to_install(&package_path, &install_path)?;
    }

    let now = now();
    let id = new_id();
    let install_mode = request.install_mode.unwrap_or_else(|| "copy".to_string());
    let update_policy = request
        .update_policy
        .unwrap_or_else(|| "follow_latest".to_string());

    let conn = state.conn.lock().expect("db mutex poisoned");
    conn.execute(
            "INSERT INTO skill_bindings
             (id, package_id, source_id, namespace, skill_id, skill_name, version, target, level,
              project_path, install_path, enabled, install_mode, update_policy, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 'installed', ?15, ?16)",
            params![
                id,
                package_id,
                source_id,
                request.namespace,
                request.skill_id,
                skill.name,
                version,
                request.target,
                request.level,
                request.project_path,
                canonical_display_path(&install_path),
                if request.enable { 1_i64 } else { 0_i64 },
                install_mode,
                update_policy,
                now,
                now
            ],
        )?;

    let skill_ref = format!("{}/{}@{}", skill.namespace, skill.id, version);
    insert_audit(&conn, "install", Some(&skill_ref), "success", None)?;

    list_bindings_inner(&conn)?
        .into_iter()
        .find(|binding| binding.id == id)
        .ok_or_else(|| anyhow!("安装后读取绑定失败"))
}

async fn install_plugin_inner(
    request: InstallPluginRequest,
    state: &AppState,
) -> Result<PluginBinding> {
    validation::validate_plugin_target(&request.target)?;
    validation::validate_plugin_scope(&request.scope)?;
    validation::validate_plugin_target_scope(&request.target, &request.scope)?;
    let _metadata_sync_error = refresh_catalog_best_effort(state).await;

    if request.scope == "project" && request.project_path.as_deref().unwrap_or("").is_empty() {
        return Err(anyhow!(
            "PLUGIN_MARKETPLACE_WRITE_FAILED: project scope requires projectPath"
        ));
    }

    let (source_id, plugin, source) = {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let source_id = request.source_id.clone().or_else(|| {
            preview::default_source_for_plugin(&conn, &request.namespace, &request.plugin_id)
                .ok()
                .flatten()
        });
        let plugin = preview::find_market_plugin(
            &conn,
            source_id.as_deref(),
            &request.namespace,
            &request.plugin_id,
        )?;
        if !plugin
            .targets
            .iter()
            .any(|target| target == &request.target)
        {
            return Err(anyhow!("PLUGIN_TARGET_UNSUPPORTED: {}", request.target));
        }
        if !plugin.scopes.iter().any(|scope| scope == &request.scope) {
            return Err(anyhow!(
                "PLUGIN_SOURCE_INVALID: unsupported scope {}",
                request.scope
            ));
        }
        let source = source_id.as_deref().and_then(|id| {
            list_sources_inner(&conn)
                .ok()?
                .into_iter()
                .find(|item| item.id == id)
        });
        if request.enable {
            ensure_plugin_scope_can_enable(
                &conn,
                None,
                &request.namespace,
                &request.plugin_id,
                &request.target,
                &request.scope,
            )?;
            if let Some(existing) = find_same_plugin_binding(
                &conn,
                &request.namespace,
                &request.plugin_id,
                &request.target,
                &request.scope,
                request.project_path.as_deref(),
            )? {
                return Ok(existing);
            }
        }
        (source_id, plugin, source)
    };

    let version = request
        .version
        .clone()
        .unwrap_or_else(|| plugin.latest_version.clone());
    let version_info = match source.as_ref() {
        Some(source) => {
            Some(fetch_plugin_manifest_version(source, &plugin.manifest_path, &version).await?)
        }
        _ => None,
    };
    let package_dir = prepare_plugin_package(
        state,
        source.as_ref(),
        &plugin,
        &version,
        &request.target,
        version_info.as_ref(),
    )
    .await?;
    let component_inventory_json =
        plugin_component_inventory_json(source.as_ref(), version_info.as_ref(), &plugin).await?;

    let package_id = {
        let conn = state.conn.lock().expect("db mutex poisoned");
        ensure_plugin_package_record(
            &conn,
            source_id.as_deref(),
            &plugin,
            &version,
            &request.target,
            &package_dir,
            version_info.as_ref().and_then(|info| {
                plugin_package_ref_for_target(info, &request.target)
                    .and_then(|package_ref| package_ref.package.as_ref())
                    .map(|package| package.sha256.as_str())
            }),
            &component_inventory_json,
        )?
    };

    if !request.enable {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let plugin_ref = format!("{}/{}@{}", plugin.namespace, plugin.id, version);
        insert_audit(
            &conn,
            "cache_plugin",
            Some(&plugin_ref),
            "success",
            Some(&request.target),
        )?;
        return Ok(PluginBinding {
            id: package_id.clone(),
            package_id,
            source_id,
            namespace: request.namespace,
            plugin_id: request.plugin_id,
            plugin_name: plugin.name,
            version,
            target: request.target,
            scope: request.scope,
            project_path: request.project_path,
            marketplace_id: None,
            marketplace_name: "skillhub".to_string(),
            platform_ref: String::new(),
            enabled: false,
            install_mode: "cache".to_string(),
            update_policy: request
                .update_policy
                .unwrap_or_else(|| "follow_latest".to_string()),
            status: "cached".to_string(),
            created_at: now(),
            updated_at: now(),
        });
    }

    let marketplace = materialize_plugin_marketplace(
        state,
        &plugin,
        &version,
        &request.target,
        &request.scope,
        request.project_path.as_deref(),
        &package_dir,
    )?;
    sync_codex_plugin_install(
        &request.target,
        &request.scope,
        &plugin.id,
        &marketplace.name,
        &marketplace.root_path,
    )?;
    sync_claude_plugin_install(
        &request.target,
        &plugin.id,
        &marketplace.name,
        &request.scope,
        request.project_path.as_deref(),
        &marketplace.root_path,
    )?;
    let now = now();
    let id = new_id();
    let install_mode = request
        .install_mode
        .unwrap_or_else(|| "marketplace".to_string());
    let update_policy = request
        .update_policy
        .unwrap_or_else(|| "follow_latest".to_string());
    let platform_ref = format!("{}@{}", plugin.id, marketplace.name);

    let conn = state.conn.lock().expect("db mutex poisoned");
    conn.execute(
        "INSERT INTO plugin_marketplaces
         (id, target, scope, project_path, marketplace_name, root_path, marketplace_path, status, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'materialized', ?8)
         ON CONFLICT(target, scope, project_path, marketplace_name) DO UPDATE SET
           root_path = excluded.root_path,
           marketplace_path = excluded.marketplace_path,
           status = excluded.status,
           updated_at = excluded.updated_at",
        params![
            marketplace.id,
            request.target,
            request.scope,
            request.project_path,
            marketplace.name,
            canonical_display_path(&marketplace.root_path),
            canonical_display_path(&marketplace.marketplace_path),
            now
        ],
    )?;
    let marketplace_id: String = conn.query_row(
        "SELECT id FROM plugin_marketplaces
         WHERE target = ?1
           AND scope = ?2
           AND COALESCE(project_path, '') = COALESCE(?3, '')
           AND marketplace_name = ?4
         LIMIT 1",
        params![
            request.target,
            request.scope,
            request.project_path,
            marketplace.name
        ],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT INTO plugin_bindings
         (id, package_id, source_id, namespace, plugin_id, plugin_name, version, target, scope,
          project_path, marketplace_id, marketplace_name, platform_ref, enabled, install_mode,
          update_policy, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 'installed', ?17, ?18)",
        params![
            id,
            package_id,
            source_id,
            request.namespace,
            request.plugin_id,
            plugin.name,
            version,
            request.target,
            request.scope,
            request.project_path,
            marketplace_id,
            marketplace.name,
            platform_ref,
            if request.enable { 1_i64 } else { 0_i64 },
            install_mode,
            update_policy,
            now,
            now
        ],
    )?;
    let plugin_ref = format!("{}/{}@{}", plugin.namespace, plugin.id, version);
    insert_audit(
        &conn,
        "install_plugin",
        Some(&plugin_ref),
        "success",
        Some(&request.target),
    )?;
    list_plugin_bindings_inner(&conn)?
        .into_iter()
        .find(|binding| binding.id == id)
        .ok_or_else(|| anyhow!("PLUGIN_MARKETPLACE_WRITE_FAILED: failed to read plugin binding"))
}

fn delete_cached_skill_inner(request: DeleteCachedSkillRequest, state: &AppState) -> Result<()> {
    let conn = state.conn.lock().expect("db mutex poisoned");
    let cached: Option<(String, String)> = conn
        .query_row(
            "SELECT id, package_path FROM skill_packages
             WHERE COALESCE(source_id, '') = COALESCE(?1, '')
               AND namespace = ?2
               AND skill_id = ?3
               AND version = ?4",
            params![
                request.source_id.as_deref(),
                request.namespace,
                request.skill_id,
                request.version
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    let Some((package_id, package_path)) = cached else {
        return Ok(());
    };

    let path = PathBuf::from(&package_path);
    if path.exists() {
        package::ensure_safe_package_cache_path(state, &path)?;
        fs::remove_dir_all(&path).context("删除本地包缓存失败")?;
    }

    conn.execute(
        "DELETE FROM local_package_metadata WHERE package_id = ?1",
        params![package_id],
    )?;
    conn.execute(
        "DELETE FROM skill_packages WHERE id = ?1",
        params![package_id],
    )?;
    let skill_ref = format!(
        "{}/{}@{}",
        request.namespace, request.skill_id, request.version
    );
    insert_audit(&conn, "delete_cache", Some(&skill_ref), "success", None)?;
    Ok(())
}

fn delete_cached_plugin_inner(request: DeleteCachedPluginRequest, state: &AppState) -> Result<()> {
    let conn = state.conn.lock().expect("db mutex poisoned");
    let cached: Option<(String, String, i64)> = conn
        .query_row(
            "SELECT id, package_path, (
                 SELECT COUNT(*)
                 FROM plugin_bindings binding
                 WHERE binding.package_id = package.id
             )
             FROM plugin_packages package
             WHERE COALESCE(package.source_id, '') = COALESCE(?1, '')
               AND package.namespace = ?2
               AND package.plugin_id = ?3
               AND package.version = ?4
               AND package.target = ?5",
            params![
                request.source_id.as_deref(),
                request.namespace,
                request.plugin_id,
                request.version,
                request.target
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    let Some((package_id, package_path, binding_count)) = cached else {
        return Ok(());
    };
    if binding_count > 0 {
        return Err(anyhow!(
            "PLUGIN_MARKETPLACE_WRITE_FAILED: cached plugin package has bindings"
        ));
    }

    let path = PathBuf::from(&package_path);
    if path.exists() {
        package::ensure_safe_plugin_package_cache_path(state, &path)?;
        fs::remove_dir_all(&path)
            .context("PLUGIN_MARKETPLACE_WRITE_FAILED: remove cached plugin package failed")?;
    }

    conn.execute(
        "DELETE FROM local_package_metadata WHERE package_id = ?1",
        params![package_id],
    )?;
    conn.execute(
        "DELETE FROM plugin_packages WHERE id = ?1",
        params![package_id],
    )?;
    let plugin_ref = format!(
        "{}/{}@{}#{}",
        request.namespace, request.plugin_id, request.version, request.target
    );
    insert_audit(
        &conn,
        "delete_cache_plugin",
        Some(&plugin_ref),
        "success",
        Some(&request.target),
    )?;
    Ok(())
}

fn delete_local_skill_inner(
    request: DeleteLocalSkillRequest,
    state: &AppState,
) -> Result<Vec<LocalSkill>> {
    let conn = state.conn.lock().expect("db mutex poisoned");
    let row: Option<(
        String,
        Option<String>,
        Option<String>,
        bool,
        String,
        String,
        String,
    )> = conn
        .query_row(
            "SELECT path, detected_manifest, skill_id, managed_by_skillhub, status, target, level
             FROM local_skills
             WHERE id = ?1",
            params![request.id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get::<_, i64>(3)? != 0,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?;

    let Some((path, detected_manifest, skill_id, managed_by_skillhub, status, target, level)) = row
    else {
        return list_local_skills_inner(&conn);
    };
    if managed_by_skillhub {
        return Err(anyhow!(
            "该 skill 由 Skill Hub 管理，请使用生效矩阵中的卸载操作"
        ));
    }
    if status == "missing" {
        conn.execute(
            "DELETE FROM local_skills WHERE id = ?1",
            params![request.id],
        )?;
        return list_local_skills_inner(&conn);
    }

    let path_buf = PathBuf::from(&path);
    if !path_buf.is_dir() {
        conn.execute(
            "DELETE FROM local_skills WHERE id = ?1",
            params![request.id],
        )?;
        return list_local_skills_inner(&conn);
    }
    if !path_buf.join("SKILL.md").is_file() {
        return Err(anyhow!("目标目录缺少 SKILL.md，已拒绝删除"));
    }

    fs::remove_dir_all(&path_buf).context("删除本地 skill 目录失败")?;
    conn.execute(
        "DELETE FROM local_skills WHERE id = ?1",
        params![request.id],
    )?;

    let audit_ref = skill_id
        .filter(|value| !value.trim().is_empty())
        .or(detected_manifest)
        .unwrap_or_else(|| path.clone());
    let target_scope = format!("{target}:{level}");
    insert_audit(
        &conn,
        "delete_local_skill",
        Some(&audit_ref),
        "success",
        Some(&target_scope),
    )?;
    list_local_skills_inner(&conn)
}

fn set_local_skill_enabled_inner(
    request: SetLocalSkillEnabledRequest,
    state: &AppState,
) -> Result<Vec<LocalSkill>> {
    let conn = state.conn.lock().expect("db mutex poisoned");
    let row: Option<(
        String,
        Option<String>,
        Option<String>,
        bool,
        bool,
        String,
        String,
    )> = conn
        .query_row(
            "SELECT path, detected_manifest, skill_id, managed_by_skillhub, enabled, target, level
             FROM local_skills
             WHERE id = ?1",
            params![request.id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get::<_, i64>(3)? != 0,
                    row.get::<_, i64>(4)? != 0,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?;

    let Some((path, detected_manifest, skill_id, managed_by_skillhub, enabled, target, level)) =
        row
    else {
        return list_local_skills_inner(&conn);
    };
    if managed_by_skillhub {
        return Err(anyhow!(
            "Skill Hub 管理的市场绑定请使用生效矩阵中的市场开关"
        ));
    }
    if enabled == request.enabled {
        return list_local_skills_inner(&conn);
    }

    let current_path = PathBuf::from(&path);
    let next_path = if request.enabled {
        enabled_local_skill_path(&current_path)?
    } else {
        disabled_local_skill_path(&current_path)?
    };

    if !current_path.is_dir() {
        conn.execute(
            "DELETE FROM local_skills WHERE id = ?1",
            params![request.id],
        )?;
        return list_local_skills_inner(&conn);
    }
    if !current_path.join("SKILL.md").is_file() {
        return Err(anyhow!("目标目录缺少 SKILL.md，已拒绝切换状态"));
    }
    if next_path.exists() {
        return Err(anyhow!(
            "目标目录已存在，无法切换状态: {}",
            canonical_display_path(&next_path)
        ));
    }

    if let Some(parent) = next_path.parent() {
        fs::create_dir_all(parent).context("创建本地 skill 状态目录失败")?;
    }
    fs::rename(&current_path, &next_path).context("切换本地 skill 生效状态失败")?;

    let next_display_path = canonical_display_path(&next_path);
    conn.execute(
        "UPDATE local_skills
         SET path = ?1, enabled = ?2, status = ?3, scanned_at = ?4
         WHERE id = ?5",
        params![
            next_display_path,
            if request.enabled { 1_i64 } else { 0_i64 },
            if request.enabled { "local" } else { "disabled" },
            now(),
            request.id
        ],
    )?;

    let audit_ref = skill_id
        .filter(|value| !value.trim().is_empty())
        .or(detected_manifest)
        .unwrap_or_else(|| path.clone());
    let action = if request.enabled {
        "enable_local_skill"
    } else {
        "disable_local_skill"
    };
    let target_scope = format!("{target}:{level}");
    insert_audit(
        &conn,
        action,
        Some(&audit_ref),
        "success",
        Some(&target_scope),
    )?;
    list_local_skills_inner(&conn)
}

fn import_local_skill_to_cache_inner(
    request: ImportLocalSkillRequest,
    state: &AppState,
) -> Result<CachedSkillPackage> {
    let source_path = PathBuf::from(request.path.trim());
    if !source_path.is_dir() {
        return Err(anyhow!("本地 skill 目录不存在"));
    }
    if !source_path.join("SKILL.md").is_file() {
        return Err(anyhow!("本地 skill 目录缺少 SKILL.md"));
    }

    let mut profile = local::read_local_skill_profile(&source_path)?;
    if let Some(skill_id) = request
        .skill_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        profile.skill_id = local::slugify_skill_id(skill_id);
    }
    if let Some(version) = request
        .version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        profile.version = version.to_string();
    }

    let source_display_path = canonical_display_path(&source_path);
    {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let bindings = list_bindings_inner(&conn)?;
        if bindings.iter().any(|binding| {
            canonical_display_path(Path::new(&binding.install_path)) == source_display_path
        }) {
            return Err(anyhow!(
                "该目录已由 Skill Hub 管理，不能作为自建 skill 导入"
            ));
        }

        let market_skills = list_market_skills_inner(&conn)?;
        let classification = local::classify_local_skill(&profile, &market_skills);
        if classification.origin == "market" {
            return Err(anyhow!(
                "该目录匹配市场 skill，请通过市场缓存或恢复管理处理"
            ));
        }

        let cached_packages = list_cached_packages_inner(&conn)?;
        if cached_packages.iter().any(|package| {
            canonical_display_path(Path::new(&package.package_path)) == source_display_path
        }) {
            return Err(anyhow!("该目录已经是 Skill Hub 缓存目录，不能重复导入"));
        }
        if let Some(existing) = cached_packages
            .into_iter()
            .find(|package| local::cached_package_matches_local_profile(package, &profile))
        {
            return Ok(existing);
        }
    }

    let package_dir = state
        .app_dir
        .join("packages")
        .join(format!("{}.{}", local::LOCAL_NAMESPACE, profile.skill_id))
        .join(&profile.version);

    if package_dir.exists() {
        if request.overwrite.unwrap_or(true) {
            package::ensure_safe_package_cache_path(state, &package_dir)?;
            fs::remove_dir_all(&package_dir).context("清理旧本地缓存失败")?;
        } else {
            return Err(anyhow!("本地缓存已存在"));
        }
    }
    fs::create_dir_all(&package_dir).context("创建本地缓存目录失败")?;
    package::copy_dir_recursive_including_json(&source_path, &package_dir)?;

    let conn = state.conn.lock().expect("db mutex poisoned");
    let package_id = ensure_package_record(
        &conn,
        Some(LOCAL_SOURCE_ID),
        local::LOCAL_NAMESPACE,
        &profile.skill_id,
        &profile.version,
        &package_dir,
        None,
    )?;
    let imported_at = now();
    conn.execute(
        "INSERT INTO local_package_metadata
         (package_id, name, summary, tags_json, author, source_path, imported_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(package_id) DO UPDATE SET
           name = excluded.name,
           summary = excluded.summary,
           tags_json = excluded.tags_json,
           author = excluded.author,
           source_path = excluded.source_path,
           imported_at = excluded.imported_at",
        params![
            package_id,
            profile.name,
            profile.summary,
            serde_json::to_string(&profile.tags)?,
            profile.author,
            source_display_path,
            imported_at
        ],
    )?;

    let skill_ref = format!(
        "{}/{}@{}",
        local::LOCAL_NAMESPACE,
        profile.skill_id,
        profile.version
    );
    insert_audit(
        &conn,
        "import_local_cache",
        Some(&skill_ref),
        "success",
        None,
    )?;

    list_cached_packages_inner(&conn)?
        .into_iter()
        .find(|package| {
            package.source_id.as_deref() == Some(LOCAL_SOURCE_ID)
                && package.namespace == local::LOCAL_NAMESPACE
                && package.skill_id == profile.skill_id
                && package.version == profile.version
        })
        .ok_or_else(|| anyhow!("导入后读取本地缓存失败"))
}

fn install_cached_skill_inner(
    request: InstallCachedSkillRequest,
    state: &AppState,
) -> Result<SkillBinding> {
    validation::validate_target(&request.target)?;
    validation::validate_level(&request.level)?;
    if request.source_id.as_deref() != Some(LOCAL_SOURCE_ID)
        || request.namespace != local::LOCAL_NAMESPACE
    {
        return Err(anyhow!("install_cached_skill 仅用于自建本地缓存"));
    }
    if request.level == "project" && request.project_path.as_deref().unwrap_or("").is_empty() {
        return Err(anyhow!("项目级启用必须选择项目目录"));
    }

    let source_id = request.source_id.as_deref();
    let (package_id, package_path, skill_name) = {
        let conn = state.conn.lock().expect("db mutex poisoned");
        if request.enable {
            ensure_scope_can_enable(
                &conn,
                None,
                &request.namespace,
                &request.skill_id,
                &request.target,
                &request.level,
            )?;
            if let Some(existing) = find_same_scope_binding(
                &conn,
                &request.namespace,
                &request.skill_id,
                &request.target,
                &request.level,
                request.project_path.as_deref(),
            )? {
                return Ok(existing);
            }
        }

        conn.query_row(
            "SELECT package.id,
                    package.package_path,
                    COALESCE(local_meta.name, catalog.name, binding.skill_name, package.skill_id)
             FROM skill_packages package
             LEFT JOIN local_package_metadata local_meta
               ON local_meta.package_id = package.id
             LEFT JOIN catalog_cache catalog
               ON COALESCE(catalog.source_id, '') = COALESCE(package.source_id, '')
              AND catalog.namespace = package.namespace
              AND catalog.skill_id = package.skill_id
             LEFT JOIN skill_bindings binding
               ON COALESCE(binding.source_id, '') = COALESCE(package.source_id, '')
              AND binding.namespace = package.namespace
              AND binding.skill_id = package.skill_id
              AND binding.version = package.version
             WHERE COALESCE(package.source_id, '') = COALESCE(?1, '')
               AND package.namespace = ?2
               AND package.skill_id = ?3
               AND package.version = ?4
             LIMIT 1",
            params![
                source_id,
                request.namespace,
                request.skill_id,
                request.version
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| anyhow!("未找到本地缓存包"))?
    };

    let package_path = PathBuf::from(package_path);
    if !package_path.join("SKILL.md").is_file() {
        return Err(anyhow!("本地缓存包无效或缺少 SKILL.md"));
    }

    let install_path = build_install_path(
        state,
        &request.target,
        &request.level,
        request.project_path.as_deref(),
        &request.namespace,
        &request.skill_id,
    )?;
    {
        let conn = state.conn.lock().expect("db mutex poisoned");
        ensure_install_path_not_bound_to_other_skill(
            &conn,
            &install_path,
            &request.namespace,
            &request.skill_id,
        )?;
    }

    if request.enable {
        fs::create_dir_all(&install_path).context("创建安装目录失败")?;
        package::copy_package_to_install_including_json(&package_path, &install_path)?;
    }

    let now = now();
    let id = new_id();
    let install_mode = request.install_mode.unwrap_or_else(|| "copy".to_string());
    let update_policy = request.update_policy.unwrap_or_else(|| {
        if source_id == Some(LOCAL_SOURCE_ID) {
            "pinned".to_string()
        } else {
            "follow_latest".to_string()
        }
    });

    let conn = state.conn.lock().expect("db mutex poisoned");
    conn.execute(
        "INSERT INTO skill_bindings
         (id, package_id, source_id, namespace, skill_id, skill_name, version, target, level,
          project_path, install_path, enabled, install_mode, update_policy, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 'installed', ?15, ?16)",
        params![
            id,
            package_id,
            source_id,
            request.namespace,
            request.skill_id,
            skill_name,
            request.version,
            request.target,
            request.level,
            request.project_path,
            canonical_display_path(&install_path),
            if request.enable { 1_i64 } else { 0_i64 },
            install_mode,
            update_policy,
            now,
            now
        ],
    )?;

    let skill_ref = format!(
        "{}/{}@{}",
        request.namespace, request.skill_id, request.version
    );
    insert_audit(&conn, "install_cached", Some(&skill_ref), "success", None)?;

    list_bindings_inner(&conn)?
        .into_iter()
        .find(|binding| binding.id == id)
        .ok_or_else(|| anyhow!("安装后读取绑定失败"))
}

pub(crate) async fn fetch_manifest_version(
    source: &Source,
    manifest_path: &str,
    version: &str,
) -> Result<SkillVersion> {
    let manifest_url = object_store::object_url(source, manifest_path)?;
    let manifest: SkillManifest = reqwest::Client::new()
        .get(manifest_url)
        .send()
        .await
        .context("请求 skill manifest 失败")?
        .error_for_status()
        .context("skill manifest 响应失败")?
        .json()
        .await
        .context("解析 skill manifest 失败")?;

    manifest
        .versions
        .into_iter()
        .find(|item| item.version == version)
        .ok_or_else(|| anyhow!("manifest 中不存在版本 {version}"))
}

pub(crate) async fn fetch_plugin_manifest_version(
    source: &Source,
    manifest_path: &str,
    version: &str,
) -> Result<PluginVersion> {
    let manifest_url = object_store::object_url(source, manifest_path)?;
    let manifest: PluginManifest = reqwest::Client::new()
        .get(manifest_url)
        .send()
        .await
        .context("PLUGIN_SOURCE_INVALID: request plugin manifest failed")?
        .error_for_status()
        .context("PLUGIN_SOURCE_INVALID: plugin manifest response failed")?
        .json()
        .await
        .context("PLUGIN_SOURCE_INVALID: parse plugin manifest failed")?;

    manifest
        .versions
        .into_iter()
        .find(|item| item.version == version)
        .ok_or_else(|| anyhow!("PLUGIN_SOURCE_INVALID: manifest missing version {version}"))
}

pub(crate) async fn prepare_package(
    state: &AppState,
    source: Option<&Source>,
    skill: &MarketSkill,
    version: &str,
    version_info: Option<&SkillVersion>,
) -> Result<PathBuf> {
    let package_dir = state
        .app_dir
        .join("packages")
        .join(format!("{}.{}", skill.namespace, skill.id))
        .join(version);

    if package_dir.join("SKILL.md").exists() {
        package::remove_json_files_recursive(&package_dir)?;
        return Ok(package_dir);
    }

    let source = source.ok_or_else(|| anyhow!("缺少 MinIO 源，无法下载 skill 包"))?;
    let version_info =
        version_info.ok_or_else(|| anyhow!("缺少远端版本信息，无法下载 skill 包"))?;

    let package_url = object_store::object_url(source, &version_info.package_path)?;
    let bytes = reqwest::Client::new()
        .get(package_url)
        .send()
        .await
        .context("下载 skill 包失败")?
        .error_for_status()
        .context("skill 包响应失败")?
        .bytes()
        .await
        .context("读取 skill 包失败")?;

    if let Some(expected) = version_info
        .package
        .as_ref()
        .map(|package| package.sha256.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        package::verify_sha256(&bytes, expected)?;
    } else {
        let hash_url = object_store::object_url(source, &version_info.sha256_path)?;
        let expected = reqwest::Client::new()
            .get(hash_url)
            .send()
            .await
            .context("下载 sha256 失败")?
            .error_for_status()
            .context("sha256 响应失败")?
            .text()
            .await
            .context("读取 sha256 失败")?;
        package::verify_sha256(&bytes, expected.trim())?;
    }

    fs::create_dir_all(&package_dir)?;
    package::remove_json_files_recursive(&package_dir)?;
    package::extract_zip_safely(&bytes, &package_dir)?;
    Ok(package_dir)
}

pub(crate) async fn prepare_plugin_package(
    state: &AppState,
    source: Option<&Source>,
    plugin: &MarketPlugin,
    version: &str,
    target: &str,
    version_info: Option<&PluginVersion>,
) -> Result<PathBuf> {
    let manifest_file = match target {
        "codex" => ".codex-plugin/plugin.json",
        "claude" => ".claude-plugin/plugin.json",
        _ => return Err(anyhow!("PLUGIN_TARGET_UNSUPPORTED: {target}")),
    };
    let package_dir = state
        .app_dir
        .join("plugin-packages")
        .join(format!("{}.{}", plugin.namespace, plugin.id))
        .join(version)
        .join(target);

    if package_dir.join(manifest_file).exists() {
        return Ok(package_dir);
    }

    let source = source.ok_or_else(|| anyhow!("PLUGIN_SOURCE_INVALID: missing MinIO source"))?;
    let version_info = version_info
        .ok_or_else(|| anyhow!("PLUGIN_SOURCE_INVALID: missing plugin version info"))?;
    let package_ref = plugin_package_ref_for_target(version_info, target)
        .ok_or_else(|| anyhow!("PLUGIN_TARGET_UNSUPPORTED: missing package for {target}"))?;
    let package_url = object_store::object_url(source, &package_ref.package_path)?;
    let bytes = reqwest::Client::new()
        .get(package_url)
        .send()
        .await
        .context("PLUGIN_PACKAGE_BUILD_FAILED: download plugin package failed")?
        .error_for_status()
        .context("PLUGIN_PACKAGE_BUILD_FAILED: plugin package response failed")?
        .bytes()
        .await
        .context("PLUGIN_PACKAGE_BUILD_FAILED: read plugin package failed")?;

    if let Some(expected) = package_ref
        .package
        .as_ref()
        .map(|package| package.sha256.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        package::verify_sha256(&bytes, expected)?;
    } else {
        let hash_url = object_store::object_url(source, &package_ref.sha256_path)?;
        let expected = reqwest::Client::new()
            .get(hash_url)
            .send()
            .await
            .context("PLUGIN_PACKAGE_CHECKSUM_MISMATCH: download sha256 failed")?
            .error_for_status()
            .context("PLUGIN_PACKAGE_CHECKSUM_MISMATCH: sha256 response failed")?
            .text()
            .await
            .context("PLUGIN_PACKAGE_CHECKSUM_MISMATCH: read sha256 failed")?;
        package::verify_sha256(&bytes, expected.trim())?;
    }

    if package_dir.exists() {
        package::ensure_safe_plugin_package_cache_path(state, &package_dir)?;
        fs::remove_dir_all(&package_dir)
            .context("PLUGIN_PACKAGE_BUILD_FAILED: clean old package failed")?;
    }
    fs::create_dir_all(&package_dir)?;
    package::extract_zip_preserving_json_safely(&bytes, &package_dir)?;
    if !package_dir.join(manifest_file).is_file() {
        return Err(anyhow!(
            "PLUGIN_MANIFEST_INVALID: package missing {manifest_file}"
        ));
    }
    Ok(package_dir)
}

fn plugin_package_ref_for_target<'a>(
    version_info: &'a PluginVersion,
    target: &str,
) -> Option<&'a PluginPackageRef> {
    match target {
        "codex" => version_info.packages.codex.as_ref(),
        "claude" => version_info.packages.claude.as_ref(),
        _ => None,
    }
}

async fn plugin_component_inventory_json(
    source: Option<&Source>,
    version_info: Option<&PluginVersion>,
    plugin: &MarketPlugin,
) -> Result<String> {
    let Some(source) = source else {
        return serde_json::to_string(&serde_json::json!({
            "schema": "skillhub.plugin-component-inventory.v1",
            "targets": {},
            "components": plugin.components
        }))
        .map_err(Into::into);
    };
    let Some(path) = version_info.and_then(|info| info.component_inventory_path.as_deref()) else {
        return serde_json::to_string(&serde_json::json!({
            "schema": "skillhub.plugin-component-inventory.v1",
            "targets": {},
            "components": plugin.components
        }))
        .map_err(Into::into);
    };
    let url = object_store::object_url(source, path)?;
    let value: serde_json::Value = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .context("PLUGIN_COMPONENT_SCAN_FAILED: request component inventory failed")?
        .error_for_status()
        .context("PLUGIN_COMPONENT_SCAN_FAILED: component inventory response failed")?
        .json()
        .await
        .context("PLUGIN_COMPONENT_SCAN_FAILED: parse component inventory failed")?;
    serde_json::to_string(&value).map_err(Into::into)
}

#[tauri::command]
pub async fn set_binding_enabled(
    request: SetBindingEnabledRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillBinding> {
    map_result((|| {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let binding = find_binding(&conn, &request.binding_id)?;

        if request.enabled {
            ensure_scope_can_enable(
                &conn,
                Some(&binding.id),
                &binding.namespace,
                &binding.skill_id,
                &binding.target,
                &binding.level,
            )?;
        }

        let install_path = PathBuf::from(&binding.install_path);

        if request.enabled {
            // 启用：从缓存包复制文件到安装目录
            let package_path: String = conn
                .query_row(
                    "SELECT package_path FROM skill_packages WHERE id = ?1",
                    params![binding.package_id],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| anyhow!("未找到缓存包，无法启用。请尝试重新安装。"))?;

            let cache_path = PathBuf::from(&package_path);
            if !cache_path.exists() {
                return Err(anyhow!("缓存包目录不存在: {}", package_path));
            }
            package::copy_package_to_install(&cache_path, &install_path)?;
        } else {
            // 禁用：删除安装目录的文件
            if is_sqlite_managed_install_path(&binding, &install_path) {
                fs::remove_dir_all(&install_path).context("禁用时删除安装目录失败")?;
            }
        }

        conn.execute(
            "UPDATE skill_bindings SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                if request.enabled { 1_i64 } else { 0_i64 },
                now(),
                request.binding_id
            ],
        )?;

        let updated = find_binding(&conn, &binding.id)?;
        let action = if request.enabled { "enable" } else { "disable" };
        let skill_ref = format!("{}/{}", updated.namespace, updated.skill_id);
        insert_audit(&conn, action, Some(&skill_ref), "success", None)?;
        Ok(updated)
    })())
}

#[tauri::command]
pub async fn upgrade_skill_binding(
    request: UpgradeBindingRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        upgrade_skill_binding_inner(request, state.inner()).await?;
        app_bootstrap(state.inner(), None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn upgrade_plugin_binding(
    request: UpgradePluginBindingRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        upgrade_plugin_binding_inner(request, state.inner()).await?;
        refresh_catalog_inner(state.inner()).await?;
        app_bootstrap(state.inner(), None)
    }
    .await;
    map_result(result)
}

async fn upgrade_skill_binding_inner(
    request: UpgradeBindingRequest,
    state: &AppState,
) -> Result<()> {
    // 1. 获取绑定信息
    let binding = {
        let conn = state.conn.lock().expect("db mutex poisoned");
        find_binding(&conn, &request.binding_id)?
    };

    // 2. 获取市场 skill 信息
    let (skill, source) = {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let skills = list_market_skills_inner(&conn)?;
        let skill = skills
            .into_iter()
            .find(|s| s.namespace == binding.namespace && s.id == binding.skill_id)
            .ok_or_else(|| anyhow!("市场中未找到该 skill"))?;

        let source = binding.source_id.as_ref().and_then(|id| {
            list_sources_inner(&conn)
                .ok()?
                .into_iter()
                .find(|item| item.id == *id)
        });

        (skill, source)
    };

    // 3. 检查是否需要升级
    if binding.version == skill.latest_version {
        return Err(anyhow!("已是最新版本"));
    }

    // 4. 获取新版本信息
    let version = &skill.latest_version;
    let version_info = match source.as_ref() {
        Some(src) => Some(fetch_manifest_version(src, &skill.manifest_path, version).await?),
        None => None,
    };

    // 5. 下载新版本到缓存
    let package_path = prepare_package(
        state,
        source.as_ref(),
        &skill,
        version,
        version_info.as_ref(),
    )
    .await?;

    // 6. 更新缓存记录
    let package_id = {
        let conn = state.conn.lock().expect("db mutex poisoned");
        ensure_package_record(
            &conn,
            binding.source_id.as_deref(),
            &binding.namespace,
            &binding.skill_id,
            version,
            &package_path,
            version_info
                .as_ref()
                .and_then(|info| info.package.as_ref().map(|package| package.sha256.as_str())),
        )?
    };

    // 7. 如果已启用，更新安装目录
    if binding.enabled {
        let install_path = PathBuf::from(&binding.install_path);
        fs::create_dir_all(&install_path).context("创建安装目录失败")?;
        package::copy_package_to_install(&package_path, &install_path)?;
    }

    // 8. 更新数据库记录
    let conn = state.conn.lock().expect("db mutex poisoned");
    conn.execute(
        "UPDATE skill_bindings
         SET version = ?1, package_id = ?2, updated_at = ?3
         WHERE id = ?4",
        params![version, package_id, now(), request.binding_id],
    )?;

    let skill_ref = format!("{}/{}@{}", binding.namespace, binding.skill_id, version);
    insert_audit(&conn, "upgrade", Some(&skill_ref), "success", None)?;

    Ok(())
}

async fn upgrade_plugin_binding_inner(
    request: UpgradePluginBindingRequest,
    state: &AppState,
) -> Result<()> {
    let binding = {
        let conn = state.conn.lock().expect("db mutex poisoned");
        find_plugin_binding(&conn, &request.binding_id)?
    };

    let (plugin, source) = {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let plugin = preview::find_market_plugin(
            &conn,
            binding.source_id.as_deref(),
            &binding.namespace,
            &binding.plugin_id,
        )?;
        let source = binding.source_id.as_ref().and_then(|id| {
            list_sources_inner(&conn)
                .ok()?
                .into_iter()
                .find(|item| item.id == *id)
        });
        (plugin, source)
    };

    if binding.version == plugin.latest_version {
        return Err(anyhow!("Plugin 已是最新版本"));
    }
    if binding.update_policy == "pinned" {
        return Err(anyhow!("Plugin 版本已锁定"));
    }

    let version = plugin.latest_version.clone();
    let version_info = match source.as_ref() {
        Some(source) => {
            Some(fetch_plugin_manifest_version(source, &plugin.manifest_path, &version).await?)
        }
        _ => None,
    };
    let package_dir = prepare_plugin_package(
        state,
        source.as_ref(),
        &plugin,
        &version,
        &binding.target,
        version_info.as_ref(),
    )
    .await?;
    let component_inventory_json =
        plugin_component_inventory_json(source.as_ref(), version_info.as_ref(), &plugin).await?;
    let package_id = {
        let conn = state.conn.lock().expect("db mutex poisoned");
        ensure_plugin_package_record(
            &conn,
            binding.source_id.as_deref(),
            &plugin,
            &version,
            &binding.target,
            &package_dir,
            version_info.as_ref().and_then(|info| {
                plugin_package_ref_for_target(info, &binding.target)
                    .and_then(|package_ref| package_ref.package.as_ref())
                    .map(|package| package.sha256.as_str())
            }),
            &component_inventory_json,
        )?
    };

    if binding.enabled {
        validation::validate_plugin_target_scope(&binding.target, &binding.scope)?;
        let marketplace = materialize_plugin_marketplace(
            state,
            &plugin,
            &version,
            &binding.target,
            &binding.scope,
            binding.project_path.as_deref(),
            &package_dir,
        )?;
        sync_codex_plugin_install(
            &binding.target,
            &binding.scope,
            &plugin.id,
            &marketplace.name,
            &marketplace.root_path,
        )?;
        sync_claude_plugin_install(
            &binding.target,
            &plugin.id,
            &marketplace.name,
            &binding.scope,
            binding.project_path.as_deref(),
            &marketplace.root_path,
        )?;
        let conn = state.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "INSERT INTO plugin_marketplaces
             (id, target, scope, project_path, marketplace_name, root_path, marketplace_path, status, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'materialized', ?8)
             ON CONFLICT(target, scope, project_path, marketplace_name) DO UPDATE SET
               root_path = excluded.root_path,
               marketplace_path = excluded.marketplace_path,
               status = excluded.status,
               updated_at = excluded.updated_at",
            params![
                binding.marketplace_id.clone().unwrap_or_else(new_id),
                binding.target,
                binding.scope,
                binding.project_path,
                marketplace.name,
                canonical_display_path(&marketplace.root_path),
                canonical_display_path(&marketplace.marketplace_path),
                now()
            ],
        )?;
    }

    let conn = state.conn.lock().expect("db mutex poisoned");
    conn.execute(
        "UPDATE plugin_bindings
         SET package_id = ?1, version = ?2, plugin_name = ?3, status = 'installed', updated_at = ?4
         WHERE id = ?5",
        params![package_id, version, plugin.name, now(), request.binding_id],
    )?;
    let plugin_ref = format!(
        "{}/{}@{}",
        binding.namespace, binding.plugin_id, plugin.latest_version
    );
    insert_audit(
        &conn,
        "upgrade_plugin",
        Some(&plugin_ref),
        "success",
        Some(&binding.target),
    )?;
    Ok(())
}

#[tauri::command]
pub async fn uninstall_binding(
    binding_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<SkillBinding>> {
    map_result((|| {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let binding = find_binding(&conn, &binding_id)?;

        let path = PathBuf::from(&binding.install_path);
        if is_sqlite_managed_install_path(&binding, &path) {
            fs::remove_dir_all(&path).context("删除安装目录失败")?;
        }

        conn.execute(
            "DELETE FROM skill_bindings WHERE id = ?1",
            params![binding_id],
        )?;
        let skill_ref = format!("{}/{}", binding.namespace, binding.skill_id);
        insert_audit(&conn, "uninstall", Some(&skill_ref), "success", None)?;
        list_bindings_inner(&conn)
    })())
}

#[tauri::command]
pub async fn set_plugin_binding_enabled(
    request: SetPluginBindingEnabledRequest,
    state: State<'_, AppState>,
) -> CommandResult<PluginBinding> {
    map_result(set_plugin_binding_enabled_inner(request, &state))
}

#[tauri::command]
pub async fn uninstall_plugin(
    request: UninstallPluginRequest,
    state: State<'_, AppState>,
) -> CommandResult<Vec<PluginBinding>> {
    map_result(uninstall_plugin_inner(request, &state))
}

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> CommandResult<Vec<Project>> {
    map_result((|| {
        let conn = state.conn.lock().expect("db mutex poisoned");
        list_projects_inner(&conn)
    })())
}

#[tauri::command]
pub async fn save_project(
    request: SaveProjectRequest,
    state: State<'_, AppState>,
) -> CommandResult<Project> {
    map_result((|| {
        let path = request.path.trim();
        if path.is_empty() {
            return Err(anyhow!("项目路径不能为空"));
        }

        let name = if request.name.trim().is_empty() {
            Path::new(path)
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| "未命名项目".to_string())
        } else {
            request.name.trim().to_string()
        };

        let id = request.id.unwrap_or_else(new_id);
        let now = now();
        let conn = state.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "INSERT INTO projects (id, name, path, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
               name = excluded.name,
               updated_at = excluded.updated_at",
            params![id, name, path, now, now],
        )?;

        list_projects_inner(&conn)?
            .into_iter()
            .find(|project| project.path == path)
            .ok_or_else(|| anyhow!("保存项目后读取失败"))
    })())
}

#[tauri::command]
pub async fn unbind_project(
    project_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Project>> {
    map_result((|| {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let project = list_projects_inner(&conn)?
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| anyhow!("未找到项目"))?;

        conn.execute(
            "DELETE FROM skill_bindings WHERE level = 'project' AND project_path = ?1",
            params![&project.path],
        )?;
        conn.execute(
            "DELETE FROM local_skills WHERE level = 'project' AND project_path = ?1",
            params![&project.path],
        )?;
        conn.execute("DELETE FROM projects WHERE id = ?1", params![project_id])?;

        insert_audit(
            &conn,
            "unbind_project",
            None,
            "success",
            Some(&project.path),
        )?;
        list_projects_inner(&conn)
    })())
}

#[tauri::command]
pub async fn scan_local_skills(state: State<'_, AppState>) -> CommandResult<Vec<LocalSkill>> {
    map_result(local::scan_local_skills(&state))
}

#[tauri::command]
pub async fn scan_local_plugins(state: State<'_, AppState>) -> CommandResult<Vec<LocalPlugin>> {
    map_result(local::scan_local_plugins(&state))
}

#[tauri::command]
pub async fn preview_skill(
    request: SkillPreviewRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillPreview> {
    map_result(preview::preview_skill_inner(request, &state).await)
}

#[tauri::command]
pub async fn preview_plugin(
    request: PluginPreviewRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillPreview> {
    map_result(preview::preview_plugin_inner(request, &state).await)
}
#[tauri::command]
pub async fn list_update_candidates(
    state: State<'_, AppState>,
) -> CommandResult<Vec<UpdateCandidate>> {
    let _metadata_sync_error = refresh_catalog_best_effort(&state).await;
    map_result((|| {
        let conn = state.conn.lock().expect("db mutex poisoned");
        list_update_candidates_inner(&conn)
    })())
}

pub(crate) async fn refresh_catalog_best_effort(state: &AppState) -> Option<String> {
    match refresh_catalog_inner(state).await {
        Ok(_) => None,
        Err(err) => Some(err.to_string()),
    }
}

async fn refresh_catalog_inner(state: &AppState) -> Result<Vec<MarketSkill>> {
    let sources = {
        let conn = state.conn.lock().expect("db mutex poisoned");
        enforce_compiled_source(&conn)?;
        list_sources_inner(&conn)?
            .into_iter()
            .filter(|source| source.enabled)
            .collect::<Vec<_>>()
    };

    let client = reqwest::Client::new();
    for source in sources {
        let catalog_url = object_store::object_url(&source, "catalog.v1.json")?;
        let plugin_catalog_url = object_store::object_url(&source, PLUGIN_CATALOG_OBJECT)?;
        let categories_url = object_store::object_url(&source, "categories.v1.json")?;
        let projects_url = object_store::object_url(&source, "projects.v1.json")?;

        let catalog_response = client
            .get(catalog_url)
            .send()
            .await
            .with_context(|| format!("无法连接 MinIO 源 {}", source.endpoint))?;
        let catalog_status = catalog_response.status();
        if catalog_status == reqwest::StatusCode::NOT_FOUND {
            return Err(anyhow!(
                "MinIO bucket 中未找到 catalog.v1.json，请先发布 skill 或上传市场索引"
            ));
        }

        let catalog: CatalogDoc = catalog_response
            .error_for_status()
            .with_context(|| format!("读取 catalog.v1.json 失败: HTTP {catalog_status}"))?
            .json()
            .await
            .context("解析 catalog.v1.json 失败")?;

        let plugin_catalog_doc: Option<PluginCatalogDoc> =
            match client.get(plugin_catalog_url).send().await {
                Ok(response) => {
                    if response.status() == reqwest::StatusCode::NOT_FOUND {
                        None
                    } else {
                        match response.error_for_status() {
                            Ok(ok_response) => ok_response.json().await.ok(),
                            Err(_) => None,
                        }
                    }
                }
                Err(_) => None,
            };

        let categories_doc: Option<CategoriesDoc> = match client.get(categories_url).send().await {
            Ok(response) => match response.error_for_status() {
                Ok(ok_response) => ok_response.json().await.ok(),
                Err(_) => None,
            },
            Err(_) => None,
        };
        let projects_doc: Option<ProjectsDoc> = match client.get(projects_url).send().await {
            Ok(response) => match response.error_for_status() {
                Ok(ok_response) => ok_response.json().await.ok(),
                Err(_) => None,
            },
            Err(_) => None,
        };
        let derived_public_categories = public_categories_from_catalog(&catalog);

        let conn = state.conn.lock().expect("db mutex poisoned");
        conn.execute("DELETE FROM categories", [])?;
        if let Some(doc) = categories_doc {
            upsert_categories(&conn, normalize_categories_doc(doc).items)?;
        }
        ensure_missing_categories(&conn, derived_public_categories)?;
        drop(conn);

        if let Some(doc) = projects_doc {
            fs::write(
                market_project_cache_path(&state.app_dir),
                serde_json::to_string_pretty(&doc)?,
            )?;
        }

        let conn = state.conn.lock().expect("db mutex poisoned");
        conn.execute(
            "DELETE FROM catalog_cache WHERE source_id = ?1",
            params![source.id.as_str()],
        )?;
        conn.execute(
            "DELETE FROM plugin_catalog_cache WHERE source_id = ?1",
            params![source.id.as_str()],
        )?;

        for mut skill in catalog.skills {
            skill.source_id = Some(source.id.clone());
            skill.targets.clear();
            conn.execute(
                "INSERT INTO catalog_cache
                 (source_id, namespace, skill_id, latest_version, name, summary, categories_json,
                  tags_json, targets_json, levels_json, manifest_path, raw_manifest, etag, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL, ?13)
                 ON CONFLICT(source_id, namespace, skill_id) DO UPDATE SET
                   latest_version = excluded.latest_version,
                   name = excluded.name,
                   summary = excluded.summary,
                   categories_json = excluded.categories_json,
                   tags_json = excluded.tags_json,
                   targets_json = excluded.targets_json,
                   levels_json = excluded.levels_json,
                   manifest_path = excluded.manifest_path,
                   raw_manifest = excluded.raw_manifest,
                   updated_at = excluded.updated_at",
                params![
                    source.id.as_str(),
                    skill.namespace,
                    skill.id,
                    skill.latest_version,
                    skill.name,
                    skill.summary,
                    serde_json::to_string(&skill.categories)?,
                    serde_json::to_string(&skill.tags)?,
                    serde_json::to_string(&skill.targets)?,
                    serde_json::to_string(&skill.levels)?,
                    skill.manifest_path,
                    serde_json::to_string(&skill)?,
                    now()
                ],
            )?;
        }

        if let Some(plugin_catalog) = plugin_catalog_doc {
            for mut plugin in plugin_catalog.plugins {
                plugin.source_id = Some(source.id.clone());
                conn.execute(
                    "INSERT INTO plugin_catalog_cache
                     (source_id, namespace, plugin_id, latest_version, name, summary, categories_json,
                      tags_json, targets_json, scopes_json, components_json, risk_level, manifest_path,
                      raw_manifest, etag, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL, ?15)
                     ON CONFLICT(source_id, namespace, plugin_id) DO UPDATE SET
                       latest_version = excluded.latest_version,
                       name = excluded.name,
                       summary = excluded.summary,
                       categories_json = excluded.categories_json,
                       tags_json = excluded.tags_json,
                       targets_json = excluded.targets_json,
                       scopes_json = excluded.scopes_json,
                       components_json = excluded.components_json,
                       risk_level = excluded.risk_level,
                       manifest_path = excluded.manifest_path,
                       raw_manifest = excluded.raw_manifest,
                       updated_at = excluded.updated_at",
                    params![
                        source.id.as_str(),
                        plugin.namespace,
                        plugin.id,
                        plugin.latest_version,
                        plugin.name,
                        plugin.summary,
                        serde_json::to_string(&plugin.categories)?,
                        serde_json::to_string(&plugin.tags)?,
                        serde_json::to_string(&plugin.targets)?,
                        serde_json::to_string(&plugin.scopes)?,
                        serde_json::to_string(&plugin.components)?,
                        plugin.risk_level,
                        plugin.manifest_path,
                        serde_json::to_string(&plugin)?,
                        now()
                    ],
                )?;
            }
        }

        conn.execute(
            "UPDATE sources SET last_sync_at = ?1 WHERE id = ?2",
            params![now(), source.id.as_str()],
        )?;
    }

    let conn = state.conn.lock().expect("db mutex poisoned");
    list_market_skills_inner(&conn)
}

fn upsert_categories(conn: &rusqlite::Connection, categories: Vec<Category>) -> Result<()> {
    for category in categories {
        conn.execute(
            "INSERT INTO categories (id, name, ordering)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               ordering = excluded.ordering",
            params![category.id, category.name, category.order],
        )?;
    }

    Ok(())
}

fn public_categories_from_catalog(catalog: &CatalogDoc) -> Vec<Category> {
    catalog
        .categories
        .iter()
        .filter(|category| !category.starts_with("project:"))
        .enumerate()
        .map(|(index, category)| Category {
            id: category.clone(),
            name: category_name_from_slug(category),
            order: 10 + index as i64 * 10,
        })
        .collect()
}

fn ensure_missing_categories(conn: &rusqlite::Connection, categories: Vec<Category>) -> Result<()> {
    for category in normalize_categories_doc(CategoriesDoc {
        schema: "skillhub.categories.v1".to_string(),
        generated_at: None,
        items: categories,
    })
    .items
    {
        conn.execute(
            "INSERT OR IGNORE INTO categories (id, name, ordering) VALUES (?1, ?2, ?3)",
            params![category.id, category.name, category.order],
        )?;
    }

    Ok(())
}

fn normalize_categories_doc(mut doc: CategoriesDoc) -> CategoriesDoc {
    let mut by_id = BTreeMap::new();
    for mut category in doc.items {
        category.id = category.id.trim().to_string();
        if !validation::is_valid_object_segment_value(&category.id)
            || category.id.starts_with("project:")
        {
            continue;
        }
        category.name = category.name.trim().to_string();
        if category.name.is_empty() {
            category.name = category_name_from_slug(&category.id);
        }
        by_id.insert(category.id.clone(), category);
    }

    let mut items = by_id.into_values().collect::<Vec<_>>();
    items.sort_by(|a, b| {
        a.order
            .cmp(&b.order)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.name.cmp(&b.name))
    });

    let mut next_order = 10;
    for item in &mut items {
        if item.order < next_order {
            item.order = next_order;
        }
        next_order = item.order + 10;
    }

    items.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));

    doc.items = items;
    doc
}

fn category_name_from_slug(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
#[derive(Debug, Clone, Default)]
pub(crate) struct DraftSkillMetadata {
    pub(crate) name: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) version: Option<String>,
    pub(crate) author: Option<String>,
}

fn default_publish_meta_from_draft(
    source_path: &str,
    metadata: &DraftSkillMetadata,
) -> PublishMeta {
    let skill_id = draft_skill_id_from_source_path(source_path);
    PublishMeta {
        namespace: FIXED_PUBLISH_NAMESPACE.to_string(),
        skill_id: skill_id.clone(),
        version: metadata.version.clone(),
        name: metadata.name.clone().unwrap_or_else(|| skill_id.clone()),
        summary: metadata.description.clone().unwrap_or_default(),
        tags: metadata.tags.clone(),
        targets: Vec::new(),
        levels: vec!["personal".to_string(), "project".to_string()],
        publish_scope: "public".to_string(),
        publish_category_slug: None,
        publish_project_slug: None,
        changelog: String::new(),
        updated_at: None,
        updated_by: None,
    }
}

fn merge_publish_meta_defaults(mut meta: PublishMeta, defaults: PublishMeta) -> PublishMeta {
    meta.namespace = defaults.namespace;
    meta.skill_id = defaults.skill_id;
    if meta.name.trim().is_empty() {
        meta.name = defaults.name;
    }
    if meta.summary.trim().is_empty() {
        meta.summary = defaults.summary;
    }
    if meta.tags.is_empty() {
        meta.tags = defaults.tags;
    }
    meta
}

fn normalize_publish_meta_for_source(meta: PublishMeta, source_path: &str) -> PublishMeta {
    let mut meta = normalize_publish_meta(meta);
    meta.namespace = FIXED_PUBLISH_NAMESPACE.to_string();
    meta.skill_id = draft_skill_id_from_source_path(source_path);
    meta
}

fn default_plugin_publish_meta(meta: &PluginSourceMeta) -> PublishMeta {
    PublishMeta {
        namespace: meta.namespace.clone(),
        skill_id: meta.id.clone(),
        version: Some(meta.version.clone()).filter(|value| !value.trim().is_empty()),
        name: meta.name.clone(),
        summary: meta.summary.clone(),
        tags: meta.tags.clone(),
        targets: meta.targets.clone(),
        levels: meta.scopes.clone(),
        publish_scope: "public".to_string(),
        publish_category_slug: None,
        publish_project_slug: None,
        changelog: String::new(),
        updated_at: None,
        updated_by: None,
    }
}

fn default_plugin_publish_meta_from_readme(
    source_path: &str,
    metadata: &DraftSkillMetadata,
) -> PublishMeta {
    let plugin_id = draft_skill_id_from_source_path(source_path);
    PublishMeta {
        namespace: FIXED_PUBLISH_NAMESPACE.to_string(),
        skill_id: plugin_id.clone(),
        version: metadata.version.clone(),
        name: metadata.name.clone().unwrap_or_else(|| plugin_id.clone()),
        summary: metadata.description.clone().unwrap_or_default(),
        tags: metadata.tags.clone(),
        targets: Vec::new(),
        levels: vec!["user".to_string(), "project".to_string()],
        publish_scope: "public".to_string(),
        publish_category_slug: None,
        publish_project_slug: None,
        changelog: String::new(),
        updated_at: None,
        updated_by: None,
    }
}

fn merge_plugin_readme_defaults(
    mut defaults: PublishMeta,
    readme_defaults: Option<PublishMeta>,
) -> PublishMeta {
    let Some(readme_defaults) = readme_defaults else {
        return defaults;
    };
    if let Some(version) = readme_defaults
        .version
        .filter(|value| !value.trim().is_empty())
    {
        defaults.version = Some(version);
    }
    if !readme_defaults.name.trim().is_empty() {
        defaults.name = readme_defaults.name;
    }
    if !readme_defaults.summary.trim().is_empty() {
        defaults.summary = readme_defaults.summary;
    }
    if !readme_defaults.tags.is_empty() {
        defaults.tags = readme_defaults.tags;
    }
    defaults
}

fn normalize_plugin_publish_meta(meta: PublishMeta, defaults: Option<&PublishMeta>) -> PublishMeta {
    let mut meta = normalize_publish_meta(meta);
    meta.levels = meta
        .levels
        .into_iter()
        .map(|level| match level.trim() {
            "personal" => "user".to_string(),
            value => value.to_string(),
        })
        .filter(|level| !level.is_empty())
        .collect();
    if let Some(defaults) = defaults {
        meta.namespace = defaults.namespace.clone();
        meta.skill_id = defaults.skill_id.clone();
        if meta
            .version
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            meta.version = defaults.version.clone();
        }
        if meta.name.trim().is_empty() {
            meta.name = defaults.name.clone();
        }
        if meta.summary.trim().is_empty() {
            meta.summary = defaults.summary.clone();
        }
        if meta.tags.is_empty() {
            meta.tags = defaults.tags.clone();
        }
        if meta.targets.is_empty() {
            meta.targets = defaults.targets.clone();
        }
        if meta.levels.is_empty() {
            meta.levels = defaults.levels.clone();
        }
        if meta.publish_scope.trim().is_empty() {
            meta.publish_scope = defaults.publish_scope.clone();
        }
        if meta.publish_scope == "project" && meta.publish_project_slug.is_none() {
            meta.publish_project_slug = defaults.publish_project_slug.clone();
        }
        if meta.publish_scope != "project" && meta.publish_category_slug.is_none() {
            meta.publish_category_slug = defaults.publish_category_slug.clone();
        }
    }
    if meta.levels.is_empty() {
        meta.levels = vec!["user".to_string(), "project".to_string()];
    }
    meta
}

fn apply_plugin_publish_meta(
    mut source: PluginSourceMeta,
    saved_meta: Option<PublishMeta>,
) -> PluginSourceMeta {
    let defaults = default_plugin_publish_meta(&source);
    let saved_identity = saved_meta.as_ref().map(|meta| {
        (
            meta.namespace.trim().to_string(),
            meta.skill_id.trim().to_string(),
        )
    });
    let meta = normalize_plugin_publish_meta(
        saved_meta.unwrap_or_else(|| defaults.clone()),
        Some(&defaults),
    );
    let mut meta = meta;
    if let Some((namespace, skill_id)) = saved_identity {
        if !namespace.is_empty() {
            meta.namespace = namespace;
        }
        if !skill_id.is_empty() {
            meta.skill_id = skill_id;
        }
    }
    if let Some(version) = meta
        .version
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        source.version = version;
    }
    source.name = meta.name;
    source.summary = meta.summary;
    source.namespace = meta.namespace;
    source.id = meta.skill_id;
    source.author = source.author.filter(|value| !value.trim().is_empty());
    source.tags = meta.tags;
    source.targets = meta.targets;
    source.scopes = meta.levels;
    source.publish_scope = Some(meta.publish_scope.clone());
    source.publish_project_slug = meta.publish_project_slug;
    source.categories = if meta.publish_scope == "project" {
        Vec::new()
    } else {
        meta.publish_category_slug.into_iter().collect()
    };
    source
}

fn apply_plugin_readme_metadata(
    mut source: PluginSourceMeta,
    metadata: &DraftSkillMetadata,
) -> Result<PluginSourceMeta> {
    let name = required_plugin_readme_field(metadata.name.as_deref(), "name")?;
    let summary = required_plugin_readme_field(metadata.description.as_deref(), "description")?;
    let version = required_plugin_readme_field(metadata.version.as_deref(), "version")?;
    let author = required_plugin_readme_field(metadata.author.as_deref(), "author")?;
    source.name = name;
    source.summary = summary;
    source.version = version;
    source.author = Some(author);
    if !metadata.tags.is_empty() {
        source.tags = metadata.tags.clone();
    }
    Ok(source)
}

fn plugin_source_meta_from_readme(
    metadata: &DraftSkillMetadata,
    saved_meta: Option<PublishMeta>,
    files: &[(String, Vec<u8>)],
) -> Result<PluginSourceMeta> {
    let name = required_plugin_readme_field(metadata.name.as_deref(), "name")?;
    let summary = required_plugin_readme_field(metadata.description.as_deref(), "description")?;
    let version = required_plugin_readme_field(metadata.version.as_deref(), "version")?;
    let author = required_plugin_readme_field(metadata.author.as_deref(), "author")?;
    let saved_identity = saved_meta.as_ref().map(|meta| {
        (
            meta.namespace.trim().to_string(),
            meta.skill_id.trim().to_string(),
        )
    });
    let defaults = PublishMeta {
        namespace: FIXED_PUBLISH_NAMESPACE.to_string(),
        skill_id: saved_meta
            .as_ref()
            .map(|meta| meta.skill_id.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| plugin_id_from_readme_name(&name)),
        version: Some(version.clone()),
        name: name.clone(),
        summary: summary.clone(),
        tags: metadata.tags.clone(),
        targets: Vec::new(),
        levels: vec!["user".to_string(), "project".to_string()],
        publish_scope: "public".to_string(),
        publish_category_slug: None,
        publish_project_slug: None,
        changelog: String::new(),
        updated_at: None,
        updated_by: None,
    };
    let meta = normalize_plugin_publish_meta(
        saved_meta.unwrap_or_else(|| defaults.clone()),
        Some(&defaults),
    );
    let mut meta = meta;
    if let Some((namespace, skill_id)) = saved_identity {
        if !namespace.is_empty() {
            meta.namespace = namespace;
        }
        if !skill_id.is_empty() {
            meta.skill_id = skill_id;
        }
    }
    validation::validate_publish_meta(&meta)?;
    let mut source = PluginSourceMeta {
        schema: "skillhub.plugin-source.v1".to_string(),
        namespace: meta.namespace,
        id: meta.skill_id,
        version,
        name: meta.name,
        summary: meta.summary,
        author: Some(author),
        categories: if meta.publish_scope == "project" {
            Vec::new()
        } else {
            meta.publish_category_slug.into_iter().collect()
        },
        tags: meta.tags,
        targets: meta.targets,
        scopes: meta.levels,
        components: infer_plugin_components(files),
        risk_level: None,
        publish_scope: Some(meta.publish_scope.clone()),
        publish_project_slug: meta.publish_project_slug,
        platforms: serde_json::Value::Null,
    };
    if source.scopes.is_empty() {
        source.scopes = vec!["user".to_string(), "project".to_string()];
    }
    validate_plugin_source_meta(&source)?;
    Ok(source)
}

fn required_plugin_readme_field(value: Option<&str>, field: &str) -> Result<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("PLUGIN_SOURCE_INVALID: README.md 缺少 {field}"))
}

fn is_plugin_readme_metadata_complete(metadata: &DraftSkillMetadata) -> bool {
    metadata
        .name
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && metadata
            .description
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        && metadata
            .version
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        && metadata
            .author
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
}

fn plugin_id_from_readme_name(name: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in name.trim().chars() {
        let mapped = if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
            Some(ch.to_ascii_lowercase())
        } else if ch == '-' || ch.is_whitespace() {
            Some('-')
        } else {
            None
        };
        let Some(mapped) = mapped else {
            continue;
        };
        if mapped == '-' {
            if last_was_dash {
                continue;
            }
            last_was_dash = true;
        } else {
            last_was_dash = false;
        }
        slug.push(mapped);
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "plugin".to_string()
    } else {
        slug
    }
}

fn draft_skill_id_from_source_path(source_path: &str) -> String {
    parse_gitlab_source_path(source_path)
        .draft_slug
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "skill".to_string())
}

pub(crate) fn parse_skill_frontmatter(content: &str) -> DraftSkillMetadata {
    let mut metadata = DraftSkillMetadata::default();
    let mut metadata_tags = Vec::new();
    let mut metadata_tags_present = false;
    let mut tag_list_source: Option<bool> = None;
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return metadata;
    }

    let mut section: Option<String> = None;
    for line in lines.take(120) {
        let trimmed = line.trim();
        if trimmed == "---" || trimmed == "..." {
            break;
        }
        if trimmed.is_empty() {
            continue;
        }

        let indent = line.chars().take_while(|ch| ch.is_whitespace()).count();
        if let Some(item) = trimmed.strip_prefix("- ") {
            if let Some(is_metadata_tags) = tag_list_source {
                if let Some(tag) = local::clean_frontmatter_value(item) {
                    if is_metadata_tags {
                        metadata_tags_present = true;
                        push_unique_tag(&mut metadata_tags, tag);
                    } else {
                        push_unique_tag(&mut metadata.tags, tag);
                    }
                }
            }
            continue;
        }
        tag_list_source = None;

        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        if indent == 0 {
            section = Some(key.clone());
        }

        match (indent == 0, section.as_deref(), key.as_str()) {
            (true, _, "name") => metadata.name = local::clean_frontmatter_value(value),
            (true, _, "description") => {
                metadata.description = local::clean_frontmatter_value(value)
            }
            (true, _, "tags") => {
                for tag in parse_frontmatter_tags(value) {
                    push_unique_tag(&mut metadata.tags, tag);
                }
                tag_list_source = Some(false);
            }
            (true, _, "version") => metadata.version = local::clean_frontmatter_value(value),
            (true, _, "author") => metadata.author = local::clean_frontmatter_value(value),
            (false, Some("metadata"), "version") => {
                metadata.version = local::clean_frontmatter_value(value)
            }
            (false, Some("metadata"), "author") => {
                metadata.author = local::clean_frontmatter_value(value)
            }
            (false, Some("metadata"), "tags") => {
                metadata_tags_present = true;
                for tag in parse_frontmatter_tags(value) {
                    push_unique_tag(&mut metadata_tags, tag);
                }
                tag_list_source = Some(true);
            }
            _ => {}
        }
    }

    if metadata_tags_present {
        metadata.tags = metadata_tags;
    }

    metadata
}

fn parse_frontmatter_tags(value: &str) -> Vec<String> {
    let Some(cleaned) = local::clean_frontmatter_value(value) else {
        return Vec::new();
    };
    let inner = cleaned
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    if inner.is_empty() {
        return Vec::new();
    }
    if inner.contains(',') {
        inner
            .split(',')
            .filter_map(local::clean_frontmatter_value)
            .collect()
    } else {
        vec![inner.to_string()]
    }
}

fn push_unique_tag(tags: &mut Vec<String>, tag: String) {
    if !tags.iter().any(|item| item.eq_ignore_ascii_case(&tag)) {
        tags.push(tag);
    }
}

fn parse_skill_markdown_field(content: &str, field: &str) -> Option<String> {
    let expected = field.trim();
    for line in content.lines().take(80) {
        let trimmed = line.trim();
        if trimmed == "---" || trimmed.is_empty() {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case(expected) {
            return local::clean_frontmatter_value(value);
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitlabDraftLocation {
    category_path: Vec<String>,
    draft_slug: Option<String>,
}

impl GitlabDraftLocation {
    fn category_code(&self) -> Option<String> {
        if self.category_path.is_empty() {
            None
        } else {
            Some(self.category_path.join("/"))
        }
    }
}

fn parse_gitlab_source_path(source_path: &str) -> GitlabDraftLocation {
    let mut parts = source_path
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let draft_slug = parts.pop();
    GitlabDraftLocation {
        category_path: parts,
        draft_slug,
    }
}

fn non_empty_string(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn plugin_draft_status(
    source_available: bool,
    readme_metadata_complete: bool,
    version: Option<&str>,
    published_version: Option<&str>,
    state_status: Option<&str>,
    targets: &[String],
    meta: Option<&PublishMeta>,
    validation_status: Option<&str>,
) -> String {
    if state_status.is_some_and(|value| value.eq_ignore_ascii_case("archived")) {
        return "archived".to_string();
    }
    if validation_failed(validation_status) {
        return "validation_failed".to_string();
    }
    if !source_available {
        return "source_missing".to_string();
    }
    if !readme_metadata_complete {
        return "metadata_incomplete".to_string();
    }
    if version.is_none() || targets.is_empty() {
        return "metadata_incomplete".to_string();
    }
    if match meta {
        Some(value) => !validation::is_publish_meta_ready_for_status(value),
        None => true,
    } {
        return "metadata_incomplete".to_string();
    }
    if published_version == version {
        return "published".to_string();
    }
    "ready_to_publish".to_string()
}

fn draft_status(
    version: Option<&str>,
    published_version: Option<&str>,
    state_status: Option<&str>,
    meta: Option<&PublishMeta>,
    validation_status: Option<&str>,
) -> String {
    if state_status.is_some_and(|value| value.eq_ignore_ascii_case("archived")) {
        return "已下架".to_string();
    }
    if validation_failed(validation_status) {
        return "校验失败".to_string();
    }
    if version.is_none() {
        return "校验失败".to_string();
    }
    if match meta {
        Some(value) => !validation::is_publish_meta_ready_for_status(value),
        None => true,
    } {
        return "元数据待补充".to_string();
    }
    match published_version {
        Some(published) if Some(published) == version => "已发布".to_string(),
        Some(published) if version.is_some_and(|value| value > published) => "可升级".to_string(),
        Some(published) if version.is_some_and(|value| value < published) => {
            "版本回退风险".to_string()
        }
        _ => "待发布".to_string(),
    }
}

fn validation_status_from_json(value: &serde_json::Value) -> Option<String> {
    value
        .get("status")
        .and_then(|status| status.as_str())
        .map(|status| status.trim().to_ascii_lowercase())
}

fn validation_failed(status: Option<&str>) -> bool {
    status
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "" | "passed" | "ok" | "success"
            )
        })
        .unwrap_or(false)
}

fn normalize_publish_meta(mut meta: PublishMeta) -> PublishMeta {
    meta.namespace = meta.namespace.trim().to_string();
    meta.skill_id = meta.skill_id.trim().to_string();
    meta.name = meta.name.trim().to_string();
    meta.summary = meta.summary.trim().to_string();
    meta.publish_scope = match meta.publish_scope.trim() {
        "project" => "project".to_string(),
        _ => "public".to_string(),
    };
    if meta.levels.is_empty() {
        meta.levels = vec!["personal".to_string(), "project".to_string()];
    }
    meta
}

async fn validate_publish_target(
    client: &object_store::AdminObjectClient,
    meta: &PublishMeta,
) -> Result<()> {
    if meta.publish_scope == "project" {
        let project_slug = meta.publish_project_slug.as_deref().unwrap_or("");
        let projects = load_remote_projects(client).await?;
        if projects.iter().any(|project| project.slug == project_slug) {
            Ok(())
        } else {
            Err(anyhow!("发布项目不存在: {project_slug}"))
        }
    } else {
        Ok(())
    }
}

fn ensure_system_admin(authorization: &admin_config::AdminAuthorization) -> Result<()> {
    if authorization.is_system() {
        Ok(())
    } else {
        Err(anyhow!("该操作需要系统管理员权限"))
    }
}

fn ensure_can_view_admin_audit(authorization: &admin_config::AdminAuthorization) -> Result<()> {
    if authorization.is_system() {
        Ok(())
    } else {
        Err(anyhow!("审计日志仅系统管理员可查看"))
    }
}

fn ensure_can_manage_project(
    authorization: &admin_config::AdminAuthorization,
    project_slug: &str,
) -> Result<()> {
    if authorization.can_manage_project(project_slug) {
        Ok(())
    } else {
        Err(anyhow!("未授权管理项目: {project_slug}"))
    }
}

fn ensure_can_manage_publish_target(
    authorization: &admin_config::AdminAuthorization,
    meta: &PublishMeta,
) -> Result<()> {
    if meta.publish_scope == "project" {
        ensure_can_manage_project(
            authorization,
            meta.publish_project_slug.as_deref().unwrap_or(""),
        )
    } else {
        ensure_system_admin(authorization)
    }
}

fn ensure_can_manage_skill_categories(
    authorization: &admin_config::AdminAuthorization,
    categories: &[String],
) -> Result<()> {
    if categories.is_empty() {
        return ensure_system_admin(authorization);
    }

    for category in categories {
        if let Some(project_slug) = category.strip_prefix("project:") {
            ensure_can_manage_project(authorization, project_slug)?;
        } else {
            ensure_system_admin(authorization)?;
        }
    }
    Ok(())
}

fn admin_actor(authorization: &admin_config::AdminAuthorization) -> String {
    authorization
        .name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| authorization.mac_address.clone())
}

fn admin_audit_envelope(
    action: &str,
    authorization: &admin_config::AdminAuthorization,
    payload: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "schema": "skillhub.admin-audit.v1",
        "action": action,
        "actor": admin_actor(authorization),
        "role": authorization.role.clone(),
        "macAddress": authorization.mac_address.clone(),
        "ipAddress": serde_json::Value::Null,
        "payload": payload,
        "createdAt": now()
    })
}

fn admin_audit_log_from_value(
    object_path: &str,
    value: serde_json::Value,
) -> Result<AdminAuditLog> {
    let payload = value
        .get("payload")
        .cloned()
        .unwrap_or_else(|| value.clone());
    let action = audit_field_string(&value, &payload, &["action"])
        .unwrap_or_else(|| audit_action_from_path(object_path));
    let actor = audit_field_string(&value, &payload, &["actor"]);
    let role = audit_field_string(&value, &payload, &["role"]);
    let mac_address = audit_field_string(&value, &payload, &["macAddress", "mac_address", "mac"]);
    let ip_address = audit_field_string(&value, &payload, &["ipAddress", "ip_address", "ip"]);
    let target = admin_audit_target(&action, &value, &payload);
    let summary = admin_audit_summary(&action, target.as_deref());
    let created_at = audit_field_string(&value, &payload, &["createdAt", "created_at"])
        .unwrap_or_else(|| audit_date_from_path(object_path).unwrap_or_else(now));

    Ok(AdminAuditLog {
        object_path: object_path.to_string(),
        action,
        actor,
        role,
        mac_address,
        ip_address,
        target,
        summary,
        created_at,
        payload,
    })
}

fn audit_field_string(
    value: &serde_json::Value,
    payload: &serde_json::Value,
    keys: &[&str],
) -> Option<String> {
    keys.iter()
        .find_map(|key| audit_value_string(value, key).or_else(|| audit_value_string(payload, key)))
}

fn audit_value_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
}

fn admin_audit_target(
    action: &str,
    value: &serde_json::Value,
    payload: &serde_json::Value,
) -> Option<String> {
    match action {
        "saveMarketProject" | "deleteMarketProject" => {
            audit_field_string(value, payload, &["slug", "projectSlug", "project_slug"])
        }
        "saveMarketCategory" | "deleteMarketCategory" => {
            audit_field_string(value, payload, &["categoryId", "category_id", "id"]).or_else(|| {
                payload.get("category").and_then(|category| {
                    audit_value_string(category, "id")
                        .or_else(|| audit_value_string(category, "categoryId"))
                })
            })
        }
        "savePublishMeta" | "savePluginPublishMeta" => {
            audit_field_string(value, payload, &["gitlabSourcePath", "gitlab_source_path"])
                .or_else(|| namespace_artifact_target(payload))
        }
        "publishDraft" | "publishPluginDraft" => value
            .get("state")
            .and_then(namespace_artifact_target)
            .or_else(|| payload.get("state").and_then(namespace_artifact_target))
            .or_else(|| namespace_artifact_target(payload)),
        "quickRepublishArchivedSkill" | "archiveMarketSkill" | "archiveMarketPlugin" => {
            namespace_artifact_target(payload)
        }
        _ => audit_field_string(
            value,
            payload,
            &[
                "target",
                "slug",
                "categoryId",
                "category_id",
                "gitlabSourcePath",
                "gitlab_source_path",
            ],
        )
        .or_else(|| namespace_artifact_target(payload)),
    }
}

fn namespace_artifact_target(value: &serde_json::Value) -> Option<String> {
    let namespace = audit_value_string(value, "namespace")?;
    let artifact_id = audit_value_string(value, "skillId")
        .or_else(|| audit_value_string(value, "skill_id"))
        .or_else(|| audit_value_string(value, "pluginId"))
        .or_else(|| audit_value_string(value, "plugin_id"))?;
    let base = format!("{namespace}/{artifact_id}");
    let version = audit_value_string(value, "version")
        .or_else(|| audit_value_string(value, "publishedVersion"))
        .or_else(|| audit_value_string(value, "published_version"));
    Some(match version {
        Some(version) => format!("{base}@{version}"),
        None => base,
    })
}

fn admin_audit_summary(action: &str, target: Option<&str>) -> String {
    let label = match action {
        "savePublishMeta" => "保存发布元数据",
        "savePluginPublishMeta" => "保存 Plugin 发布元数据",
        "saveMarketProject" => "保存项目",
        "deleteMarketProject" => "删除项目",
        "saveMarketCategory" => "保存公共分类",
        "deleteMarketCategory" => "删除公共分类",
        "publishDraft" => "发布草稿",
        "publishPluginDraft" => "发布 Plugin 草稿",
        "quickRepublishArchivedSkill" => "快速重新上架",
        "archiveMarketSkill" => "下架 skill",
        "archiveMarketPlugin" => "下架 plugin",
        other => other,
    };
    match target {
        Some(target) => format!("{label}: {target}"),
        None => label.to_string(),
    }
}

fn audit_action_from_path(object_path: &str) -> String {
    object_path
        .rsplit('/')
        .next()
        .and_then(|file| {
            file.strip_suffix(".json")
                .unwrap_or(file)
                .rsplit_once('-')
                .map(|(action, _)| action)
        })
        .filter(|action| !action.trim().is_empty())
        .unwrap_or("unknown")
        .to_string()
}

fn audit_date_from_path(object_path: &str) -> Option<String> {
    let mut parts = object_path.split('/');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("admin"), Some("audit"), Some(year), Some(month)) => {
            let day = parts.next()?;
            Some(format!("{year}-{month}-{day}T00:00:00Z"))
        }
        _ => None,
    }
}

fn should_republish_existing_version(
    manifest: &SkillManifest,
    catalog: &CatalogDoc,
    meta: &PublishMeta,
    version: &str,
) -> Result<bool> {
    let version_exists = manifest.versions.iter().any(|item| item.version == version);
    let already_in_catalog = catalog
        .skills
        .iter()
        .any(|skill| skill.namespace == meta.namespace && skill.id == meta.skill_id);
    if version_exists && already_in_catalog {
        Err(anyhow!(
            "版本已在市场中，禁止重复发布: {}@{}",
            meta.skill_id,
            version
        ))
    } else {
        Ok(version_exists)
    }
}

fn admin_object_path(source_path: &str, leaf: &str) -> Result<String> {
    Ok(format!(
        "{}{}/{}",
        DRAFT_ADMIN_PREFIX,
        validation::normalize_relative_object_path(source_path)?,
        leaf
    ))
}

fn plugin_admin_object_path(source_path: &str, leaf: &str) -> Result<String> {
    Ok(format!(
        "{}{}/{}",
        PLUGIN_ADMIN_PREFIX,
        validation::normalize_relative_object_path(source_path)?,
        leaf
    ))
}

fn publish_categories(meta: &PublishMeta) -> Vec<String> {
    if meta.publish_scope == "project" {
        vec![format!(
            "project:{}",
            meta.publish_project_slug.as_deref().unwrap_or("")
        )]
    } else {
        vec![meta.publish_category_slug.clone().unwrap_or_default()]
    }
}

fn rebuild_catalog_categories(skills: &[MarketSkill]) -> Vec<String> {
    let mut categories = skills
        .iter()
        .flat_map(|skill| skill.categories.iter().cloned())
        .collect::<Vec<_>>();
    categories.sort();
    categories.dedup();
    categories
}

fn merge_categories(mut first: Vec<String>, second: Vec<String>) -> Vec<String> {
    first.extend(second);
    first.sort();
    first.dedup();
    first
}

fn plugin_publish_categories(meta: &PluginSourceMeta) -> Vec<String> {
    if meta.publish_scope.as_deref() == Some("project") {
        vec![format!(
            "project:{}",
            meta.publish_project_slug.as_deref().unwrap_or("")
        )]
    } else if meta.categories.is_empty() {
        vec!["uncategorized".to_string()]
    } else {
        meta.categories.clone()
    }
}

fn plugin_publish_category_slug(meta: &PluginSourceMeta) -> Option<String> {
    if meta.publish_scope.as_deref() == Some("project") {
        None
    } else {
        meta.categories.first().cloned()
    }
}

fn build_plugin_search_lite_index(catalog: &PluginCatalogDoc) -> serde_json::Value {
    let items = catalog
        .plugins
        .iter()
        .map(|plugin| {
            serde_json::json!({
                "namespace": plugin.namespace,
                "id": plugin.id,
                "name": plugin.name,
                "summary": plugin.summary,
                "latestVersion": plugin.latest_version,
                "categories": plugin.categories,
                "tags": plugin.tags,
                "targets": plugin.targets,
                "scopes": plugin.scopes,
                "components": plugin.components,
                "riskLevel": plugin.risk_level,
                "manifestPath": plugin.manifest_path
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "schema": "skillhub.index.plugin-search-lite.v1",
        "generatedAt": now(),
        "items": items
    })
}

fn should_publish_plugin_version(
    manifest: &PluginManifest,
    catalog: &PluginCatalogDoc,
    meta: &PluginSourceMeta,
) -> Result<bool> {
    let version_exists = manifest
        .versions
        .iter()
        .any(|item| item.version == meta.version);
    let already_in_catalog = catalog
        .plugins
        .iter()
        .any(|plugin| plugin.namespace == meta.namespace && plugin.id == meta.id);
    if version_exists && already_in_catalog {
        Err(anyhow!(
            "PLUGIN_PUBLISH_OBJECT_EXISTS: {}@{} 已在市场中",
            meta.id,
            meta.version
        ))
    } else {
        Ok(!version_exists)
    }
}

async fn validate_plugin_publish_target(
    client: &object_store::AdminObjectClient,
    meta: &PluginSourceMeta,
) -> Result<()> {
    if meta.publish_scope.as_deref() == Some("project") {
        let project_slug = meta.publish_project_slug.as_deref().unwrap_or("");
        let projects = load_remote_projects(client).await?;
        if projects.iter().any(|project| project.slug == project_slug) {
            Ok(())
        } else {
            Err(anyhow!(
                "PLUGIN_SOURCE_INVALID: 发布项目不存在 {project_slug}"
            ))
        }
    } else {
        Ok(())
    }
}

fn ensure_can_manage_plugin_publish_target(
    authorization: &admin_config::AdminAuthorization,
    meta: &PluginSourceMeta,
) -> Result<()> {
    if meta.publish_scope.as_deref() == Some("project") {
        ensure_can_manage_project(
            authorization,
            meta.publish_project_slug.as_deref().unwrap_or(""),
        )
    } else {
        ensure_system_admin(authorization)
    }
}

async fn load_remote_catalog(client: &object_store::AdminObjectClient) -> Result<CatalogDoc> {
    Ok(client
        .get_optional_json::<CatalogDoc>(CATALOG_OBJECT)
        .await?
        .unwrap_or_else(|| CatalogDoc {
            schema: "skillhub.catalog.v1".to_string(),
            generated_at: Some(now()),
            categories: Vec::new(),
            skills: Vec::new(),
        }))
}

async fn load_remote_plugin_catalog(
    client: &object_store::AdminObjectClient,
) -> Result<PluginCatalogDoc> {
    Ok(client
        .get_optional_json::<PluginCatalogDoc>(PLUGIN_CATALOG_OBJECT)
        .await?
        .unwrap_or_else(|| PluginCatalogDoc {
            schema: "skillhub.plugin-catalog.v1".to_string(),
            generated_at: Some(now()),
            plugins: Vec::new(),
        }))
}

async fn load_remote_categories(client: &object_store::AdminObjectClient) -> Result<CategoriesDoc> {
    let doc = client
        .get_optional_json::<CategoriesDoc>(CATEGORIES_OBJECT)
        .await?
        .unwrap_or_else(|| CategoriesDoc {
            schema: "skillhub.categories.v1".to_string(),
            generated_at: Some(now()),
            items: Vec::new(),
        });
    Ok(normalize_categories_doc(doc))
}

fn ensure_publish_category(mut doc: CategoriesDoc, meta: &PublishMeta) -> CategoriesDoc {
    if meta.publish_scope != "project" {
        if let Some(slug) = meta
            .publish_category_slug
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if !doc.items.iter().any(|item| item.id == slug) {
                doc.items.push(Category {
                    id: slug.to_string(),
                    name: category_name_from_slug(slug),
                    order: 10 + doc.items.len() as i64 * 10,
                });
            }
        }
    }
    doc.generated_at = Some(now());
    normalize_categories_doc(doc)
}

async fn load_remote_projects(
    client: &object_store::AdminObjectClient,
) -> Result<Vec<MarketProject>> {
    Ok(client
        .get_optional_json::<ProjectsDoc>(PROJECTS_OBJECT)
        .await?
        .map(ProjectsDoc::into_projects)
        .map(normalize_market_projects)
        .unwrap_or_default())
}

async fn save_remote_projects(
    client: &object_store::AdminObjectClient,
    projects: &[MarketProject],
) -> Result<()> {
    client
        .put_json(PROJECTS_OBJECT, &projects_doc(projects.to_vec()))
        .await
}

fn projects_doc(projects: Vec<MarketProject>) -> ProjectsDoc {
    ProjectsDoc {
        schema: "skillhub.projects.v1".to_string(),
        generated_at: Some(now()),
        projects: normalize_market_projects(projects),
        items: Vec::new(),
    }
}

fn normalize_market_projects(projects: Vec<MarketProject>) -> Vec<MarketProject> {
    let mut by_slug = BTreeMap::new();
    for mut project in projects {
        project.slug = project.slug.trim().to_string();
        if !validation::is_valid_object_segment_value(&project.slug) {
            continue;
        }
        project.name = project.name.trim().to_string();
        if project.name.is_empty() {
            project.name = project.slug.clone();
        }
        project.description = project.description.trim().to_string();
        by_slug.insert(project.slug.clone(), project);
    }

    let mut items = by_slug.into_values().collect::<Vec<_>>();
    items.sort_by(|a, b| {
        a.order
            .cmp(&b.order)
            .then_with(|| a.slug.cmp(&b.slug))
            .then_with(|| a.name.cmp(&b.name))
    });

    let mut next_order = 10;
    for item in &mut items {
        if item.order < next_order {
            item.order = next_order;
        }
        next_order = item.order + 10;
    }

    items.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.slug.cmp(&b.slug)));
    items
}

async fn write_all_market_indexes(
    client: &object_store::AdminObjectClient,
    catalog: &CatalogDoc,
) -> Result<()> {
    let categories = rebuild_catalog_categories(&catalog.skills);
    let search_index = build_search_lite_index(catalog);
    write_market_indexes_for_categories(client, catalog, &categories).await?;
    client
        .put_json("indexes/search-lite.v1.json", &search_index)
        .await?;
    client.put_json(CATALOG_OBJECT, catalog).await?;
    Ok(())
}

async fn write_plugin_market_indexes_for_categories(
    client: &object_store::AdminObjectClient,
    catalog: &PluginCatalogDoc,
    categories: &[String],
) -> Result<()> {
    for category in categories {
        let items = catalog
            .plugins
            .iter()
            .filter(|plugin| plugin.categories.iter().any(|item| item == category))
            .cloned()
            .collect::<Vec<_>>();
        let (scope, slug) = category
            .strip_prefix("project:")
            .map(|project_slug| ("project", project_slug.to_string()))
            .unwrap_or_else(|| ("public", category.to_string()));
        let index = serde_json::json!({
            "schema": "skillhub.index.plugin-market.v1",
            "generatedAt": now(),
            "scope": scope,
            "slug": slug,
            "plugins": items
        });
        let path = if let Some(project_slug) = category.strip_prefix("project:") {
            format!("indexes/plugin-category/projects/{project_slug}.json")
        } else {
            format!("indexes/plugin-category/{category}.json")
        };
        client.put_json(&path, &index).await?;
    }
    Ok(())
}

async fn write_market_indexes_for_categories(
    client: &object_store::AdminObjectClient,
    catalog: &CatalogDoc,
    categories: &[String],
) -> Result<()> {
    for category in categories {
        let market_index = build_market_index_for_category(catalog, category);
        let path = if let Some(project_slug) = category.strip_prefix("project:") {
            format!("indexes/market/projects/{project_slug}.v1.json")
        } else {
            format!("indexes/market/public/{category}.v1.json")
        };
        client.put_json(&path, &market_index).await?;
    }
    Ok(())
}

async fn write_admin_audit(
    client: &object_store::AdminObjectClient,
    authorization: &admin_config::AdminAuthorization,
    action: &str,
    payload: serde_json::Value,
) -> Result<()> {
    let audit_path = format!(
        "admin/audit/{}/{}-{}.json",
        now()[0..10].replace('-', "/"),
        action,
        new_id()
    );
    client
        .put_json(
            &audit_path,
            &admin_audit_envelope(action, authorization, payload),
        )
        .await
}

async fn find_draft_source_for_skill(
    client: &object_store::AdminObjectClient,
    namespace: &str,
    skill_id: &str,
) -> Result<Option<String>> {
    let objects = client.list_objects(DRAFT_ADMIN_PREFIX).await?;
    for object in objects.iter().filter(|object| {
        object.ends_with("/state.v1.json") && object.starts_with(DRAFT_ADMIN_PREFIX)
    }) {
        let state = client
            .get_optional_json::<serde_json::Value>(object)
            .await?
            .unwrap_or_default();
        let state_namespace = state
            .get("namespace")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let state_skill_id = state
            .get("skillId")
            .or_else(|| state.get("skill_id"))
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if state_namespace == namespace && state_skill_id == skill_id {
            let source_path = object
                .trim_start_matches(DRAFT_ADMIN_PREFIX)
                .trim_end_matches("/state.v1.json")
                .to_string();
            return Ok(Some(source_path));
        }
    }

    Ok(None)
}

async fn find_draft_source_for_plugin(
    client: &object_store::AdminObjectClient,
    namespace: &str,
    plugin_id: &str,
) -> Result<Option<String>> {
    let admin_objects = client.list_objects(PLUGIN_ADMIN_PREFIX).await?;
    for object in admin_objects.iter().filter(|object| {
        object.ends_with("/state.v1.json") && object.starts_with(PLUGIN_ADMIN_PREFIX)
    }) {
        let state = client
            .get_optional_json::<serde_json::Value>(object)
            .await?
            .unwrap_or_default();
        let state_namespace = state
            .get("namespace")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let state_plugin_id = state
            .get("pluginId")
            .or_else(|| state.get("plugin_id"))
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if state_namespace == namespace && state_plugin_id == plugin_id {
            let source_path = object
                .trim_start_matches(PLUGIN_ADMIN_PREFIX)
                .trim_end_matches("/state.v1.json")
                .to_string();
            return Ok(Some(source_path));
        }
    }

    let draft_objects = client.list_objects(PLUGIN_DRAFT_PREFIX).await?;
    for source_path in collect_plugin_draft_source_paths(&draft_objects) {
        let draft_root = format!("{}{}/", PLUGIN_DRAFT_PREFIX, source_path);
        let Some(content_root) = resolve_plugin_draft_content_prefix(&draft_root, &draft_objects)
        else {
            continue;
        };
        let Some(meta) = client
            .get_optional_json::<PluginSourceMeta>(&content_root.pluginhub_path)
            .await?
        else {
            continue;
        };
        if meta.namespace == namespace && meta.id == plugin_id {
            return Ok(Some(source_path));
        }
    }

    Ok(None)
}

fn collect_plugin_draft_source_paths(objects: &[String]) -> Vec<String> {
    let mut paths = objects
        .iter()
        .filter_map(|object| {
            if !object.starts_with(PLUGIN_DRAFT_PREFIX) || !object.ends_with("/pluginhub.json") {
                return None;
            }
            let relative = object.trim_start_matches(PLUGIN_DRAFT_PREFIX);
            let source_path = if let Some(path) = relative.strip_suffix("/source/pluginhub.json") {
                let flat_pluginhub = format!("{PLUGIN_DRAFT_PREFIX}{path}/pluginhub.json");
                if objects.iter().any(|candidate| candidate == &flat_pluginhub) {
                    return None;
                }
                path
            } else {
                relative.strip_suffix("/pluginhub.json")?
            };
            if source_path.trim().is_empty()
                || source_path.contains("..")
                || source_path.contains('\\')
            {
                return None;
            }
            Some(source_path.to_string())
        })
        .collect::<Vec<_>>();
    for object in objects {
        if !object.starts_with(PLUGIN_DRAFT_PREFIX) || object.ends_with('/') {
            continue;
        }
        let relative = object.trim_start_matches(PLUGIN_DRAFT_PREFIX);
        if let Some(source_path) = infer_plugin_draft_source_path_from_relative(relative) {
            paths.push(source_path);
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn infer_plugin_draft_source_path_from_relative(relative: &str) -> Option<String> {
    let normalized = relative.replace('\\', "/");
    if normalized.is_empty() || normalized.contains("..") {
        return None;
    }
    let parts = normalized.split('/').collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }

    let marker_index = parts
        .iter()
        .position(|part| is_plugin_draft_root_marker(part))?;
    if marker_index == 0 {
        return None;
    }
    let source_path = parts[..marker_index].join("/");
    if source_path.trim().is_empty() {
        None
    } else {
        Some(source_path)
    }
}

fn is_plugin_draft_root_marker(segment: &str) -> bool {
    matches!(
        segment.to_ascii_lowercase().as_str(),
        "pluginhub.json"
            | "readme.md"
            | "changelog.md"
            | "skills"
            | "agents"
            | "hooks"
            | "assets"
            | ".mcp.json"
            | ".app.json"
            | ".lsp.json"
            | "monitors"
            | "bin"
            | "settings.json"
    )
}

async fn read_draft_files(
    client: &object_store::AdminObjectClient,
    draft_prefix: &str,
    objects: &[String],
) -> Result<Vec<(String, Vec<u8>)>> {
    let mut files = Vec::new();
    for object in objects {
        if !object.starts_with(draft_prefix) || object.ends_with('/') {
            continue;
        }
        let relative = object.trim_start_matches(draft_prefix).to_string();
        if relative.is_empty() || relative.contains("..") || relative.contains('\\') {
            continue;
        }
        let bytes = client.get_bytes(object).await?;
        files.push((relative, bytes));
    }
    Ok(files)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginDraftContentRoot {
    prefix: String,
    pluginhub_path: String,
}

fn resolve_plugin_draft_content_prefix(
    draft_root: &str,
    objects: &[String],
) -> Option<PluginDraftContentRoot> {
    let flat_pluginhub_path = format!("{draft_root}pluginhub.json");
    let legacy_prefix = format!("{draft_root}source/");
    let legacy_pluginhub_path = format!("{legacy_prefix}pluginhub.json");
    let has_legacy_pluginhub = objects
        .iter()
        .any(|object| object == &legacy_pluginhub_path);

    if has_legacy_pluginhub && plugin_draft_root_has_generated_artifacts(draft_root, objects) {
        return Some(PluginDraftContentRoot {
            prefix: legacy_prefix,
            pluginhub_path: legacy_pluginhub_path,
        });
    }

    if objects.iter().any(|object| object == &flat_pluginhub_path) {
        return Some(PluginDraftContentRoot {
            prefix: draft_root.to_string(),
            pluginhub_path: flat_pluginhub_path,
        });
    }

    if has_legacy_pluginhub {
        return Some(PluginDraftContentRoot {
            prefix: legacy_prefix,
            pluginhub_path: legacy_pluginhub_path,
        });
    }

    None
}

fn plugin_draft_root_has_generated_artifacts(draft_root: &str, objects: &[String]) -> bool {
    objects.iter().any(|object| {
        if !object.starts_with(draft_root) || object.ends_with('/') {
            return false;
        }
        let relative = object.trim_start_matches(draft_root);
        is_plugin_platform_generated_path(relative)
    })
}

async fn read_plugin_draft_files(
    client: &object_store::AdminObjectClient,
    draft_prefix: &str,
    objects: &[String],
) -> Result<Vec<(String, Vec<u8>)>> {
    let mut files = Vec::new();
    for object in objects {
        if !object.starts_with(draft_prefix) || object.ends_with('/') {
            continue;
        }
        let relative = object.trim_start_matches(draft_prefix).to_string();
        if !is_plugin_source_file(&relative) {
            continue;
        }
        let bytes = client.get_bytes(object).await?;
        files.push((relative, bytes));
    }
    Ok(files)
}

async fn read_plugin_readme_metadata_from_objects(
    client: &object_store::AdminObjectClient,
    draft_prefix: &str,
    objects: &[String],
) -> Result<DraftSkillMetadata> {
    let object = objects
        .iter()
        .find(|object| {
            object.starts_with(draft_prefix)
                && !object.ends_with('/')
                && object
                    .trim_start_matches(draft_prefix)
                    .eq_ignore_ascii_case("README.md")
        })
        .ok_or_else(|| anyhow!("PLUGIN_SOURCE_INVALID: README.md 缺失"))?;
    let text = client.get_text(object).await?;
    Ok(parse_skill_frontmatter(&text))
}

fn collect_plugin_draft_preview_file_list(
    draft_prefix: &str,
    objects: &[String],
) -> Vec<SkillPreviewFileEntry> {
    let mut relatives = objects
        .iter()
        .filter_map(|object| {
            if !object.starts_with(draft_prefix) || object.ends_with('/') {
                return None;
            }
            let relative = object.trim_start_matches(draft_prefix);
            if !is_plugin_draft_preview_file(relative) {
                return None;
            }
            Some(relative.to_string())
        })
        .collect::<Vec<_>>();
    relatives.sort();
    relatives.truncate(preview::PREVIEW_MAX_FILE_LIST);
    relatives
        .iter()
        .map(|relative| preview::preview_file_entry(relative))
        .collect()
}

fn is_plugin_draft_preview_file(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/");
    if normalized.is_empty() || normalized.contains("..") {
        return false;
    }
    if normalized.starts_with("source/") {
        return false;
    }
    !matches!(
        normalized.to_ascii_lowercase().as_str(),
        "draft.json" | "sync-log.json" | "publish-meta.v1.json" | "state.v1.json"
    )
}

fn is_plugin_source_file(relative: &str) -> bool {
    is_plugin_draft_preview_file(relative)
        && relative.replace('\\', "/").to_ascii_lowercase() != "validation.json"
}

fn has_plugin_source_files(draft_prefix: &str, objects: &[String]) -> bool {
    objects.iter().any(|object| {
        object.starts_with(draft_prefix)
            && !object.ends_with('/')
            && is_plugin_source_file(object.trim_start_matches(draft_prefix))
    })
}

fn infer_plugin_components_from_object_paths(
    draft_prefix: &str,
    objects: &[String],
) -> Vec<String> {
    let files = objects
        .iter()
        .filter_map(|object| {
            if !object.starts_with(draft_prefix) || object.ends_with('/') {
                return None;
            }
            let relative = object.trim_start_matches(draft_prefix).to_string();
            if is_plugin_source_file(&relative) {
                Some((relative, Vec::new()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    infer_plugin_components(&files)
}

fn draft_source_fingerprint(files: &[(String, Vec<u8>)]) -> serde_json::Value {
    let mut items = files
        .iter()
        .map(|(path, bytes)| {
            serde_json::json!({
                "path": path,
                "size": bytes.len(),
                "sha256": object_store::sha256_hex(bytes)
            })
        })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| {
        a.get("path")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .cmp(b.get("path").and_then(|value| value.as_str()).unwrap_or(""))
    });
    let canonical = serde_json::to_vec(&items).unwrap_or_default();
    serde_json::json!({
        "algorithm": "sha256",
        "digest": object_store::sha256_hex(&canonical),
        "files": items
    })
}

fn collect_draft_preview_file_list(
    draft_prefix: &str,
    objects: &[String],
) -> Vec<SkillPreviewFileEntry> {
    let mut relatives = objects
        .iter()
        .filter_map(|object| {
            if !object.starts_with(draft_prefix) || object.ends_with('/') {
                return None;
            }
            let relative = object.trim_start_matches(draft_prefix);
            if relative.is_empty() || relative.contains("..") || relative.contains('\\') {
                return None;
            }
            Some(relative.to_string())
        })
        .collect::<Vec<_>>();
    relatives.sort();
    relatives.truncate(preview::PREVIEW_MAX_FILE_LIST);
    relatives
        .iter()
        .map(|relative| preview::preview_file_entry(relative))
        .collect()
}

async fn collect_draft_preview_files(
    client: &object_store::AdminObjectClient,
    draft_prefix: &str,
    file_list: &[SkillPreviewFileEntry],
    selected_path: Option<&str>,
    meta: Option<&PublishMeta>,
) -> Result<Vec<SkillPreviewFile>> {
    let candidates = preview::preview_candidate_paths(file_list, selected_path);

    let mut files = Vec::new();
    for relative in candidates {
        if files.len() >= preview::PREVIEW_MAX_FILES {
            break;
        }
        let bytes = if relative == "publish-meta.v1.json" {
            let Some(meta) = meta else {
                continue;
            };
            serde_json::to_vec_pretty(meta)?
        } else {
            let object_path = format!("{draft_prefix}{relative}");
            client.get_bytes(&object_path).await?
        };
        if let Some(file) =
            preview::preview_file_from_bytes(&relative, &bytes, preview::PREVIEW_MAX_BYTES)
        {
            files.push(file);
        }
    }

    Ok(files)
}

#[derive(Debug, Clone)]
struct PreparedPluginPublish {
    meta: PluginSourceMeta,
    packages: BTreeMap<String, PreparedPluginPackage>,
    component_inventory: serde_json::Value,
    risk_report: serde_json::Value,
    risk_level: String,
}

#[derive(Debug, Clone)]
struct PreparedPluginPackage {
    bytes: Vec<u8>,
    sha256: String,
    size: i64,
}

fn prepare_plugin_publish(
    files: &[(String, Vec<u8>)],
    saved_meta: Option<PublishMeta>,
) -> Result<PreparedPluginPublish> {
    let readme_metadata = read_plugin_readme_metadata(files)?;
    let meta = match read_optional_plugin_source_meta(files)? {
        Some(source) => {
            let source = apply_plugin_readme_metadata(source, &readme_metadata)?;
            apply_plugin_publish_meta(source, saved_meta)
        }
        None => plugin_source_meta_from_readme(&readme_metadata, saved_meta, files)?,
    };
    validate_plugin_source_meta(&meta)?;
    validate_plugin_source_files(files)?;

    let mut packages = BTreeMap::new();
    for target in &meta.targets {
        let bytes = package::build_plugin_package_zip(files, &meta, target)?;
        let sha256 = object_store::sha256_hex(&bytes);
        packages.insert(
            target.clone(),
            PreparedPluginPackage {
                size: bytes.len() as i64,
                sha256,
                bytes,
            },
        );
    }

    let component_inventory = build_plugin_component_inventory(files, &meta.targets);
    let risk_report = build_plugin_risk_report(&component_inventory, meta.risk_level.as_deref());
    let risk_level = risk_report
        .get("riskLevel")
        .and_then(|value| value.as_str())
        .unwrap_or("low")
        .to_string();

    Ok(PreparedPluginPublish {
        meta,
        packages,
        component_inventory,
        risk_report,
        risk_level,
    })
}

fn read_plugin_readme_metadata(files: &[(String, Vec<u8>)]) -> Result<DraftSkillMetadata> {
    let bytes = files
        .iter()
        .find(|(path, _)| {
            package::normalize_zip_relative_path(path)
                .as_deref()
                .is_some_and(|path| path.eq_ignore_ascii_case("README.md"))
        })
        .map(|(_, bytes)| bytes)
        .ok_or_else(|| anyhow!("PLUGIN_SOURCE_INVALID: README.md 缺失"))?;
    let content = String::from_utf8(bytes.clone())
        .context("PLUGIN_SOURCE_INVALID: README.md 必须是 UTF-8")?;
    let metadata = parse_skill_frontmatter(&content);
    for field in ["name", "description", "version", "author"] {
        match field {
            "name" => {
                required_plugin_readme_field(metadata.name.as_deref(), field)?;
            }
            "description" => {
                required_plugin_readme_field(metadata.description.as_deref(), field)?;
            }
            "version" => {
                required_plugin_readme_field(metadata.version.as_deref(), field)?;
            }
            "author" => {
                required_plugin_readme_field(metadata.author.as_deref(), field)?;
            }
            _ => {}
        }
    }
    Ok(metadata)
}

fn read_optional_plugin_source_meta(
    files: &[(String, Vec<u8>)],
) -> Result<Option<PluginSourceMeta>> {
    let Some(bytes) = files
        .iter()
        .find(|(path, _)| {
            package::normalize_zip_relative_path(path).as_deref() == Some("pluginhub.json")
        })
        .map(|(_, bytes)| bytes)
    else {
        return Ok(None);
    };
    serde_json::from_slice(bytes)
        .map(Some)
        .context("PLUGIN_SOURCE_INVALID: pluginhub.json 解析失败")
}

fn validate_plugin_source_meta(meta: &PluginSourceMeta) -> Result<()> {
    if meta.schema.trim() != "skillhub.plugin-source.v1" {
        return Err(anyhow!(
            "PLUGIN_SOURCE_INVALID: schema 必须是 skillhub.plugin-source.v1"
        ));
    }
    for (name, value) in [
        ("namespace", meta.namespace.as_str()),
        ("id", meta.id.as_str()),
        ("name", meta.name.as_str()),
        ("version", meta.version.as_str()),
        ("summary", meta.summary.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(anyhow!("PLUGIN_SOURCE_INVALID: {name} 不能为空"));
        }
    }
    validation::validate_object_segment("namespace", &meta.namespace)?;
    validation::validate_object_segment("plugin id", &meta.id)?;
    validation::validate_object_segment("version", &meta.version)?;
    if meta.targets.is_empty() {
        return Err(anyhow!("PLUGIN_SOURCE_INVALID: targets 至少包含一个平台"));
    }
    for target in &meta.targets {
        validation::validate_plugin_target(target)?;
    }
    if meta.scopes.is_empty() {
        return Err(anyhow!("PLUGIN_SOURCE_INVALID: scopes 至少包含一个作用域"));
    }
    for scope in &meta.scopes {
        validation::validate_plugin_scope(scope)?;
    }
    Ok(())
}

fn validate_plugin_source_files(files: &[(String, Vec<u8>)]) -> Result<()> {
    for (path, _) in files {
        let Some(path) = package::normalize_zip_relative_path(path) else {
            return Err(anyhow!(
                "PLUGIN_PACKAGE_BUILD_FAILED: 包含不安全路径 {path}"
            ));
        };
        if is_plugin_platform_generated_path(&path) {
            return Err(anyhow!(
                "PLUGIN_SOURCE_INVALID: 插件草稿只保存通用插件数据，平台目录由发布器动态生成: {path}"
            ));
        }
    }
    Ok(())
}

fn is_plugin_platform_generated_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    matches!(
        normalized.as_str(),
        ".codex-plugin/plugin.json" | ".claude-plugin/plugin.json"
    ) || normalized.starts_with(".codex-plugin/")
        || normalized.starts_with(".claude-plugin/")
        || normalized.starts_with("codex/")
        || normalized.starts_with("claude/")
}

fn build_plugin_component_inventory(
    files: &[(String, Vec<u8>)],
    targets: &[String],
) -> serde_json::Value {
    let mut target_map = serde_json::Map::new();
    for target in targets {
        let inventory = match target.as_str() {
            "codex" => codex_component_inventory(files),
            "claude" => claude_component_inventory(files),
            _ => serde_json::json!({}),
        };
        target_map.insert(target.clone(), inventory);
    }
    serde_json::json!({
        "schema": "skillhub.plugin-component-inventory.v1",
        "targets": target_map
    })
}

fn codex_component_inventory(files: &[(String, Vec<u8>)]) -> serde_json::Value {
    let paths = package::common_plugin_paths(files);
    serde_json::json!({
        "skills": package::direct_child_dirs(&paths, "skills/"),
        "hooks": direct_child_files(&paths, "hooks/"),
        "mcpServers": file_present(&paths, ".mcp.json"),
        "apps": file_present(&paths, ".app.json"),
        "assets": files_under(&paths, "assets/")
    })
}

fn claude_component_inventory(files: &[(String, Vec<u8>)]) -> serde_json::Value {
    let paths = package::common_plugin_paths(files);
    serde_json::json!({
        "skills": package::direct_child_dirs(&paths, "skills/"),
        "agents": direct_child_files(&paths, "agents/"),
        "hooks": direct_child_files(&paths, "hooks/"),
        "mcpServers": file_present(&paths, ".mcp.json"),
        "lspServers": file_present(&paths, ".lsp.json"),
        "monitors": direct_child_files(&paths, "monitors/"),
        "bin": files_under(&paths, "bin/"),
        "settings": file_present(&paths, "settings.json")
    })
}

fn infer_plugin_components(files: &[(String, Vec<u8>)]) -> Vec<String> {
    let paths = package::common_plugin_paths(files);
    let mut components = Vec::new();
    for (component, present) in [
        (
            "skills",
            paths.iter().any(|path| path.starts_with("skills/")),
        ),
        (
            "agents",
            paths.iter().any(|path| path.starts_with("agents/")),
        ),
        ("hooks", paths.iter().any(|path| path.starts_with("hooks/"))),
        (
            "assets",
            paths.iter().any(|path| path.starts_with("assets/")),
        ),
        ("mcp", paths.iter().any(|path| path == ".mcp.json")),
        ("apps", paths.iter().any(|path| path == ".app.json")),
        ("lsp", paths.iter().any(|path| path == ".lsp.json")),
        (
            "monitors",
            paths.iter().any(|path| path.starts_with("monitors/")),
        ),
        ("bin", paths.iter().any(|path| path.starts_with("bin/"))),
        ("settings", paths.iter().any(|path| path == "settings.json")),
    ] {
        if present {
            components.push(component.to_string());
        }
    }
    components
}

fn direct_child_files(paths: &[String], prefix: &str) -> Vec<String> {
    let mut items = paths
        .iter()
        .filter_map(|path| path.strip_prefix(prefix))
        .filter(|rest| !rest.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    items.sort();
    items.dedup();
    items
}

fn files_under(paths: &[String], prefix: &str) -> Vec<String> {
    direct_child_files(paths, prefix)
}

fn file_present(paths: &[String], file: &str) -> Vec<String> {
    if paths.iter().any(|path| path == file) {
        vec![file.to_string()]
    } else {
        Vec::new()
    }
}

fn build_plugin_risk_report(
    component_inventory: &serde_json::Value,
    declared_level: Option<&str>,
) -> serde_json::Value {
    let mut reasons = Vec::<String>::new();
    let Some(targets) = component_inventory
        .get("targets")
        .and_then(|value| value.as_object())
    else {
        return serde_json::json!({
            "schema": "skillhub.plugin-risk-report.v1",
            "riskLevel": declared_level.unwrap_or("low"),
            "reasons": [],
            "requiresUserReview": false,
            "notes": []
        });
    };

    for (target, inventory) in targets {
        for (field, reason) in [
            ("hooks", "contains hooks"),
            ("mcpServers", "contains MCP servers"),
            ("lspServers", "contains LSP servers"),
            ("monitors", "contains monitors"),
            ("bin", "contains executable bin files"),
        ] {
            if !inventory_array(inventory, field).is_empty() {
                reasons.push(format!("{target}: {reason}"));
            }
        }
        for (field, reason) in [
            ("agents", "contains agents"),
            ("apps", "contains app definitions"),
            ("settings", "contains settings"),
        ] {
            if !inventory_array(inventory, field).is_empty() {
                reasons.push(format!("{target}: {reason}"));
            }
        }
    }
    reasons.sort();
    reasons.dedup();

    let computed = if reasons.iter().any(|reason| {
        reason.contains("hooks")
            || reason.contains("MCP")
            || reason.contains("LSP")
            || reason.contains("monitors")
            || reason.contains("executable")
    }) {
        "high"
    } else if reasons.is_empty() {
        "low"
    } else {
        "medium"
    };
    let risk_level = max_risk_level(declared_level.unwrap_or(""), computed);
    let requires_user_review = risk_level == "high";
    serde_json::json!({
        "schema": "skillhub.plugin-risk-report.v1",
        "riskLevel": risk_level,
        "reasons": reasons,
        "requiresUserReview": requires_user_review,
        "notes": [
            "Codex plugin hooks require user trust review after installation.",
            "Claude plugin changes require /reload-plugins to take effect in current sessions."
        ]
    })
}

fn inventory_array<'a>(
    inventory: &'a serde_json::Value,
    field: &str,
) -> Vec<&'a serde_json::Value> {
    inventory
        .get(field)
        .and_then(|value| value.as_array())
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn max_risk_level(first: &str, second: &str) -> String {
    fn rank(level: &str) -> i32 {
        match level {
            "high" => 3,
            "medium" => 2,
            "low" => 1,
            _ => 0,
        }
    }
    if rank(first) >= rank(second) {
        first.to_string()
    } else {
        second.to_string()
    }
}

fn build_plugin_json(prepared: &PreparedPluginPublish) -> serde_json::Value {
    let mut packages = serde_json::Map::new();
    for (target, package) in &prepared.packages {
        packages.insert(
            target.clone(),
            serde_json::json!({
                "file": format!("package.{target}.zip"),
                "sha256": package.sha256,
                "size": package.size
            }),
        );
    }
    serde_json::json!({
        "schema": "skillhub.plugin.v1",
        "namespace": prepared.meta.namespace,
        "id": prepared.meta.id,
        "name": prepared.meta.name,
        "version": prepared.meta.version,
        "summary": prepared.meta.summary,
        "categories": prepared.meta.categories,
        "tags": prepared.meta.tags,
        "targets": prepared.meta.targets,
        "scopes": prepared.meta.scopes,
        "components": prepared.meta.components,
        "riskLevel": prepared.risk_level,
        "packages": packages
    })
}

fn plugin_changelog(files: &[(String, Vec<u8>)]) -> String {
    for path in ["CHANGELOG.md", "changelog.md"] {
        if let Some(bytes) = files
            .iter()
            .find(|(relative, _)| {
                package::normalize_zip_relative_path(relative).as_deref() == Some(path)
            })
            .map(|(_, bytes)| bytes)
        {
            if let Ok(text) = String::from_utf8(bytes.clone()) {
                return text;
            }
        }
    }
    "Initial plugin publish.\n".to_string()
}

fn build_skill_json(
    meta: &PublishMeta,
    version: &str,
    author: &str,
    package_hash: &str,
    package_size: i64,
) -> serde_json::Value {
    serde_json::json!({
        "schema": "skillhub.skill.v1",
        "id": meta.skill_id,
        "namespace": meta.namespace,
        "name": meta.name,
        "version": version,
        "summary": meta.summary,
        "categories": publish_categories(meta),
        "tags": meta.tags,
        "levels": meta.levels,
        "targets": meta.targets,
        "author": { "name": author },
        "package": {
            "file": "package.zip",
            "sha256": package_hash,
            "size": package_size
        }
    })
}

fn build_search_lite_index(catalog: &CatalogDoc) -> serde_json::Value {
    let items = catalog
        .skills
        .iter()
        .map(|skill| {
            serde_json::json!({
                "namespace": skill.namespace,
                "id": skill.id,
                "name": skill.name,
                "summary": skill.summary,
                "latestVersion": skill.latest_version,
                "categories": skill.categories,
                "tags": skill.tags,
                "manifestPath": skill.manifest_path
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "schema": "skillhub.index.search-lite.v1",
        "generatedAt": now(),
        "items": items
    })
}

fn build_market_index_for_category(catalog: &CatalogDoc, category: &str) -> serde_json::Value {
    let items = catalog
        .skills
        .iter()
        .filter(|skill| skill.categories.iter().any(|item| item == category))
        .cloned()
        .collect::<Vec<_>>();
    let (scope, slug) = category
        .strip_prefix("project:")
        .map(|project_slug| ("project", project_slug.to_string()))
        .unwrap_or_else(|| ("public", category.to_string()));
    serde_json::json!({
        "schema": "skillhub.index.market.v1",
        "generatedAt": now(),
        "scope": scope,
        "slug": slug,
        "skills": items
    })
}

fn ensure_scope_can_enable(
    conn: &rusqlite::Connection,
    exclude_binding_id: Option<&str>,
    namespace: &str,
    skill_id: &str,
    target: &str,
    desired_level: &str,
) -> Result<()> {
    let conflicting_level = if desired_level == "personal" {
        "project"
    } else {
        "personal"
    };

    let mut stmt = conn.prepare(
        "SELECT id, level, project_path
         FROM skill_bindings
         WHERE namespace = ?1
           AND skill_id = ?2
           AND target = ?3
           AND level = ?4
           AND enabled = 1",
    )?;

    let rows = stmt.query_map(
        params![namespace, skill_id, target, conflicting_level],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        },
    )?;

    let conflicts = rows
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|(id, _, _)| {
            exclude_binding_id
                .map(|exclude| exclude != id)
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();

    if conflicts.is_empty() {
        return Ok(());
    }

    if desired_level == "personal" {
        Err(anyhow!(
            "该 skill 已在 {} 个项目中启用，请先禁用项目级绑定",
            conflicts.len()
        ))
    } else {
        Err(anyhow!("该 skill 已在个人级启用，项目级不能再启用"))
    }
}

fn find_same_scope_binding(
    conn: &rusqlite::Connection,
    namespace: &str,
    skill_id: &str,
    target: &str,
    level: &str,
    project_path: Option<&str>,
) -> Result<Option<SkillBinding>> {
    Ok(list_bindings_inner(conn)?.into_iter().find(|binding| {
        let same_project = level != "project" || binding.project_path.as_deref() == project_path;
        binding.namespace == namespace
            && binding.skill_id == skill_id
            && binding.target == target
            && binding.level == level
            && same_project
    }))
}

fn ensure_plugin_scope_can_enable(
    conn: &rusqlite::Connection,
    exclude_binding_id: Option<&str>,
    namespace: &str,
    plugin_id: &str,
    target: &str,
    desired_scope: &str,
) -> Result<()> {
    let bindings = list_plugin_bindings_inner(conn)?;
    ensure_plugin_scope_can_enable_from_bindings(
        &bindings,
        exclude_binding_id,
        namespace,
        plugin_id,
        target,
        desired_scope,
    )
}

fn ensure_plugin_scope_can_enable_from_bindings(
    bindings: &[PluginBinding],
    exclude_binding_id: Option<&str>,
    namespace: &str,
    plugin_id: &str,
    target: &str,
    desired_scope: &str,
) -> Result<()> {
    let conflicting_scopes: &[&str] = if desired_scope == "user" || desired_scope == "personal" {
        &["project"]
    } else {
        &["user", "personal"]
    };

    let conflicts = bindings
        .iter()
        .filter(|binding| {
            exclude_binding_id
                .map(|exclude| exclude != binding.id)
                .unwrap_or(true)
        })
        .filter(|binding| {
            binding.namespace == namespace
                && binding.plugin_id == plugin_id
                && binding.target == target
                && conflicting_scopes.contains(&binding.scope.as_str())
                && binding.enabled
        })
        .collect::<Vec<_>>();

    if conflicts.is_empty() {
        return Ok(());
    }

    if desired_scope == "user" || desired_scope == "personal" {
        Err(anyhow!(
            "PLUGIN_SCOPE_CONFLICT: 该 plugin 已在 {} 个项目中启用，请先禁用项目级绑定",
            conflicts.len()
        ))
    } else {
        Err(anyhow!(
            "PLUGIN_SCOPE_CONFLICT: 该 plugin 已在个人级启用，项目级不能再启用"
        ))
    }
}

fn find_same_plugin_binding(
    conn: &rusqlite::Connection,
    namespace: &str,
    plugin_id: &str,
    target: &str,
    scope: &str,
    project_path: Option<&str>,
) -> Result<Option<PluginBinding>> {
    Ok(list_plugin_bindings_inner(conn)?
        .into_iter()
        .find(|binding| {
            let same_project =
                scope != "project" || binding.project_path.as_deref() == project_path;
            binding.namespace == namespace
                && binding.plugin_id == plugin_id
                && binding.target == target
                && binding.scope == scope
                && same_project
        }))
}

#[derive(Debug, Clone)]
struct MaterializedPluginMarketplace {
    id: String,
    name: String,
    root_path: PathBuf,
    marketplace_path: PathBuf,
}

fn materialize_plugin_marketplace(
    state: &AppState,
    plugin: &MarketPlugin,
    version: &str,
    target: &str,
    scope: &str,
    project_path: Option<&str>,
    package_dir: &Path,
) -> Result<MaterializedPluginMarketplace> {
    let name = plugin_marketplace_name(target, scope, project_path);
    let root = prepare_plugin_marketplace_root(state, target, scope, project_path)?;
    let plugins_root = root.join("plugins");
    let plugin_dir_name = format!("{}.{}", plugin.namespace, plugin.id);
    let plugin_dir = plugins_root.join(&plugin_dir_name);
    if plugin_dir.exists() {
        fs::remove_dir_all(&plugin_dir)
            .context("PLUGIN_MARKETPLACE_WRITE_FAILED: clean plugin marketplace dir failed")?;
    }
    fs::create_dir_all(&plugin_dir)?;
    package::copy_dir_recursive_including_json(package_dir, &plugin_dir)?;

    let marketplace_path =
        write_plugin_marketplace_file(target, &root, &plugin_dir_name, plugin, version, &name)?;
    Ok(MaterializedPluginMarketplace {
        id: new_id(),
        name,
        root_path: root,
        marketplace_path,
    })
}

pub(crate) fn plugin_marketplace_root(
    state: &AppState,
    target: &str,
    scope: &str,
    project_path: Option<&str>,
) -> Result<PathBuf> {
    if scope == "project" {
        let project = project_path
            .ok_or_else(|| anyhow!("PLUGIN_MARKETPLACE_WRITE_FAILED: missing projectPath"))?;
        return Ok(match target {
            "codex" => PathBuf::from(project),
            "claude" => PathBuf::from(project)
                .join(".claude")
                .join("skillhub-plugin-marketplace"),
            _ => state
                .app_dir
                .join("plugin-marketplaces")
                .join(target)
                .join("projects")
                .join(local::path_hash(project)),
        });
    }

    if target == "codex" && scope == "user" {
        return user_home_dir();
    }

    Ok(state
        .app_dir
        .join("plugin-marketplaces")
        .join(target)
        .join(scope))
}

fn prepare_plugin_marketplace_root(
    state: &AppState,
    target: &str,
    scope: &str,
    project_path: Option<&str>,
) -> Result<PathBuf> {
    let root = plugin_marketplace_root(state, target, scope, project_path)?;
    migrate_claude_project_marketplace_root(target, scope, project_path, &root)?;
    Ok(root)
}

fn migrate_claude_project_marketplace_root(
    target: &str,
    scope: &str,
    project_path: Option<&str>,
    new_root: &Path,
) -> Result<()> {
    if target != "claude" || !is_claude_project_like_scope(scope) {
        return Ok(());
    }
    let Some(project_path) = project_path.map(str::trim).filter(|path| !path.is_empty()) else {
        return Ok(());
    };

    let legacy_root = PathBuf::from(project_path);
    if legacy_root == new_root {
        return Ok(());
    }

    let legacy_marketplace_path = local::plugin_marketplace_path("claude", &legacy_root);
    let Some(legacy_doc) = read_json_file(&legacy_marketplace_path) else {
        return Ok(());
    };

    let new_marketplace_path = local::plugin_marketplace_path("claude", new_root);
    if !new_marketplace_path.exists() {
        if let Some(parent) = new_marketplace_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            &new_marketplace_path,
            serde_json::to_vec_pretty(&ensure_claude_marketplace_schema(legacy_doc.clone()))?,
        )
        .context("PLUGIN_MARKETPLACE_WRITE_FAILED: migrate Claude marketplace failed")?;
    }

    for relative in claude_marketplace_plugin_source_paths(&legacy_doc) {
        let source_path = legacy_root.join(&relative);
        let destination_path = new_root.join(&relative);
        if source_path.is_dir() {
            if !destination_path.exists() {
                if let Some(parent) = destination_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                package::copy_dir_recursive_including_json(&source_path, &destination_path)?;
            }
            if destination_path.exists() {
                fs::remove_dir_all(&source_path).context(
                    "PLUGIN_MARKETPLACE_WRITE_FAILED: remove legacy Claude plugin dir failed",
                )?;
            }
        }
    }

    fs::remove_file(&legacy_marketplace_path)
        .context("PLUGIN_MARKETPLACE_WRITE_FAILED: remove legacy Claude marketplace failed")?;
    if let Some(parent) = legacy_marketplace_path.parent() {
        remove_dir_if_empty(parent)?;
    }
    remove_dir_if_empty(&legacy_root.join("plugins"))?;
    Ok(())
}

fn claude_marketplace_plugin_source_paths(doc: &serde_json::Value) -> Vec<PathBuf> {
    doc.get("plugins")
        .and_then(|plugins| plugins.as_array())
        .into_iter()
        .flatten()
        .filter_map(|plugin| plugin.get("source").and_then(|source| source.as_str()))
        .filter_map(normalize_claude_marketplace_plugin_source)
        .map(PathBuf::from)
        .collect()
}

fn normalize_claude_marketplace_plugin_source(source: &str) -> Option<String> {
    let normalized = source.replace('\\', "/");
    let relative = normalized.strip_prefix("./").unwrap_or(&normalized);
    let relative = package::normalize_zip_relative_path(relative)?;
    relative.starts_with("plugins/").then_some(relative)
}

fn remove_dir_if_empty(path: &Path) -> Result<()> {
    if path.is_dir() && fs::read_dir(path)?.next().is_none() {
        fs::remove_dir(path)?;
    }
    Ok(())
}

fn plugin_marketplace_name(target: &str, scope: &str, project_path: Option<&str>) -> String {
    if target == "codex" && scope == "user" {
        return "personal".to_string();
    }
    if target == "codex" && scope == "project" {
        if let Some(project_path) = project_path {
            return format!("skillhub-{}", local::path_hash(project_path));
        }
    }
    "skillhub".to_string()
}

fn plugin_marketplace_display_name(marketplace_name: &str) -> String {
    if marketplace_name == "personal" {
        "Personal".to_string()
    } else {
        "Skill Hub".to_string()
    }
}

fn write_plugin_marketplace_file(
    target: &str,
    root: &Path,
    plugin_dir_name: &str,
    plugin: &MarketPlugin,
    version: &str,
    marketplace_name: &str,
) -> Result<PathBuf> {
    match target {
        "codex" => {
            let market_dir = root.join(".agents").join("plugins");
            fs::create_dir_all(&market_dir)?;
            let path = market_dir.join("marketplace.json");
            let existing = read_json_file(&path).unwrap_or_else(|| {
                serde_json::json!({
                    "name": marketplace_name,
                    "interface": {
                        "displayName": plugin_marketplace_display_name(marketplace_name)
                    },
                    "plugins": []
                })
            });
            let doc = upsert_plugin_marketplace_entry(
                existing,
                marketplace_name,
                None,
                serde_json::json!({
                    "name": plugin.id,
                    "description": plugin.summary,
                    "version": version,
                    "source": {
                        "source": "local",
                        "path": format!("./plugins/{plugin_dir_name}")
                    },
                    "policy": {
                        "installation": "AVAILABLE",
                        "authentication": "ON_INSTALL"
                    },
                    "category": plugin.categories.first().cloned().unwrap_or_else(|| "Productivity".to_string())
                }),
            );
            fs::write(&path, serde_json::to_vec_pretty(&doc)?)
                .context("PLUGIN_MARKETPLACE_WRITE_FAILED: write Codex marketplace failed")?;
            Ok(path)
        }
        "claude" => {
            let market_dir = root.join(".claude-plugin");
            fs::create_dir_all(&market_dir)?;
            let path = market_dir.join("marketplace.json");
            let existing = read_json_file(&path).unwrap_or_else(|| {
                serde_json::json!({
                    "name": marketplace_name,
                    "owner": plugin_marketplace_owner(),
                    "plugins": []
                })
            });
            let doc = upsert_plugin_marketplace_entry(
                ensure_claude_marketplace_schema(existing),
                marketplace_name,
                None,
                serde_json::json!({
                    "name": plugin.id,
                    "description": plugin.summary,
                    "version": version,
                    "source": format!("./plugins/{plugin_dir_name}")
                }),
            );
            fs::write(&path, serde_json::to_vec_pretty(&doc)?)
                .context("PLUGIN_MARKETPLACE_WRITE_FAILED: write Claude marketplace failed")?;
            Ok(path)
        }
        _ => Err(anyhow!("PLUGIN_TARGET_UNSUPPORTED: {target}")),
    }
}

pub(crate) fn read_json_file(path: &Path) -> Option<serde_json::Value> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn ensure_claude_marketplace_schema(mut doc: serde_json::Value) -> serde_json::Value {
    if !doc.get("owner").is_some_and(|value| value.is_object()) {
        doc["owner"] = plugin_marketplace_owner();
    }
    doc
}

fn plugin_marketplace_owner() -> serde_json::Value {
    serde_json::json!({
        "name": "Skill Hub"
    })
}

fn upsert_plugin_marketplace_entry(
    mut doc: serde_json::Value,
    marketplace_name: &str,
    version: Option<serde_json::Value>,
    entry: serde_json::Value,
) -> serde_json::Value {
    if !doc.is_object() {
        doc = serde_json::json!({});
    }
    let object = doc.as_object_mut().expect("object checked");
    object.insert(
        "name".to_string(),
        serde_json::Value::String(marketplace_name.to_string()),
    );
    if let Some(version) = version {
        object.insert("version".to_string(), version);
    }

    let entry_name = entry
        .get("name")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let plugins = object
        .entry("plugins".to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if !plugins.is_array() {
        *plugins = serde_json::Value::Array(Vec::new());
    }
    let items = plugins.as_array_mut().expect("array checked");
    items.retain(|item| {
        item.get("name")
            .and_then(|value| value.as_str())
            .map(|name| name != entry_name)
            .unwrap_or(true)
    });
    items.push(entry);
    items.sort_by(|a, b| {
        a.get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .cmp(b.get("name").and_then(|value| value.as_str()).unwrap_or(""))
    });
    doc
}

fn remove_plugin_marketplace_entry(
    mut doc: serde_json::Value,
    plugin_id: &str,
) -> serde_json::Value {
    if let Some(items) = doc
        .get_mut("plugins")
        .and_then(|plugins| plugins.as_array_mut())
    {
        items.retain(|item| {
            item.get("name")
                .and_then(|value| value.as_str())
                .map(|name| name != plugin_id)
                .unwrap_or(true)
        });
    }
    doc
}

fn rewrite_plugin_marketplace_entry(
    target: &str,
    root: &Path,
    plugin_dir_name: &str,
    plugin: &MarketPlugin,
    version: &str,
    marketplace_name: &str,
) -> Result<PathBuf> {
    write_plugin_marketplace_file(
        target,
        root,
        plugin_dir_name,
        plugin,
        version,
        marketplace_name,
    )
}

fn remove_plugin_from_marketplace_file(target: &str, root: &Path, plugin_id: &str) -> Result<()> {
    let path = local::plugin_marketplace_path(target, root);
    let Some(doc) = read_json_file(&path) else {
        return Ok(());
    };
    let doc = remove_plugin_marketplace_entry(doc, plugin_id);
    fs::write(&path, serde_json::to_vec_pretty(&doc)?)
        .context("PLUGIN_MARKETPLACE_WRITE_FAILED: remove plugin marketplace entry failed")?;
    Ok(())
}

fn should_sync_codex_plugin_cli(target: &str, scope: &str) -> bool {
    target == "codex" && scope != "project"
}

fn sync_codex_plugin_install(
    target: &str,
    scope: &str,
    plugin_id: &str,
    marketplace_name: &str,
    marketplace_root: &Path,
) -> Result<()> {
    if !should_sync_codex_plugin_cli(target, scope) {
        return Ok(());
    }
    if marketplace_name != "personal" {
        let root_path = canonical_display_path(marketplace_root);
        run_codex_plugin_command(
            &["plugin", "marketplace", "add", &root_path],
            CodexCommandFailureMode::IgnoreAlreadyConfigured,
        )?;
    }
    let selector = format!("{plugin_id}@{marketplace_name}");
    run_codex_plugin_command(
        &["plugin", "add", &selector],
        CodexCommandFailureMode::Strict,
    )
}

fn sync_codex_plugin_remove(
    target: &str,
    scope: &str,
    plugin_id: &str,
    marketplace_name: &str,
) -> Result<()> {
    if !should_sync_codex_plugin_cli(target, scope) {
        return Ok(());
    }
    let selector = format!("{plugin_id}@{marketplace_name}");
    run_codex_plugin_command(
        &["plugin", "remove", &selector],
        CodexCommandFailureMode::IgnoreMissing,
    )
}

fn sync_claude_plugin_install(
    target: &str,
    plugin_id: &str,
    marketplace_name: &str,
    scope: &str,
    project_path: Option<&str>,
    marketplace_root: &Path,
) -> Result<()> {
    let Some(commands) = build_claude_plugin_install_commands(
        target,
        plugin_id,
        marketplace_name,
        scope,
        marketplace_root,
    ) else {
        return Ok(());
    };

    let working_dir = claude_plugin_command_working_dir(scope, project_path);
    if let Some(args) = build_claude_marketplace_remove_command(target, marketplace_name, scope) {
        run_claude_plugin_command(
            &args,
            ClaudeCommandFailureMode::IgnoreMissing,
            working_dir.as_deref(),
        )?;
    }
    for args in commands {
        run_claude_plugin_command(
            &args,
            ClaudeCommandFailureMode::IgnoreAlreadyConfigured,
            working_dir.as_deref(),
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaudePluginRemoveAction {
    Disable,
    Uninstall,
}

fn sync_claude_plugin_remove(
    target: &str,
    plugin_id: &str,
    marketplace_name: &str,
    scope: &str,
    project_path: Option<&str>,
    action: ClaudePluginRemoveAction,
) -> Result<()> {
    let Some(args) =
        build_claude_plugin_remove_command(target, plugin_id, marketplace_name, scope, action)
    else {
        return Ok(());
    };
    let working_dir = claude_plugin_command_working_dir(scope, project_path);
    run_claude_plugin_command(
        &args,
        ClaudeCommandFailureMode::IgnoreMissing,
        working_dir.as_deref(),
    )
}

fn build_claude_plugin_install_commands(
    target: &str,
    plugin_id: &str,
    marketplace_name: &str,
    scope: &str,
    marketplace_root: &Path,
) -> Option<Vec<Vec<String>>> {
    if target != "claude" {
        return None;
    }
    let selector = format!("{plugin_id}@{marketplace_name}");
    let scope = normalize_claude_plugin_scope(scope);
    let marketplace_path =
        canonical_display_path(&local::plugin_marketplace_path("claude", marketplace_root));
    Some(vec![
        vec![
            "plugin".to_string(),
            "marketplace".to_string(),
            "add".to_string(),
            marketplace_path,
            "--scope".to_string(),
            scope.clone(),
        ],
        vec![
            "plugin".to_string(),
            "install".to_string(),
            selector.clone(),
            "--scope".to_string(),
            scope.clone(),
        ],
        vec![
            "plugin".to_string(),
            "enable".to_string(),
            selector,
            "--scope".to_string(),
            scope,
        ],
    ])
}

fn build_claude_marketplace_remove_command(
    target: &str,
    marketplace_name: &str,
    scope: &str,
) -> Option<Vec<String>> {
    if target != "claude" || !is_claude_project_like_scope(scope) {
        return None;
    }
    Some(vec![
        "plugin".to_string(),
        "marketplace".to_string(),
        "remove".to_string(),
        marketplace_name.to_string(),
        "--scope".to_string(),
        normalize_claude_plugin_scope(scope),
    ])
}

fn build_claude_plugin_remove_command(
    target: &str,
    plugin_id: &str,
    marketplace_name: &str,
    scope: &str,
    action: ClaudePluginRemoveAction,
) -> Option<Vec<String>> {
    if target != "claude" {
        return None;
    }
    let command = match action {
        ClaudePluginRemoveAction::Disable => "disable",
        ClaudePluginRemoveAction::Uninstall => "uninstall",
    };
    Some(vec![
        "plugin".to_string(),
        command.to_string(),
        format!("{plugin_id}@{marketplace_name}"),
        "--scope".to_string(),
        normalize_claude_plugin_scope(scope),
    ])
}

fn normalize_claude_plugin_scope(scope: &str) -> String {
    match scope {
        "project" | "local" => scope.to_string(),
        _ => "user".to_string(),
    }
}

fn is_claude_project_like_scope(scope: &str) -> bool {
    matches!(
        normalize_claude_plugin_scope(scope).as_str(),
        "project" | "local"
    )
}

fn claude_plugin_command_working_dir(scope: &str, project_path: Option<&str>) -> Option<PathBuf> {
    if !is_claude_project_like_scope(scope) {
        return None;
    }
    project_path
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaudeCommandFailureMode {
    IgnoreMissing,
    IgnoreAlreadyConfigured,
}

fn run_claude_plugin_command(
    args: &[String],
    failure_mode: ClaudeCommandFailureMode,
    working_dir: Option<&Path>,
) -> Result<()> {
    let borrowed = args.iter().map(String::as_str).collect::<Vec<_>>();
    let candidates = claude_plugin_command_candidates();
    run_plugin_command(
        &candidates,
        &borrowed,
        "PLUGIN_CLAUDE_INSTALL_FAILED",
        working_dir,
        |message| match failure_mode {
            ClaudeCommandFailureMode::IgnoreMissing => is_plugin_missing_message(message),
            ClaudeCommandFailureMode::IgnoreAlreadyConfigured => {
                is_plugin_already_configured_message(message)
                    || is_plugin_already_installed_message(message)
                    || is_plugin_already_enabled_message(message)
            }
        },
    )
}

fn claude_plugin_command_candidates() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut candidates = Vec::new();
    if cfg!(windows) {
        candidates.push(("cmd", vec!["/C", "claude.cmd"]));
    }
    candidates.push(("claude", Vec::new()));
    candidates
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodexCommandFailureMode {
    Strict,
    IgnoreMissing,
    IgnoreAlreadyConfigured,
}

fn run_codex_plugin_command(args: &[&str], failure_mode: CodexCommandFailureMode) -> Result<()> {
    let mut candidates = Vec::new();
    if cfg!(windows) {
        candidates.push(("cmd", vec!["/C", "codex.cmd"]));
    }
    candidates.push(("codex", Vec::new()));

    run_plugin_command(
        &candidates,
        args,
        "PLUGIN_CODEX_INSTALL_FAILED",
        None,
        |message| match failure_mode {
            CodexCommandFailureMode::Strict => false,
            CodexCommandFailureMode::IgnoreMissing => is_plugin_missing_message(message),
            CodexCommandFailureMode::IgnoreAlreadyConfigured => {
                is_plugin_already_configured_message(message)
            }
        },
    )
}

fn run_plugin_command<F>(
    candidates: &[(&str, Vec<&str>)],
    args: &[&str],
    error_code: &str,
    working_dir: Option<&Path>,
    is_ignorable_failure: F,
) -> Result<()>
where
    F: Fn(&str) -> bool,
{
    let mut last_error = None;
    let mut first_execution_error = None;
    let mut command_not_found = true;
    for (program, prefix) in candidates {
        let mut command = external_command(program);
        command.args(prefix).args(args);
        if let Some(working_dir) = working_dir {
            command.current_dir(working_dir);
        }
        match command.output() {
            Ok(output) if output.status.success() => return Ok(()),
            Ok(output) => {
                command_not_found = false;
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let message = if stderr.is_empty() { stdout } else { stderr };
                if is_ignorable_failure(&message) {
                    return Ok(());
                }
                let error = format!("{program} exited with {}: {message}", output.status);
                if first_execution_error.is_none() {
                    first_execution_error = Some(error.clone());
                }
                last_error = Some(error);
            }
            Err(err) => {
                last_error = Some(format!("{program}: {err}"));
            }
        }
    }

    if command_not_found {
        return Err(anyhow!(
            "{}: {}",
            error_code,
            plugin_cli_missing_guidance(
                candidates
                    .first()
                    .map(|(program, _)| *program)
                    .unwrap_or("plugin")
            )
        ));
    }

    Err(anyhow!(
        "{}: {}",
        error_code,
        first_execution_error
            .or(last_error)
            .unwrap_or_else(|| format!(
                "{} command not found",
                candidates
                    .first()
                    .map(|(program, _)| *program)
                    .unwrap_or("plugin")
            ))
    ))
}

fn plugin_cli_missing_guidance(program: &str) -> String {
    match program {
        "claude" => concat!(
            "未找到 Claude Code CLI，无法自动安装 Claude plugin。请先安装 Claude Code CLI 后重试：",
            "Windows PowerShell 运行 `irm https://claude.ai/install.ps1 | iex`，",
            "或运行 `winget install Anthropic.ClaudeCode`；",
            "也可以使用 npm：`npm install -g @anthropic-ai/claude-code`。"
        )
        .to_string(),
        "codex" => {
            "未找到 Codex CLI，无法自动安装 Codex plugin。请先安装 Codex CLI 后重试。".to_string()
        }
        other => format!("未找到 {other} CLI，无法自动安装 plugin。请先安装对应 CLI 后重试。"),
    }
}

fn is_plugin_missing_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("not installed")
        || lower.contains("not found")
        || lower.contains("no plugin")
        || lower.contains("unknown plugin")
}

fn is_plugin_already_configured_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    (lower.contains("already") || lower.contains("duplicate")) && lower.contains("marketplace")
}

fn is_plugin_already_installed_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("already") && lower.contains("installed")
}

fn is_plugin_already_enabled_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("already") && lower.contains("enabled")
}

fn user_home_dir() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let path = PathBuf::from(home);
        if !path.as_os_str().is_empty() {
            return Ok(path);
        }
    }
    Err(anyhow!(
        "PLUGIN_MARKETPLACE_WRITE_FAILED: cannot resolve user home directory"
    ))
}

fn set_plugin_binding_enabled_inner(
    request: SetPluginBindingEnabledRequest,
    state: &AppState,
) -> Result<PluginBinding> {
    let conn = state.conn.lock().expect("db mutex poisoned");
    let binding = find_plugin_binding(&conn, &request.binding_id)?;
    let package_dir: String = conn
        .query_row(
            "SELECT package_path FROM plugin_packages WHERE id = ?1",
            params![binding.package_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            anyhow!("PLUGIN_MARKETPLACE_WRITE_FAILED: plugin cache package not found")
        })?;
    let marketplace_root = prepare_plugin_marketplace_root(
        state,
        &binding.target,
        &binding.scope,
        binding.project_path.as_deref(),
    )?;

    if request.enabled {
        validation::validate_plugin_target_scope(&binding.target, &binding.scope)?;
        ensure_plugin_scope_can_enable(
            &conn,
            Some(&binding.id),
            &binding.namespace,
            &binding.plugin_id,
            &binding.target,
            &binding.scope,
        )?;
        let package_dir = PathBuf::from(&package_dir);
        if !package_dir.exists() {
            return Err(anyhow!(
                "PLUGIN_MARKETPLACE_WRITE_FAILED: plugin cache package missing: {}",
                canonical_display_path(&package_dir)
            ));
        }
        let plugin_dir_name = format!("{}.{}", binding.namespace, binding.plugin_id);
        let plugin_dir = marketplace_root.join("plugins").join(&plugin_dir_name);
        if plugin_dir.exists() {
            fs::remove_dir_all(&plugin_dir)
                .context("PLUGIN_MARKETPLACE_WRITE_FAILED: clean plugin marketplace dir failed")?;
        }
        fs::create_dir_all(&plugin_dir)?;
        package::copy_dir_recursive_including_json(&package_dir, &plugin_dir)?;
        let plugin = market_plugin_from_binding(&binding);
        rewrite_plugin_marketplace_entry(
            &binding.target,
            &marketplace_root,
            &plugin_dir_name,
            &plugin,
            &binding.version,
            &binding.marketplace_name,
        )?;
        sync_codex_plugin_install(
            &binding.target,
            &binding.scope,
            &binding.plugin_id,
            &binding.marketplace_name,
            &marketplace_root,
        )?;
        sync_claude_plugin_install(
            &binding.target,
            &binding.plugin_id,
            &binding.marketplace_name,
            &binding.scope,
            binding.project_path.as_deref(),
            &marketplace_root,
        )?;
    } else {
        sync_codex_plugin_remove(
            &binding.target,
            &binding.scope,
            &binding.plugin_id,
            &binding.marketplace_name,
        )?;
        sync_claude_plugin_remove(
            &binding.target,
            &binding.plugin_id,
            &binding.marketplace_name,
            &binding.scope,
            binding.project_path.as_deref(),
            ClaudePluginRemoveAction::Disable,
        )?;
        remove_plugin_from_marketplace_file(
            &binding.target,
            &marketplace_root,
            &binding.plugin_id,
        )?;
    }

    conn.execute(
        "UPDATE plugin_bindings
         SET enabled = ?1, status = 'installed', updated_at = ?2
         WHERE id = ?3",
        params![
            if request.enabled { 1_i64 } else { 0_i64 },
            now(),
            request.binding_id
        ],
    )?;
    let updated = find_plugin_binding(&conn, &binding.id)?;
    let action = if request.enabled {
        "enable_plugin"
    } else {
        "disable_plugin"
    };
    let plugin_ref = format!("{}/{}", updated.namespace, updated.plugin_id);
    insert_audit(
        &conn,
        action,
        Some(&plugin_ref),
        "success",
        Some(&updated.target),
    )?;
    Ok(updated)
}

fn uninstall_plugin_inner(
    request: UninstallPluginRequest,
    state: &AppState,
) -> Result<Vec<PluginBinding>> {
    let conn = state.conn.lock().expect("db mutex poisoned");
    let binding = find_plugin_binding(&conn, &request.binding_id)?;
    let marketplace_root = prepare_plugin_marketplace_root(
        state,
        &binding.target,
        &binding.scope,
        binding.project_path.as_deref(),
    )?;
    sync_codex_plugin_remove(
        &binding.target,
        &binding.scope,
        &binding.plugin_id,
        &binding.marketplace_name,
    )?;
    sync_claude_plugin_remove(
        &binding.target,
        &binding.plugin_id,
        &binding.marketplace_name,
        &binding.scope,
        binding.project_path.as_deref(),
        ClaudePluginRemoveAction::Uninstall,
    )?;
    remove_plugin_from_marketplace_file(&binding.target, &marketplace_root, &binding.plugin_id)?;

    let plugin_dir = marketplace_root
        .join("plugins")
        .join(format!("{}.{}", binding.namespace, binding.plugin_id));
    if plugin_dir.exists() {
        fs::remove_dir_all(&plugin_dir)
            .context("PLUGIN_MARKETPLACE_WRITE_FAILED: remove plugin materialized dir failed")?;
    }

    if request.delete_cached_package {
        let package_path: Option<String> = conn
            .query_row(
                "SELECT package_path FROM plugin_packages WHERE id = ?1",
                params![binding.package_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(package_path) = package_path {
            let path = PathBuf::from(&package_path);
            if path.exists() {
                fs::remove_dir_all(&path).context(
                    "PLUGIN_MARKETPLACE_WRITE_FAILED: remove cached plugin package failed",
                )?;
            }
            conn.execute(
                "DELETE FROM plugin_packages WHERE id = ?1",
                params![binding.package_id],
            )?;
        }
    }

    conn.execute(
        "DELETE FROM plugin_bindings WHERE id = ?1",
        params![request.binding_id],
    )?;
    let plugin_ref = format!("{}/{}", binding.namespace, binding.plugin_id);
    insert_audit(
        &conn,
        "uninstall_plugin",
        Some(&plugin_ref),
        "success",
        Some(&binding.target),
    )?;
    list_plugin_bindings_inner(&conn)
}

pub(crate) fn find_plugin_binding(
    conn: &rusqlite::Connection,
    binding_id: &str,
) -> Result<PluginBinding> {
    list_plugin_bindings_inner(conn)?
        .into_iter()
        .find(|binding| binding.id == binding_id)
        .ok_or_else(|| anyhow!("未找到 plugin 绑定记录"))
}

fn market_plugin_from_binding(binding: &PluginBinding) -> MarketPlugin {
    MarketPlugin {
        source_id: binding.source_id.clone(),
        namespace: binding.namespace.clone(),
        id: binding.plugin_id.clone(),
        name: binding.plugin_name.clone(),
        summary: String::new(),
        latest_version: binding.version.clone(),
        manifest_path: String::new(),
        categories: Vec::new(),
        tags: Vec::new(),
        targets: vec![binding.target.clone()],
        scopes: vec![binding.scope.clone()],
        components: Vec::new(),
        risk_level: "unknown".to_string(),
        cached_versions: vec![binding.version.clone()],
        installed_bindings: Vec::new(),
        updated_at: Some(binding.updated_at.clone()),
    }
}

fn ensure_package_record(
    conn: &rusqlite::Connection,
    source_id: Option<&str>,
    namespace: &str,
    skill_id: &str,
    version: &str,
    package_path: &Path,
    sha256: Option<&str>,
) -> Result<String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM skill_packages
             WHERE COALESCE(source_id, '') = COALESCE(?1, '')
               AND namespace = ?2
               AND skill_id = ?3
               AND version = ?4",
            params![source_id, namespace, skill_id, version],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(id) = existing {
        conn.execute(
            "UPDATE skill_packages SET package_path = ?1, sha256 = ?2 WHERE id = ?3",
            params![canonical_display_path(package_path), sha256, id],
        )?;
        return Ok(id);
    }

    let id = new_id();
    fs::create_dir_all(&package_path)?;

    conn.execute(
        "INSERT INTO skill_packages
         (id, source_id, namespace, skill_id, version, package_path, sha256, cached_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            source_id,
            namespace,
            skill_id,
            version,
            canonical_display_path(package_path),
            sha256,
            now()
        ],
    )?;

    Ok(id)
}

fn ensure_plugin_package_record(
    conn: &rusqlite::Connection,
    source_id: Option<&str>,
    plugin: &MarketPlugin,
    version: &str,
    target: &str,
    package_path: &Path,
    sha256: Option<&str>,
    component_inventory_json: &str,
) -> Result<String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM plugin_packages
             WHERE COALESCE(source_id, '') = COALESCE(?1, '')
               AND namespace = ?2
               AND plugin_id = ?3
               AND version = ?4
               AND target = ?5",
            params![source_id, plugin.namespace, plugin.id, version, target],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(id) = existing {
        conn.execute(
            "UPDATE plugin_packages
             SET plugin_name = ?1,
                 package_path = ?2,
                 sha256 = ?3,
                 component_inventory_json = ?4,
                 risk_level = ?5,
                 cached_at = ?6
             WHERE id = ?7",
            params![
                plugin.name,
                canonical_display_path(package_path),
                sha256,
                component_inventory_json,
                plugin.risk_level,
                now(),
                id
            ],
        )?;
        return Ok(id);
    }

    let id = new_id();
    fs::create_dir_all(package_path)?;
    conn.execute(
        "INSERT INTO plugin_packages
         (id, source_id, namespace, plugin_id, plugin_name, version, target, package_path,
          sha256, component_inventory_json, risk_level, cached_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            id,
            source_id,
            plugin.namespace,
            plugin.id,
            plugin.name,
            version,
            target,
            canonical_display_path(package_path),
            sha256,
            component_inventory_json,
            plugin.risk_level,
            now()
        ],
    )?;
    Ok(id)
}

fn build_install_path(
    state: &AppState,
    target: &str,
    level: &str,
    project_path: Option<&str>,
    _namespace: &str,
    skill_id: &str,
) -> Result<PathBuf> {
    let root = if level == "personal" {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let root = list_target_roots_inner(&conn)?
            .into_iter()
            .find(|root| root.target == target)
            .ok_or_else(|| anyhow!("未配置 {target} 的个人级 skill 目录"))?;
        PathBuf::from(root.personal_path)
    } else {
        let project = project_path.ok_or_else(|| anyhow!("项目级启用必须选择项目目录"))?;
        resolve_project_skill_root(target, Path::new(project))
    };

    Ok(root.join(skill_id))
}

fn ensure_install_path_not_bound_to_other_skill(
    conn: &rusqlite::Connection,
    install_path: &Path,
    namespace: &str,
    skill_id: &str,
) -> Result<()> {
    let target_path = canonical_display_path(install_path);
    let conflict = list_bindings_inner(conn)?.into_iter().find(|binding| {
        canonical_display_path(Path::new(&binding.install_path)) == target_path
            && (binding.namespace != namespace || binding.skill_id != skill_id)
    });

    if let Some(binding) = conflict {
        Err(anyhow!(
            "目标目录已由 {} / {} 管理，不能覆盖安装",
            binding.namespace,
            binding.skill_id
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn resolve_project_skill_root(target: &str, project_path: &Path) -> PathBuf {
    match target {
        "codex" => project_path.join(".codex").join("skills"),
        "claude" => project_path.join(".claude").join("skills"),
        _ => project_path.join(".skillhub").join(target).join("skills"),
    }
}

pub(crate) fn find_binding(conn: &rusqlite::Connection, binding_id: &str) -> Result<SkillBinding> {
    list_bindings_inner(conn)?
        .into_iter()
        .find(|binding| binding.id == binding_id)
        .ok_or_else(|| anyhow!("未找到绑定记录"))
}

fn is_sqlite_managed_install_path(binding: &SkillBinding, path: &Path) -> bool {
    if !path.exists() || !path.is_dir() {
        return false;
    }

    let legacy_leaf = format!("{}.{}", binding.namespace, binding.skill_id);
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|leaf| leaf == binding.skill_id || leaf == legacy_leaf)
}

fn disabled_local_skill_path(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("本地 skill 路径缺少父目录"))?;
    let leaf = path
        .file_name()
        .ok_or_else(|| anyhow!("本地 skill 路径缺少目录名"))?;
    Ok(parent.join(local::DISABLED_SKILLS_DIR).join(leaf))
}

fn enabled_local_skill_path(path: &Path) -> Result<PathBuf> {
    let disabled_root = path
        .parent()
        .ok_or_else(|| anyhow!("禁用 skill 路径缺少父目录"))?;
    if disabled_root.file_name().and_then(|name| name.to_str()) != Some(local::DISABLED_SKILLS_DIR)
    {
        return Err(anyhow!("该本地 skill 不在禁用目录中，无法恢复启用"));
    }
    let root = disabled_root
        .parent()
        .ok_or_else(|| anyhow!("禁用 skill 路径缺少启用根目录"))?;
    let leaf = path
        .file_name()
        .ok_or_else(|| anyhow!("禁用 skill 路径缺少目录名"))?;
    Ok(root.join(leaf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_mac_rejection_does_not_expose_allowlist_macs() {
        let allowlist = admin_config::parse_mac_allowlist(
            r#"{
              "entries": [
                { "mac": "C1:7F:54:5C:60:D8", "role": "system" }
              ]
            }"#,
        )
        .expect("allowlist should parse");

        let err = authorize_admin_from_allowlist(
            admin_config::ADMIN_KEY,
            &[String::from("C8:7F:54:5C:60:D8")],
            &allowlist,
        )
        .expect_err("non-allowlisted mac should be rejected");
        let message = err.to_string();

        assert!(message.contains("C8:7F:54:5C:60:D8"));
        assert!(!message.contains("C1:7F:54:5C:60:D8"));
        assert!(!message.contains("白名单:"));
    }

    #[test]
    fn admin_audit_envelope_includes_authorized_mac() {
        let authorization = admin_config::AdminAuthorization {
            role: "project".to_string(),
            projects: vec![],
            mac_address: "C8:7F:54:5C:60:D8".to_string(),
            name: Some("系统管理员".to_string()),
        };

        let envelope = admin_audit_envelope(
            "saveMarketProject",
            &authorization,
            serde_json::json!({ "slug": "live-project" }),
        );

        assert_eq!(envelope["action"], "saveMarketProject");
        assert_eq!(envelope["actor"], "系统管理员");
        assert_eq!(envelope["role"], "project");
        assert_eq!(envelope["macAddress"], "C8:7F:54:5C:60:D8");
        assert_eq!(envelope["payload"]["slug"], "live-project");
    }

    #[test]
    fn admin_audit_log_normalizes_wrapped_payload_identity() {
        let record = admin_audit_log_from_value(
            "admin/audit/2026/06/14/saveMarketProject-abc.json",
            serde_json::json!({
                "schema": "skillhub.admin-audit.v1",
                "action": "saveMarketProject",
                "actor": "系统管理员",
                "role": "project",
                "macAddress": "C8:7F:54:5C:60:D8",
                "payload": {
                    "slug": "live-project"
                },
                "createdAt": "2026-06-14T10:20:30Z"
            }),
        )
        .expect("audit record should normalize");

        assert_eq!(record.action, "saveMarketProject");
        assert_eq!(record.actor.as_deref(), Some("系统管理员"));
        assert_eq!(record.role.as_deref(), Some("project"));
        assert_eq!(record.mac_address.as_deref(), Some("C8:7F:54:5C:60:D8"));
        assert_eq!(record.target.as_deref(), Some("live-project"));
        assert!(record.summary.contains("live-project"));
        assert_eq!(record.created_at, "2026-06-14T10:20:30Z");
    }

    #[test]
    fn project_admin_cannot_view_admin_audit_logs() {
        let authorization = admin_config::AdminAuthorization {
            role: "project".to_string(),
            projects: vec![],
            mac_address: "C8:7F:54:5C:60:D8".to_string(),
            name: Some("项目管理员".to_string()),
        };

        let err = ensure_can_view_admin_audit(&authorization)
            .expect_err("project admin should not view audit logs");

        assert!(err.to_string().contains("系统管理员"));
    }

    #[test]
    fn local_skill_enable_disable_paths_round_trip() {
        let active = PathBuf::from(r"C:\Users\ctf19\.codex\skills\daily-note-helper");
        let disabled = disabled_local_skill_path(&active).expect("disabled path should resolve");
        assert_eq!(
            disabled,
            PathBuf::from(r"C:\Users\ctf19\.codex\skills\.skill-hub-disabled\daily-note-helper")
        );
        assert_eq!(
            enabled_local_skill_path(&disabled).expect("enabled path should resolve"),
            active
        );
    }

    #[test]
    fn local_skill_enable_path_requires_disabled_root() {
        let active = PathBuf::from(r"C:\Users\ctf19\.codex\skills\daily-note-helper");
        assert!(enabled_local_skill_path(&active).is_err());
    }

    #[test]
    fn parses_skill_markdown_admin_fields() {
        let content = r#"---
version: 1.2.3
author: "Skill Hub"
---

# Demo
"#;

        assert_eq!(
            parse_skill_markdown_field(content, "version").as_deref(),
            Some("1.2.3")
        );
        assert_eq!(
            parse_skill_markdown_field(content, "author").as_deref(),
            Some("Skill Hub")
        );
    }

    #[test]
    fn validates_publish_meta_completeness() {
        let meta = PublishMeta {
            namespace: "community".to_string(),
            skill_id: "demo".to_string(),
            version: None,
            name: "Demo".to_string(),
            summary: "Demo skill".to_string(),
            tags: vec![],
            targets: vec![],
            levels: vec!["personal".to_string()],
            publish_scope: "project".to_string(),
            publish_category_slug: None,
            publish_project_slug: Some("project-a".to_string()),
            changelog: String::new(),
            updated_at: None,
            updated_by: None,
        };

        assert!(validation::validate_publish_meta(&meta).is_ok());
        assert_eq!(
            publish_categories(&meta),
            vec!["project:project-a".to_string()]
        );
    }

    #[test]
    fn existing_version_can_republish_only_when_missing_from_catalog() {
        let meta = PublishMeta {
            namespace: "community".to_string(),
            skill_id: "demo".to_string(),
            version: None,
            name: "Demo".to_string(),
            summary: "Demo skill".to_string(),
            tags: vec![],
            targets: vec![],
            levels: vec!["personal".to_string()],
            publish_scope: "public".to_string(),
            publish_category_slug: Some("general".to_string()),
            publish_project_slug: None,
            changelog: String::new(),
            updated_at: None,
            updated_by: None,
        };
        let manifest = SkillManifest {
            schema: "skillhub.skill-manifest.v1".to_string(),
            namespace: meta.namespace.clone(),
            id: meta.skill_id.clone(),
            name: meta.name.clone(),
            summary: meta.summary.clone(),
            categories: vec!["general".to_string()],
            tags: vec![],
            targets: vec![],
            levels: vec!["personal".to_string()],
            latest_version: "1.0.0".to_string(),
            versions: vec![SkillVersion {
                version: "1.0.0".to_string(),
                skill_path: "skills/community/demo/versions/1.0.0/skill.json".to_string(),
                package_path: "skills/community/demo/versions/1.0.0/package.zip".to_string(),
                sha256_path: "skills/community/demo/versions/1.0.0/package.sha256".to_string(),
                changelog_path: None,
                signature_path: None,
                created_at: None,
                package: None,
            }],
            updated_at: None,
        };
        let empty_catalog = CatalogDoc {
            schema: "skillhub.catalog.v1".to_string(),
            generated_at: None,
            categories: vec![],
            skills: vec![],
        };
        assert_eq!(
            should_republish_existing_version(&manifest, &empty_catalog, &meta, "1.0.0").unwrap(),
            true
        );

        let active_catalog = CatalogDoc {
            skills: vec![MarketSkill {
                namespace: meta.namespace.clone(),
                id: meta.skill_id.clone(),
                name: meta.name.clone(),
                summary: meta.summary.clone(),
                latest_version: "1.0.0".to_string(),
                categories: vec!["general".to_string()],
                tags: vec![],
                targets: vec![],
                levels: vec!["personal".to_string()],
                manifest_path: "skills/community/demo/manifest.json".to_string(),
                updated_at: None,
                source_id: None,
                installed_bindings: vec![],
                cached_versions: vec![],
            }],
            ..empty_catalog
        };
        assert!(
            should_republish_existing_version(&manifest, &active_catalog, &meta, "1.0.0").is_err()
        );
    }

    #[test]
    fn parses_gitlab_draft_category_paths_from_skill_location() {
        let single = parse_gitlab_source_path("product/prd-shaper");
        assert_eq!(single.category_path, vec!["product".to_string()]);
        assert_eq!(single.category_code().as_deref(), Some("product"));
        assert_eq!(single.draft_slug.as_deref(), Some("prd-shaper"));

        let nested = parse_gitlab_source_path("general/product/prd-shaper");
        assert_eq!(
            nested.category_path,
            vec!["general".to_string(), "product".to_string()]
        );
        assert_eq!(nested.category_code().as_deref(), Some("general/product"));
        assert_eq!(nested.draft_slug.as_deref(), Some("prd-shaper"));

        assert_ne!(single.category_code(), nested.category_code());
    }

    #[test]
    fn parses_plugin_draft_multi_level_category_path() {
        let nested = parse_gitlab_source_path("backend/java/commit-workflow");
        assert_eq!(
            nested.category_path,
            vec!["backend".to_string(), "java".to_string()]
        );
        assert_eq!(nested.category_code().as_deref(), Some("backend/java"));
        assert_eq!(nested.draft_slug.as_deref(), Some("commit-workflow"));
    }

    #[test]
    fn plugin_publish_generates_target_packages_from_common_source() {
        let files = sample_plugin_files(vec![
            (
                "README.md",
                br#"---
name: Commit Workflow
description: Team commit and PR workflow plugin.
version: 1.0.0
author: skill-hub
---
# Commit Workflow
"#
                .to_vec(),
            ),
            ("CHANGELOG.md", b"## 1.0.0\n".to_vec()),
            ("skills/review/SKILL.md", b"# Review\n".to_vec()),
            ("agents/reviewer.md", b"agent".to_vec()),
        ]);

        let prepared =
            prepare_plugin_publish(&files, None).expect("common plugin source should publish");
        let codex_package = prepared.packages.get("codex").expect("codex package");
        let claude_package = prepared.packages.get("claude").expect("claude package");

        let mut codex_archive =
            ZipArchive::new(Cursor::new(codex_package.bytes.clone())).expect("codex zip opens");
        assert!(codex_archive.by_name(".codex-plugin/plugin.json").is_ok());
        assert!(codex_archive.by_name("skills/review/SKILL.md").is_ok());
        assert!(codex_archive
            .by_name("codex/.codex-plugin/plugin.json")
            .is_err());
        assert!(codex_archive
            .by_name("claude/.claude-plugin/plugin.json")
            .is_err());

        let mut claude_archive =
            ZipArchive::new(Cursor::new(claude_package.bytes.clone())).expect("claude zip opens");
        assert!(claude_archive.by_name(".claude-plugin/plugin.json").is_ok());
        assert!(claude_archive.by_name("skills/review/SKILL.md").is_ok());
        assert!(claude_archive.by_name("agents/reviewer.md").is_ok());
        assert!(claude_archive
            .by_name("codex/.codex-plugin/plugin.json")
            .is_err());
        assert!(claude_archive
            .by_name("claude/.claude-plugin/plugin.json")
            .is_err());
        assert_eq!(prepared.risk_level, "medium");
    }

    #[test]
    fn plugin_publish_rejects_readme_without_required_frontmatter() {
        let files = vec![
            ("README.md".to_string(), b"# Commit Workflow\n".to_vec()),
            ("skills/review/SKILL.md".to_string(), b"# Review\n".to_vec()),
        ];
        let saved = PublishMeta {
            namespace: "internal".to_string(),
            skill_id: "commit-workflow".to_string(),
            version: Some("1.0.0".to_string()),
            name: "Commit Workflow".to_string(),
            summary: "Team commit and PR workflow plugin.".to_string(),
            tags: vec!["git".to_string()],
            targets: vec!["codex".to_string(), "claude".to_string()],
            levels: vec!["user".to_string(), "project".to_string()],
            publish_scope: "public".to_string(),
            publish_category_slug: Some("backend".to_string()),
            publish_project_slug: None,
            changelog: String::new(),
            updated_at: None,
            updated_by: None,
        };

        let err = prepare_plugin_publish(&files, Some(saved))
            .expect_err("README front matter should be required");

        assert!(err.to_string().contains("README.md 缺少 name"));
    }

    #[test]
    fn plugin_publish_can_use_readme_frontmatter_without_pluginhub() {
        let files = vec![
            (
                "README.md".to_string(),
                br#"---
name: webapp-testing
description: Toolkit for interacting with and testing local web applications using Playwright.
metadata:
  version: 1.0.0
  author: skill-hub
  tags: [web, testing]
---
# Webapp Testing
"#
                .to_vec(),
            ),
            ("skills/review/SKILL.md".to_string(), b"# Review\n".to_vec()),
        ];
        let saved = PublishMeta {
            namespace: "internal".to_string(),
            skill_id: "webapp-testing".to_string(),
            version: Some("1.0.0".to_string()),
            name: String::new(),
            summary: String::new(),
            tags: Vec::new(),
            targets: vec!["codex".to_string()],
            levels: vec!["user".to_string(), "project".to_string()],
            publish_scope: "public".to_string(),
            publish_category_slug: Some("testing".to_string()),
            publish_project_slug: None,
            changelog: String::new(),
            updated_at: None,
            updated_by: None,
        };

        let prepared = prepare_plugin_publish(&files, Some(saved))
            .expect("README front matter should fill plugin source metadata");

        assert_eq!(prepared.meta.id, "webapp-testing");
        assert_eq!(prepared.meta.name, "webapp-testing");
        assert_eq!(
            prepared.meta.summary,
            "Toolkit for interacting with and testing local web applications using Playwright."
        );
        assert_eq!(prepared.meta.version, "1.0.0");
        assert_eq!(prepared.meta.tags, vec!["web", "testing"]);
        assert!(prepared.packages.contains_key("codex"));
    }

    #[test]
    fn plugin_publish_meta_overrides_source_publish_target() {
        let source = PluginSourceMeta {
            schema: "skillhub.plugin-source.v1".to_string(),
            namespace: "internal".to_string(),
            id: "commit-workflow".to_string(),
            name: "Commit Workflow".to_string(),
            version: "1.0.0".to_string(),
            summary: "From pluginhub".to_string(),
            author: None,
            categories: vec!["backend".to_string()],
            tags: vec!["git".to_string()],
            targets: vec!["codex".to_string(), "claude".to_string()],
            scopes: vec!["user".to_string(), "project".to_string()],
            components: vec!["skills".to_string()],
            risk_level: Some("low".to_string()),
            publish_scope: Some("public".to_string()),
            publish_project_slug: None,
            platforms: serde_json::json!({}),
        };
        let saved = PublishMeta {
            namespace: "ignored".to_string(),
            skill_id: "ignored".to_string(),
            version: Some("1.1.0".to_string()),
            name: "Managed Commit Workflow".to_string(),
            summary: "Managed summary".to_string(),
            tags: vec!["managed".to_string()],
            targets: vec!["claude".to_string()],
            levels: vec!["project".to_string()],
            publish_scope: "project".to_string(),
            publish_category_slug: None,
            publish_project_slug: Some("alpha".to_string()),
            changelog: "Managed changelog".to_string(),
            updated_at: None,
            updated_by: None,
        };

        let merged = apply_plugin_publish_meta(source, Some(saved));

        assert_eq!(merged.name, "Managed Commit Workflow");
        assert_eq!(merged.summary, "Managed summary");
        assert_eq!(merged.tags, vec!["managed"]);
        assert_eq!(merged.targets, vec!["claude"]);
        assert_eq!(merged.scopes, vec!["project"]);
        assert_eq!(merged.publish_scope.as_deref(), Some("project"));
        assert_eq!(merged.publish_project_slug.as_deref(), Some("alpha"));
        assert_eq!(plugin_publish_categories(&merged), vec!["project:alpha"]);
    }

    #[test]
    fn plugin_publish_rejects_platform_generated_directories_in_source() {
        let files = sample_plugin_files(vec![(
            "codex/.codex-plugin/plugin.json",
            br#"{"name":"Commit Workflow"}"#.to_vec(),
        )]);
        let err =
            prepare_plugin_publish(&files, None).expect_err("platform directories should fail");
        assert!(err.to_string().contains("PLUGIN_SOURCE_INVALID"));
    }

    #[test]
    fn plugin_package_filters_target_specific_common_files() {
        let files = sample_plugin_files(vec![
            ("skills/review/SKILL.md", b"# Review\n".to_vec()),
            ("agents/reviewer.md", b"agent".to_vec()),
            (".app.json", br#"{"apps":[]}"#.to_vec()),
        ]);
        let prepared = prepare_plugin_publish(&files, None).expect("plugin publish should prepare");
        let codex_package = prepared.packages.get("codex").expect("codex package");
        let mut archive =
            ZipArchive::new(Cursor::new(codex_package.bytes.clone())).expect("zip opens");
        assert!(archive.by_name(".codex-plugin/plugin.json").is_ok());
        assert!(archive.by_name("skills/review/SKILL.md").is_ok());
        assert!(archive.by_name(".app.json").is_ok());
        assert!(archive.by_name("agents/reviewer.md").is_err());
        assert_eq!(prepared.risk_level, "medium");
    }

    #[test]
    fn plugin_package_rejects_path_traversal() {
        let files = sample_plugin_files(vec![
            ("skills/review/SKILL.md", b"# Review\n".to_vec()),
            ("../escape.txt", b"bad".to_vec()),
        ]);
        let err = prepare_plugin_publish(&files, None).expect_err("unsafe path should fail");
        assert!(err.to_string().contains("PLUGIN_PACKAGE_BUILD_FAILED"));
    }

    #[test]
    fn plugin_marketplace_files_point_to_materialized_plugin_dir() {
        let root = std::env::temp_dir().join(format!("skillhub-plugin-market-{}", new_id()));
        let plugin = MarketPlugin {
            namespace: "internal".to_string(),
            id: "commit-workflow".to_string(),
            name: "Commit Workflow".to_string(),
            summary: "Team workflow plugin.".to_string(),
            latest_version: "1.0.0".to_string(),
            categories: vec!["backend".to_string()],
            tags: vec![],
            targets: vec!["codex".to_string()],
            scopes: vec!["user".to_string()],
            components: vec!["skills".to_string()],
            risk_level: "low".to_string(),
            manifest_path: "plugins/internal/commit-workflow/manifest.json".to_string(),
            updated_at: None,
            source_id: Some("compiled-source".to_string()),
            installed_bindings: vec![],
            cached_versions: vec![],
        };
        let codex_path = write_plugin_marketplace_file(
            "codex",
            &root,
            "internal.commit-workflow",
            &plugin,
            "1.0.0",
            "skillhub",
        )
        .expect("write codex marketplace");
        assert_eq!(
            canonical_display_path(&codex_path),
            canonical_display_path(
                &root
                    .join(".agents")
                    .join("plugins")
                    .join("marketplace.json")
            )
        );
        let codex_doc: serde_json::Value =
            serde_json::from_slice(&fs::read(codex_path).expect("read marketplace")).expect("json");
        assert_eq!(
            codex_doc["plugins"][0]["source"]["path"],
            "./plugins/internal.commit-workflow"
        );
        assert_eq!(codex_doc["plugins"][0]["source"]["source"], "local");
        assert_eq!(
            codex_doc["plugins"][0]["policy"]["installation"],
            "AVAILABLE"
        );

        let claude_path = write_plugin_marketplace_file(
            "claude",
            &root,
            "internal.commit-workflow",
            &plugin,
            "1.0.0",
            "skillhub",
        )
        .expect("write claude marketplace");
        let claude_doc: serde_json::Value =
            serde_json::from_slice(&fs::read(claude_path).expect("read marketplace"))
                .expect("json");
        assert_eq!(
            claude_doc["plugins"][0]["source"],
            "./plugins/internal.commit-workflow"
        );
        assert_eq!(claude_doc["owner"]["name"], "Skill Hub");
        fs::remove_dir_all(root).expect("remove temp dir");
    }

    #[test]
    fn plugin_marketplace_upsert_preserves_other_entries() {
        let existing = serde_json::json!({
            "name": "old",
            "plugins": [
                { "name": "alpha", "source": { "path": "./plugins/alpha" } },
                { "name": "commit-workflow", "source": { "path": "./plugins/old" } }
            ]
        });
        let next = upsert_plugin_marketplace_entry(
            existing,
            "skillhub",
            Some(serde_json::json!(1)),
            serde_json::json!({
                "name": "commit-workflow",
                "source": { "path": "./plugins/internal.commit-workflow" }
            }),
        );
        let plugins = next["plugins"].as_array().expect("plugins array");
        assert_eq!(plugins.len(), 2);
        assert_eq!(plugins[0]["name"], "alpha");
        assert_eq!(
            plugins[1]["source"]["path"],
            "./plugins/internal.commit-workflow"
        );
    }

    #[test]
    fn claude_marketplace_schema_backfills_missing_owner() {
        let next = ensure_claude_marketplace_schema(serde_json::json!({
            "name": "skillhub",
            "plugins": []
        }));

        assert!(next["owner"].is_object());
        assert_eq!(next["owner"]["name"], "Skill Hub");
    }

    #[test]
    fn plugin_marketplace_remove_preserves_other_entries() {
        let existing = serde_json::json!({
            "name": "skillhub",
            "plugins": [
                { "name": "alpha", "source": { "path": "./plugins/alpha" } },
                { "name": "commit-workflow", "source": { "path": "./plugins/internal.commit-workflow" } }
            ]
        });
        let next = remove_plugin_marketplace_entry(existing, "commit-workflow");
        let plugins = next["plugins"].as_array().expect("plugins array");
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0]["name"], "alpha");
    }

    #[test]
    fn delete_cached_plugin_rejects_any_binding_relation() {
        let (state, package_id, package_dir) = plugin_cache_test_state(Some("disabled"));
        let request = DeleteCachedPluginRequest {
            source_id: Some("compiled-source".to_string()),
            namespace: "internal".to_string(),
            plugin_id: "commit-workflow".to_string(),
            version: "1.0.0".to_string(),
            target: "codex".to_string(),
        };

        let err = delete_cached_plugin_inner(request, &state)
            .expect_err("any binding relation should block cache deletion");

        assert!(err.to_string().contains("bindings"));
        assert!(package_dir.exists());

        let conn = state.conn.lock().expect("db mutex poisoned");
        let package_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM plugin_packages WHERE id = ?1",
                params![package_id],
                |row| row.get(0),
            )
            .expect("count packages");
        assert_eq!(package_count, 1);
    }

    #[test]
    fn delete_cached_plugin_removes_unbound_cache_package() {
        let (state, package_id, package_dir) = plugin_cache_test_state(None);
        let request = DeleteCachedPluginRequest {
            source_id: Some("compiled-source".to_string()),
            namespace: "internal".to_string(),
            plugin_id: "commit-workflow".to_string(),
            version: "1.0.0".to_string(),
            target: "codex".to_string(),
        };

        delete_cached_plugin_inner(request, &state).expect("unbound cache should delete");

        assert!(!package_dir.exists());

        let conn = state.conn.lock().expect("db mutex poisoned");
        let package_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM plugin_packages WHERE id = ?1",
                params![package_id],
                |row| row.get(0),
            )
            .expect("count packages");
        let metadata_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM local_package_metadata WHERE package_id = ?1",
                params![package_id],
                |row| row.get(0),
            )
            .expect("count metadata");
        assert_eq!(package_count, 0);
        assert_eq!(metadata_count, 0);
    }

    #[test]
    fn plugin_draft_content_prefix_prefers_flat_plugin_root() {
        let root = "draft/gitlab/plugins/backend/java/commit-workflow/";
        let objects = vec![
            "draft/gitlab/plugins/backend/java/commit-workflow/pluginhub.json".to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/skills/review/SKILL.md".to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/source/pluginhub.json".to_string(),
        ];
        let resolved =
            resolve_plugin_draft_content_prefix(root, &objects).expect("flat root should resolve");
        assert_eq!(resolved.prefix, root);
        assert_eq!(
            resolved.pluginhub_path,
            "draft/gitlab/plugins/backend/java/commit-workflow/pluginhub.json"
        );

        let files = collect_plugin_draft_preview_file_list(&resolved.prefix, &objects);
        let paths = files.into_iter().map(|file| file.path).collect::<Vec<_>>();
        assert_eq!(
            paths,
            vec![
                "pluginhub.json".to_string(),
                "skills/review/SKILL.md".to_string()
            ]
        );
    }

    #[test]
    fn plugin_draft_content_prefix_falls_back_to_legacy_source_root() {
        let root = "draft/gitlab/plugins/backend/java/commit-workflow/";
        let objects = vec![
            "draft/gitlab/plugins/backend/java/commit-workflow/source/pluginhub.json".to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/source/skills/review/SKILL.md"
                .to_string(),
        ];
        let resolved = resolve_plugin_draft_content_prefix(root, &objects)
            .expect("legacy source root should resolve");
        assert_eq!(
            resolved.prefix,
            "draft/gitlab/plugins/backend/java/commit-workflow/source/"
        );
        assert_eq!(
            resolved.pluginhub_path,
            "draft/gitlab/plugins/backend/java/commit-workflow/source/pluginhub.json"
        );
    }

    #[test]
    fn plugin_draft_content_prefix_uses_gitlab_source_when_flat_root_is_generated_artifact() {
        let root = "draft/gitlab/plugins/backend/java/commit-workflow/";
        let objects = vec![
            "draft/gitlab/plugins/backend/java/commit-workflow/pluginhub.json".to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/.codex-plugin/plugin.json"
                .to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/codex/.codex-plugin/plugin.json"
                .to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/source/pluginhub.json".to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/source/README.md".to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/source/skills/review/SKILL.md"
                .to_string(),
        ];

        let resolved = resolve_plugin_draft_content_prefix(root, &objects)
            .expect("GitLab source root should resolve");

        assert_eq!(
            resolved.prefix,
            "draft/gitlab/plugins/backend/java/commit-workflow/source/"
        );
        assert_eq!(
            resolved.pluginhub_path,
            "draft/gitlab/plugins/backend/java/commit-workflow/source/pluginhub.json"
        );

        let files = collect_plugin_draft_preview_file_list(&resolved.prefix, &objects);
        let paths = files.into_iter().map(|file| file.path).collect::<Vec<_>>();
        assert_eq!(
            paths,
            vec![
                "README.md".to_string(),
                "pluginhub.json".to_string(),
                "skills/review/SKILL.md".to_string()
            ]
        );
    }

    #[test]
    fn plugin_draft_preview_file_list_uses_plugin_content_prefix() {
        let prefix = "draft/gitlab/plugins/backend/java/commit-workflow/";
        let files = collect_plugin_draft_preview_file_list(
            prefix,
            &[
                "draft/gitlab/plugins/backend/java/commit-workflow/pluginhub.json".to_string(),
                "draft/gitlab/plugins/backend/java/commit-workflow/skills/review/SKILL.md"
                    .to_string(),
                "draft/gitlab/plugins/backend/java/commit-workflow/agents/reviewer.md".to_string(),
                "draft/gitlab/plugins/backend/java/commit-workflow/validation.json".to_string(),
            ],
        );
        let paths = files.into_iter().map(|file| file.path).collect::<Vec<_>>();
        assert_eq!(
            paths,
            vec![
                "agents/reviewer.md".to_string(),
                "pluginhub.json".to_string(),
                "skills/review/SKILL.md".to_string(),
                "validation.json".to_string()
            ]
        );
    }

    #[test]
    fn plugin_draft_preview_file_list_excludes_publish_meta() {
        let prefix = "draft/gitlab/plugins/backend/java/commit-workflow/";
        let files = collect_plugin_draft_preview_file_list(
            prefix,
            &[
                "draft/gitlab/plugins/backend/java/commit-workflow/pluginhub.json".to_string(),
                "draft/gitlab/plugins/backend/java/commit-workflow/README.md".to_string(),
                "draft/gitlab/plugins/backend/java/commit-workflow/publish-meta.v1.json"
                    .to_string(),
                "draft/gitlab/plugins/backend/java/commit-workflow/skills/review/SKILL.md"
                    .to_string(),
            ],
        );
        let paths = files.into_iter().map(|file| file.path).collect::<Vec<_>>();
        assert_eq!(
            paths,
            vec![
                "README.md".to_string(),
                "pluginhub.json".to_string(),
                "skills/review/SKILL.md".to_string()
            ]
        );
    }

    #[test]
    fn plugin_draft_source_paths_are_discovered_from_pluginhub_without_draft_json() {
        let objects = vec![
            "draft/gitlab/plugins/backend/java/commit-workflow/pluginhub.json".to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/README.md".to_string(),
            "draft/gitlab/plugins/productivity/release-helper/pluginhub.json".to_string(),
        ];
        let paths = collect_plugin_draft_source_paths(&objects);
        assert_eq!(
            paths,
            vec![
                "backend/java/commit-workflow".to_string(),
                "productivity/release-helper".to_string()
            ]
        );
    }

    #[test]
    fn plugin_draft_source_paths_include_directories_without_pluginhub() {
        let objects = vec![
            "draft/gitlab/plugins/productivity/automation/release-notes-helper/README.md"
                .to_string(),
            "draft/gitlab/plugins/productivity/automation/release-notes-helper/CHANGELOG.md"
                .to_string(),
            "draft/gitlab/plugins/productivity/automation/release-notes-helper/skills/write/SKILL.md"
                .to_string(),
            "draft/gitlab/plugins/productivity/automation/release-notes-helper/validation.json"
                .to_string(),
        ];
        let paths = collect_plugin_draft_source_paths(&objects);
        assert_eq!(
            paths,
            vec!["productivity/automation/release-notes-helper".to_string()]
        );
    }

    #[test]
    fn plugin_publish_can_synthesize_source_meta_from_readme_and_saved_publish_meta() {
        let files = vec![
            (
                "README.md".to_string(),
                br#"---
name: Release Notes Helper
description: Generate release notes from commits and PRs.
metadata:
  version: 0.2.0
  author: skill-hub
  tags: [release, automation]
---
# Release Notes Helper
"#
                .to_vec(),
            ),
            (
                "skills/write/SKILL.md".to_string(),
                b"# Write Release Notes\n".to_vec(),
            ),
            ("agents/reviewer.md".to_string(), b"agent".to_vec()),
        ];
        let saved = PublishMeta {
            namespace: "productivity".to_string(),
            skill_id: "release-notes-helper".to_string(),
            version: Some("0.2.0".to_string()),
            name: "Release Notes Helper".to_string(),
            summary: "Generate release notes from commits and PRs.".to_string(),
            tags: vec!["release".to_string(), "automation".to_string()],
            targets: vec!["codex".to_string(), "claude".to_string()],
            levels: vec!["user".to_string(), "project".to_string()],
            publish_scope: "public".to_string(),
            publish_category_slug: Some("productivity".to_string()),
            publish_project_slug: None,
            changelog: "Initial plugin publish.".to_string(),
            updated_at: None,
            updated_by: None,
        };

        let prepared =
            prepare_plugin_publish(&files, Some(saved)).expect("saved meta should publish plugin");

        assert_eq!(prepared.meta.namespace, "productivity");
        assert_eq!(prepared.meta.id, "release-notes-helper");
        assert_eq!(prepared.meta.version, "0.2.0");
        assert_eq!(prepared.meta.targets, vec!["codex", "claude"]);
        assert_eq!(prepared.meta.scopes, vec!["user", "project"]);
        assert_eq!(prepared.meta.components, vec!["skills", "agents"]);
        assert!(prepared.packages.contains_key("codex"));
        assert!(prepared.packages.contains_key("claude"));
    }

    #[test]
    fn default_plugin_publish_meta_does_not_take_market_scope_from_pluginhub() {
        let source = PluginSourceMeta {
            schema: "skillhub.plugin-source.v1".to_string(),
            namespace: "internal".to_string(),
            id: "commit-workflow".to_string(),
            name: "Commit Workflow".to_string(),
            version: "1.0.0".to_string(),
            summary: "Workflow helper".to_string(),
            author: None,
            categories: vec!["backend".to_string()],
            tags: vec!["git".to_string()],
            targets: vec!["codex".to_string()],
            scopes: vec!["user".to_string(), "project".to_string()],
            components: vec!["skills".to_string()],
            risk_level: Some("low".to_string()),
            publish_scope: Some("project".to_string()),
            publish_project_slug: Some("alpha".to_string()),
            platforms: serde_json::Value::Null,
        };
        let meta = default_plugin_publish_meta(&source);
        assert_eq!(meta.publish_scope, "public");
        assert_eq!(meta.publish_category_slug, None);
        assert_eq!(meta.publish_project_slug, None);
        assert_eq!(meta.targets, vec!["codex".to_string()]);
    }

    #[test]
    fn claude_plugin_marketplace_path_resolves_from_marketplace_dir() {
        let root = PathBuf::from("/tmp/project");
        let resolved = local::resolve_plugin_source_path(
            "claude",
            &root,
            "./plugins/internal.commit-workflow",
        );
        assert_eq!(
            canonical_display_path(&resolved),
            "/tmp/project/./plugins/internal.commit-workflow"
        );
    }

    #[test]
    fn claude_project_marketplace_root_lives_under_project_claude_dir() {
        let state = AppState {
            conn: std::sync::Arc::new(std::sync::Mutex::new(
                rusqlite::Connection::open_in_memory().expect("open sqlite"),
            )),
            app_dir: PathBuf::from("/tmp/skillhub-app"),
            local_macs: vec![],
        };

        let claude_root =
            plugin_marketplace_root(&state, "claude", "project", Some("/tmp/project-a"))
                .expect("resolve claude project marketplace root");
        assert_eq!(
            canonical_display_path(&claude_root),
            "/tmp/project-a/.claude/skillhub-plugin-marketplace"
        );

        let codex_root =
            plugin_marketplace_root(&state, "codex", "project", Some("/tmp/project-a"))
                .expect("resolve codex project marketplace root");
        assert_eq!(canonical_display_path(&codex_root), "/tmp/project-a");
    }

    #[test]
    fn claude_project_marketplace_migration_moves_legacy_skillhub_root_under_claude_dir() {
        let project = std::env::temp_dir().join(format!("skillhub-claude-project-{}", new_id()));
        let legacy_marketplace_dir = project.join(".claude-plugin");
        let legacy_plugin_dir = project.join("plugins").join("internal.commit-workflow");
        fs::create_dir_all(&legacy_marketplace_dir).expect("create legacy marketplace dir");
        fs::create_dir_all(&legacy_plugin_dir).expect("create legacy plugin dir");
        fs::write(
            legacy_marketplace_dir.join("marketplace.json"),
            br#"{
              "name": "skillhub",
              "owner": { "name": "Skill Hub" },
              "plugins": [
                {
                  "name": "commit-workflow",
                  "version": "1.0.0",
                  "source": "./plugins/internal.commit-workflow"
                }
              ]
            }"#,
        )
        .expect("write legacy marketplace");
        fs::write(legacy_plugin_dir.join("README.md"), "# Commit Workflow\n")
            .expect("write legacy plugin file");

        let new_root = project.join(".claude").join("skillhub-plugin-marketplace");
        migrate_claude_project_marketplace_root(
            "claude",
            "project",
            Some(project.to_string_lossy().as_ref()),
            &new_root,
        )
        .expect("migrate legacy marketplace");

        assert!(new_root
            .join(".claude-plugin")
            .join("marketplace.json")
            .is_file());
        assert!(new_root
            .join("plugins")
            .join("internal.commit-workflow")
            .join("README.md")
            .is_file());
        assert!(!project.join(".claude-plugin").exists());
        assert!(!project
            .join("plugins")
            .join("internal.commit-workflow")
            .exists());

        fs::remove_dir_all(project).expect("remove temp project");
    }

    #[test]
    fn claude_plugin_sync_commands_use_scope_and_marketplace_path() {
        let marketplace_root = PathBuf::from("/tmp/project/.claude/skillhub-plugin-marketplace");
        assert_eq!(
            build_claude_plugin_install_commands(
                "claude",
                "commit-workflow",
                "skillhub",
                "project",
                &marketplace_root
            ),
            Some(vec![
                vec![
                    "plugin".to_string(),
                    "marketplace".to_string(),
                    "add".to_string(),
                    "/tmp/project/.claude/skillhub-plugin-marketplace/.claude-plugin/marketplace.json".to_string(),
                    "--scope".to_string(),
                    "project".to_string(),
                ],
                vec![
                    "plugin".to_string(),
                    "install".to_string(),
                    "commit-workflow@skillhub".to_string(),
                    "--scope".to_string(),
                    "project".to_string(),
                ],
                vec![
                    "plugin".to_string(),
                    "enable".to_string(),
                    "commit-workflow@skillhub".to_string(),
                    "--scope".to_string(),
                    "project".to_string(),
                ],
            ])
        );
        assert_eq!(
            build_claude_plugin_install_commands(
                "codex",
                "commit-workflow",
                "skillhub",
                "project",
                &marketplace_root
            ),
            None
        );
    }

    #[test]
    fn claude_project_marketplace_refresh_command_removes_existing_registration() {
        assert_eq!(
            build_claude_marketplace_remove_command("claude", "skillhub", "project"),
            Some(vec![
                "plugin".to_string(),
                "marketplace".to_string(),
                "remove".to_string(),
                "skillhub".to_string(),
                "--scope".to_string(),
                "project".to_string(),
            ])
        );
        assert_eq!(
            build_claude_marketplace_remove_command("claude", "skillhub", "user"),
            None
        );
        assert_eq!(
            build_claude_marketplace_remove_command("codex", "skillhub", "project"),
            None
        );
    }

    #[test]
    fn claude_plugin_project_scope_uses_project_root_as_cli_working_dir() {
        let project = r"C:\Users\ctf19\project-a";
        assert_eq!(
            claude_plugin_command_working_dir("project", Some(project)),
            Some(PathBuf::from(project))
        );
        assert_eq!(
            claude_plugin_command_working_dir("local", Some(project)),
            Some(PathBuf::from(project))
        );
        assert_eq!(
            claude_plugin_command_working_dir("user", Some(project)),
            None
        );
        assert_eq!(claude_plugin_command_working_dir("project", None), None);
    }

    #[test]
    fn codex_project_scope_does_not_sync_global_cli_plugin_config() {
        assert!(should_sync_codex_plugin_cli("codex", "user"));
        assert!(!should_sync_codex_plugin_cli("codex", "project"));
        assert!(!should_sync_codex_plugin_cli("claude", "project"));
    }

    #[test]
    fn claude_plugin_remove_command_uses_disable_or_uninstall() {
        assert_eq!(
            build_claude_plugin_remove_command(
                "claude",
                "commit-workflow",
                "skillhub",
                "user",
                ClaudePluginRemoveAction::Disable
            ),
            Some(vec![
                "plugin".to_string(),
                "disable".to_string(),
                "commit-workflow@skillhub".to_string(),
                "--scope".to_string(),
                "user".to_string(),
            ])
        );
        assert_eq!(
            build_claude_plugin_remove_command(
                "claude",
                "commit-workflow",
                "skillhub",
                "user",
                ClaudePluginRemoveAction::Uninstall
            ),
            Some(vec![
                "plugin".to_string(),
                "uninstall".to_string(),
                "commit-workflow@skillhub".to_string(),
                "--scope".to_string(),
                "user".to_string(),
            ])
        );
    }

    #[test]
    fn claude_plugin_command_candidates_use_windows_cmd_shim() {
        let candidates = claude_plugin_command_candidates();
        if cfg!(windows) {
            assert_eq!(candidates[0].0, "cmd");
            assert_eq!(candidates[0].1, vec!["/C", "claude.cmd"]);
        }
        assert!(candidates
            .iter()
            .any(|(program, prefix)| *program == "claude" && prefix.is_empty()));
    }

    #[cfg(windows)]
    #[test]
    fn plugin_command_finds_cmd_shim_from_external_command_path() {
        let temp = std::env::temp_dir().join(format!("skillhub-cli-shim-{}", new_id()));
        fs::create_dir_all(&temp).expect("create shim dir");
        fs::write(temp.join("claude.cmd"), "@echo off\r\nexit /b 0\r\n").expect("write shim");
        let original_path = std::env::var_os("PATH");
        std::env::set_var("PATH", temp.as_os_str());

        let result = run_plugin_command(
            &[("cmd", vec!["/C", "claude.cmd"])],
            &["--version"],
            "TEST_PLUGIN_COMMAND_FAILED",
            None,
            |_| false,
        );

        match original_path {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
        let _ = fs::remove_dir_all(&temp);
        result.expect("cmd shim should run from external command PATH");
    }

    #[cfg(windows)]
    #[test]
    fn plugin_command_runs_from_requested_working_dir() {
        let temp = std::env::temp_dir().join(format!("skillhub-cli-cwd-{}", new_id()));
        fs::create_dir_all(&temp).expect("create cwd dir");
        fs::write(temp.join("marker.txt"), "ok").expect("write marker");

        let result = run_plugin_command(
            &[(
                "cmd",
                vec!["/C", "if exist marker.txt (exit /b 0) else (exit /b 1)"],
            )],
            &[],
            "TEST_PLUGIN_COMMAND_FAILED",
            Some(&temp),
            |_| false,
        );

        let _ = fs::remove_dir_all(&temp);
        result.expect("command should run from requested working directory");
    }

    #[cfg(windows)]
    #[test]
    fn plugin_command_preserves_first_executed_candidate_failure() {
        let err = run_plugin_command(
            &[
                (
                    "cmd",
                    vec!["/C", "definitely-missing-skillhub-cli-shim.cmd"],
                ),
                ("definitely-missing-skillhub-cli-shim", Vec::new()),
            ],
            &["--version"],
            "TEST_PLUGIN_COMMAND_FAILED",
            None,
            |_| false,
        )
        .expect_err("missing cmd shim should fail");

        let message = err.to_string();
        assert!(message.contains("cmd exited"));
        assert!(!message.contains("definitely-missing-skillhub-cli-shim:"));
    }

    #[test]
    fn claude_cli_missing_guidance_points_to_install_commands() {
        let message = plugin_cli_missing_guidance("claude");
        assert!(message.contains("Claude Code CLI"));
        assert!(message.contains("irm https://claude.ai/install.ps1 | iex"));
        assert!(message.contains("winget install Anthropic.ClaudeCode"));
        assert!(message.contains("npm install -g @anthropic-ai/claude-code"));
    }

    #[test]
    fn plugin_scope_conflict_blocks_personal_when_project_enabled() {
        let bindings = vec![plugin_binding_fixture(
            "project-binding",
            "project",
            Some("/tmp/project-a"),
            true,
        )];
        let err = ensure_plugin_scope_can_enable_from_bindings(
            &bindings,
            None,
            "internal",
            "commit-workflow",
            "codex",
            "user",
        )
        .expect_err("personal install should conflict with project binding");
        assert!(err.to_string().contains("PLUGIN_SCOPE_CONFLICT"));
    }

    #[test]
    fn plugin_scope_conflict_blocks_project_when_personal_enabled() {
        let bindings = vec![plugin_binding_fixture("user-binding", "user", None, true)];
        let err = ensure_plugin_scope_can_enable_from_bindings(
            &bindings,
            None,
            "internal",
            "commit-workflow",
            "codex",
            "project",
        )
        .expect_err("project install should conflict with user binding");
        assert!(err.to_string().contains("PLUGIN_SCOPE_CONFLICT"));
    }

    #[test]
    fn plugin_scope_conflict_ignores_other_targets_disabled_and_excluded_binding() {
        let mut other_target = plugin_binding_fixture("other-target", "user", None, true);
        other_target.target = "claude".to_string();
        let disabled_project =
            plugin_binding_fixture("disabled-project", "project", Some("/tmp/project-a"), false);
        let current = plugin_binding_fixture("current", "user", None, true);
        let bindings = vec![other_target, disabled_project, current];

        ensure_plugin_scope_can_enable_from_bindings(
            &bindings,
            Some("current"),
            "internal",
            "commit-workflow",
            "codex",
            "project",
        )
        .expect("excluded current binding and unrelated bindings should not conflict");
    }

    #[test]
    fn codex_plugin_marketplace_path_uses_agents_plugins_catalog() {
        let root = PathBuf::from("/tmp/project");
        assert_eq!(
            canonical_display_path(&local::plugin_marketplace_path("codex", &root)),
            "/tmp/project/.agents/plugins/marketplace.json"
        );
    }

    #[test]
    fn codex_user_marketplace_uses_personal_name() {
        assert_eq!(plugin_marketplace_name("codex", "user", None), "personal");
        assert_eq!(
            plugin_marketplace_name("codex", "project", Some("/tmp/project")),
            format!("skillhub-{}", local::path_hash("/tmp/project"))
        );
        assert_eq!(plugin_marketplace_name("claude", "user", None), "skillhub");
    }

    #[test]
    fn codex_cli_idempotent_error_detection_is_narrow() {
        assert!(is_plugin_missing_message(
            "Error: plugin `demo` is not installed"
        ));
        assert!(is_plugin_already_configured_message(
            "Error: marketplace `skillhub` is already configured"
        ));
        assert!(!is_plugin_already_configured_message(
            "Error: plugin `demo` is already installed"
        ));
    }

    #[test]
    fn claude_cli_idempotent_error_detection_handles_already_enabled() {
        assert!(is_plugin_already_enabled_message(
            "Plugin \"commit-workflow@skillhub\" is already enabled at user scope"
        ));
    }

    fn plugin_binding_fixture(
        id: &str,
        scope: &str,
        project_path: Option<&str>,
        enabled: bool,
    ) -> PluginBinding {
        PluginBinding {
            id: id.to_string(),
            package_id: "pkg".to_string(),
            source_id: Some("source".to_string()),
            namespace: "internal".to_string(),
            plugin_id: "commit-workflow".to_string(),
            plugin_name: "Commit Workflow".to_string(),
            version: "1.0.0".to_string(),
            target: "codex".to_string(),
            scope: scope.to_string(),
            project_path: project_path.map(ToString::to_string),
            marketplace_id: Some("market".to_string()),
            marketplace_name: "personal".to_string(),
            platform_ref: "commit-workflow@personal".to_string(),
            enabled,
            install_mode: "marketplace".to_string(),
            update_policy: "follow_latest".to_string(),
            status: "installed".to_string(),
            created_at: now(),
            updated_at: now(),
        }
    }

    fn sample_plugin_files(extra: Vec<(&str, Vec<u8>)>) -> Vec<(String, Vec<u8>)> {
        let mut files = vec![
            (
                "pluginhub.json".to_string(),
                br#"{
              "schema": "skillhub.plugin-source.v1",
              "namespace": "internal",
              "id": "commit-workflow",
              "name": "Commit Workflow",
              "version": "1.0.0",
              "summary": "Team commit and PR workflow plugin.",
              "categories": ["backend"],
              "tags": ["git"],
              "targets": ["codex", "claude"],
              "scopes": ["user", "project"],
              "components": ["skills", "agents"],
              "riskLevel": "medium",
              "publishScope": "public"
            }"#
                .to_vec(),
            ),
            (
                "README.md".to_string(),
                br#"---
name: Commit Workflow
description: Team commit and PR workflow plugin.
version: 1.0.0
author: skill-hub
---
# Commit Workflow
"#
                .to_vec(),
            ),
        ];
        for (path, bytes) in extra {
            if let Some((_, existing_bytes)) = files
                .iter_mut()
                .find(|(existing_path, _)| existing_path == path)
            {
                *existing_bytes = bytes;
            } else {
                files.push((path.to_string(), bytes));
            }
        }
        files
    }

    fn plugin_cache_test_state(binding_status: Option<&str>) -> (AppState, String, PathBuf) {
        let app_dir = std::env::temp_dir().join(format!("skillhub-plugin-cache-{}", new_id()));
        let package_root = app_dir.join("plugin-packages");
        let package_dir = package_root
            .join("internal")
            .join("commit-workflow")
            .join("1.0.0")
            .join("codex");
        fs::create_dir_all(&package_dir).expect("create package dir");

        let conn = rusqlite::Connection::open_in_memory().expect("open sqlite");
        conn.execute_batch(
            r#"
            CREATE TABLE plugin_packages (
              id TEXT PRIMARY KEY,
              source_id TEXT,
              namespace TEXT NOT NULL,
              plugin_id TEXT NOT NULL,
              plugin_name TEXT NOT NULL,
              version TEXT NOT NULL,
              target TEXT NOT NULL,
              package_path TEXT NOT NULL,
              sha256 TEXT,
              component_inventory_json TEXT NOT NULL,
              risk_level TEXT NOT NULL,
              cached_at TEXT NOT NULL
            );
            CREATE TABLE plugin_bindings (
              id TEXT PRIMARY KEY,
              package_id TEXT NOT NULL,
              status TEXT NOT NULL
            );
            CREATE TABLE local_package_metadata (
              package_id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              summary TEXT NOT NULL,
              tags_json TEXT NOT NULL,
              author TEXT,
              source_path TEXT NOT NULL,
              imported_at TEXT NOT NULL
            );
            CREATE TABLE audit_logs (
              id TEXT PRIMARY KEY,
              action TEXT NOT NULL,
              skill_ref TEXT,
              result TEXT NOT NULL,
              detail TEXT,
              created_at TEXT NOT NULL
            );
            "#,
        )
        .expect("create test schema");

        let package_id = new_id();
        let package_path = canonical_display_path(&package_dir);
        conn.execute(
            "INSERT INTO plugin_packages
             (id, source_id, namespace, plugin_id, plugin_name, version, target, package_path,
              sha256, component_inventory_json, risk_level, cached_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                package_id,
                Some("compiled-source".to_string()),
                "internal",
                "commit-workflow",
                "Commit Workflow",
                "1.0.0",
                "codex",
                package_path,
                None::<String>,
                "{}",
                "low",
                now()
            ],
        )
        .expect("insert package");
        conn.execute(
            "INSERT INTO local_package_metadata
             (package_id, name, summary, tags_json, author, source_path, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                package_id,
                "Commit Workflow",
                "Team commit and PR workflow plugin.",
                "[]",
                Option::<String>::None,
                package_path,
                now()
            ],
        )
        .expect("insert metadata");
        if let Some(status) = binding_status {
            conn.execute(
                "INSERT INTO plugin_bindings (id, package_id, status) VALUES (?1, ?2, ?3)",
                params![new_id(), package_id, status],
            )
            .expect("insert binding");
        }

        (
            AppState {
                conn: std::sync::Arc::new(std::sync::Mutex::new(conn)),
                app_dir,
                local_macs: vec![],
            },
            package_id,
            package_dir,
        )
    }

    #[test]
    fn normalize_categories_cleans_names_and_duplicate_order() {
        let doc = normalize_categories_doc(CategoriesDoc {
            schema: "skillhub.categories.v1".to_string(),
            generated_at: None,
            items: vec![
                Category {
                    id: " public ".to_string(),
                    name: "Public".to_string(),
                    order: 10,
                },
                Category {
                    id: "backend".to_string(),
                    name: "后端".to_string(),
                    order: 10,
                },
                Category {
                    id: "yy".to_string(),
                    name: "".to_string(),
                    order: 10,
                },
                Category {
                    id: "bad/path".to_string(),
                    name: "bad".to_string(),
                    order: 10,
                },
            ],
        });

        assert_eq!(
            doc.items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["backend", "public", "yy"]
        );
        assert_eq!(doc.items[1].name, "Public");
        assert_eq!(
            doc.items.iter().map(|item| item.order).collect::<Vec<_>>(),
            vec![10, 20, 30]
        );
    }

    #[test]
    fn validation_status_controls_draft_status() {
        let failed = serde_json::json!({ "status": "failed" });
        let passed = serde_json::json!({ "status": "passed" });

        assert_eq!(
            validation_status_from_json(&failed).as_deref(),
            Some("failed")
        );
        assert!(validation_failed(Some("failed")));
        assert!(!validation_failed(Some("passed")));
        assert_eq!(
            draft_status(
                Some("1.0.0"),
                None,
                None,
                None,
                validation_status_from_json(&failed).as_deref()
            ),
            "校验失败"
        );
        assert_eq!(
            draft_status(
                Some("1.0.0"),
                None,
                None,
                None,
                validation_status_from_json(&passed).as_deref()
            ),
            "元数据待补充"
        );
        assert_eq!(
            draft_status(Some("1.0.0"), None, Some("archived"), None, None),
            "已下架"
        );
    }

    #[test]
    fn draft_status_treats_missing_publish_target_as_pending() {
        let meta = PublishMeta {
            namespace: "community".to_string(),
            skill_id: "demo".to_string(),
            version: None,
            name: "Demo".to_string(),
            summary: "Demo skill".to_string(),
            tags: vec![],
            targets: vec![],
            levels: vec!["personal".to_string()],
            publish_scope: "project".to_string(),
            publish_category_slug: None,
            publish_project_slug: None,
            changelog: String::new(),
            updated_at: None,
            updated_by: None,
        };

        assert!(validation::is_publish_meta_ready_for_status(&meta));
        assert!(!validation::is_publish_meta_complete(&meta));
        assert_eq!(
            draft_status(Some("1.0.0"), None, None, Some(&meta), None),
            "待发布"
        );
    }

    #[test]
    fn parses_gitlab_draft_frontmatter_with_metadata_section() {
        let content = r#"---
name: MinIO Live Draft
description: MinIO 实时草稿测试
tags: minio, test
metadata:
  version: 0.1.0
  author: Skill Hub Test
---

# MinIO Live Draft

这是一个测试草稿。
"#;

        let metadata = parse_skill_frontmatter(content);
        assert_eq!(metadata.name.as_deref(), Some("MinIO Live Draft"));
        assert_eq!(metadata.description.as_deref(), Some("MinIO 实时草稿测试"));
        assert_eq!(metadata.version.as_deref(), Some("0.1.0"));
        assert_eq!(metadata.author.as_deref(), Some("Skill Hub Test"));
        assert_eq!(metadata.tags, vec!["minio", "test"]);
    }

    #[test]
    fn parses_gitlab_draft_frontmatter_with_array_tags() {
        let content = r#"---
name: Array Tags Test
description: Test array-style tags
tags:
  - frontend
  - react
  - ui
metadata:
  version: 1.0.0
  author: Test Author
---

Content here.
"#;

        let metadata = parse_skill_frontmatter(content);
        assert_eq!(metadata.name.as_deref(), Some("Array Tags Test"));
        assert_eq!(metadata.tags, vec!["frontend", "react", "ui"]);
        assert_eq!(metadata.version.as_deref(), Some("1.0.0"));
        assert_eq!(metadata.author.as_deref(), Some("Test Author"));
    }

    #[test]
    fn metadata_tags_take_priority_over_top_level_tags() {
        let content = r#"---
name: Metadata Tags Test
description: Metadata tags should win
tags: legacy, draft
metadata:
  version: 2.0.0
  author: Test Author
  tags:
    - curated
    - verified
---"#;

        let metadata = parse_skill_frontmatter(content);
        assert_eq!(metadata.tags, vec!["curated", "verified"]);
        assert_eq!(metadata.version.as_deref(), Some("2.0.0"));
        assert_eq!(metadata.author.as_deref(), Some("Test Author"));
    }

    #[test]
    fn draft_source_fingerprint_is_order_stable() {
        let first = draft_source_fingerprint(&[
            ("b.txt".to_string(), b"bbb".to_vec()),
            ("a.txt".to_string(), b"aaa".to_vec()),
        ]);
        let second = draft_source_fingerprint(&[
            ("a.txt".to_string(), b"aaa".to_vec()),
            ("b.txt".to_string(), b"bbb".to_vec()),
        ]);

        assert_eq!(first["digest"], second["digest"]);
        assert_eq!(first["files"][0]["path"], "a.txt");
        assert_eq!(first["files"][1]["path"], "b.txt");
    }

    #[test]
    #[ignore = "requires live MinIO at the compiled endpoint and an allowlisted local MAC"]
    fn live_minio_admin_publish_flow() {
        tauri::async_runtime::block_on(async {
            let admin_key = admin_config::ADMIN_KEY.to_string();
            let local_macs = admin_config::local_mac_addresses();
            unlock_admin_mode_inner(
                AdminUnlockRequest {
                    admin_key: admin_key.clone(),
                },
                &local_macs,
            )
            .await
            .expect("admin mode should unlock against live MinIO");

            let project = MarketProject {
                slug: "live-project".to_string(),
                name: "Live Project".to_string(),
                description: "Created by live MinIO integration test".to_string(),
                order: 10,
                created_at: None,
                updated_at: None,
                updated_by: None,
            };
            let client = object_store::AdminObjectClient::new();
            let mut projects = load_remote_projects(&client).await.expect("load projects");
            projects.retain(|item| item.slug != project.slug);
            projects.push(project);
            save_remote_projects(&client, &projects)
                .await
                .expect("save projects");

            let source_path = "product/minio-live-draft".to_string();
            let drafts = list_admin_drafts_inner(&admin_key, &local_macs)
                .await
                .expect("drafts should list");
            assert!(
                drafts
                    .iter()
                    .any(|draft| draft.gitlab_source_path == source_path
                        && draft.version.as_deref() == Some("0.1.0")),
                "live draft should be visible"
            );

            let meta = PublishMeta {
                namespace: "live".to_string(),
                skill_id: "minio-live-draft".to_string(),
                version: Some("0.1.0".to_string()),
                name: "MinIO Live Draft".to_string(),
                summary: "Published by live MinIO integration test.".to_string(),
                tags: vec!["minio".to_string(), "live-test".to_string()],
                targets: vec!["codex".to_string()],
                levels: vec!["personal".to_string(), "project".to_string()],
                publish_scope: "project".to_string(),
                publish_category_slug: None,
                publish_project_slug: Some("live-project".to_string()),
                changelog: "Initial live MinIO publish.".to_string(),
                updated_at: None,
                updated_by: None,
            };

            save_publish_meta_inner(
                SavePublishMetaRequest {
                    admin_key: admin_key.clone(),
                    gitlab_source_path: source_path.clone(),
                    meta,
                    artifact_kind: None,
                },
                &local_macs,
            )
            .await
            .expect("save publish metadata");

            let preview = preview_admin_draft_inner(
                AdminDraftPreviewRequest {
                    admin_key: admin_key.clone(),
                    gitlab_source_path: source_path.clone(),
                    file_path: None,
                },
                &local_macs,
            )
            .await
            .expect("preview draft");
            assert_eq!(preview.title, "MinIO Live Draft");
            assert!(preview.files.iter().any(|file| file.path == "SKILL.md"));
            assert!(preview.file_list.iter().any(|file| file.path == "SKILL.md"));

            let state_path = "draft/admin/gitlab/skills/product/minio-live-draft/state.v1.json";
            let already_published = client
                .get_optional_json::<serde_json::Value>(state_path)
                .await
                .expect("read existing state")
                .is_some_and(|state| state["publishedVersion"] == "0.1.0");

            if already_published {
                return;
            }

            publish_draft_inner(
                PublishDraftRequest {
                    admin_key,
                    gitlab_source_path: source_path,
                },
                &local_macs,
            )
            .await
            .expect("publish draft");

            let catalog = load_remote_catalog(&client).await.expect("load catalog");
            assert!(catalog
                .skills
                .iter()
                .any(|skill| skill.namespace == "live" && skill.id == "minio-live-draft"));
            let state = client
                .get_optional_json::<serde_json::Value>(state_path)
                .await
                .expect("read state")
                .expect("state exists");
            assert_eq!(state["publishedVersion"], "0.1.0");
        });
    }

    #[test]
    #[ignore = "requires live MinIO at the compiled endpoint and an allowlisted local MAC"]
    fn live_minio_archive_then_republish_flow() {
        tauri::async_runtime::block_on(async {
            let admin_key = admin_config::ADMIN_KEY.to_string();
            let local_macs = admin_config::local_mac_addresses();
            unlock_admin_mode_inner(
                AdminUnlockRequest {
                    admin_key: admin_key.clone(),
                },
                &local_macs,
            )
            .await
            .expect("admin mode should unlock against live MinIO");

            let client = object_store::AdminObjectClient::new();
            let project = MarketProject {
                slug: "live-project".to_string(),
                name: "Live Project".to_string(),
                description: "Created by live MinIO integration test".to_string(),
                order: 10,
                created_at: None,
                updated_at: None,
                updated_by: None,
            };
            let mut projects = load_remote_projects(&client).await.expect("load projects");
            projects.retain(|item| item.slug != project.slug);
            projects.push(project);
            save_remote_projects(&client, &projects)
                .await
                .expect("save projects");

            let source_path = "product/minio-live-draft".to_string();
            let meta = PublishMeta {
                namespace: "live".to_string(),
                skill_id: "minio-live-draft".to_string(),
                version: Some("0.1.0".to_string()),
                name: "MinIO Live Draft".to_string(),
                summary: "Published by live MinIO integration test.".to_string(),
                tags: vec!["minio".to_string(), "live-test".to_string()],
                targets: vec!["codex".to_string()],
                levels: vec!["personal".to_string(), "project".to_string()],
                publish_scope: "project".to_string(),
                publish_category_slug: None,
                publish_project_slug: Some("live-project".to_string()),
                changelog: "Live archive and republish test.".to_string(),
                updated_at: None,
                updated_by: None,
            };
            save_publish_meta_inner(
                SavePublishMetaRequest {
                    admin_key: admin_key.clone(),
                    gitlab_source_path: source_path.clone(),
                    meta,
                    artifact_kind: None,
                },
                &local_macs,
            )
            .await
            .expect("save publish metadata");

            let catalog = load_remote_catalog(&client).await.expect("load catalog");
            let currently_listed = catalog
                .skills
                .iter()
                .any(|skill| skill.namespace == "live" && skill.id == "minio-live-draft");
            if !currently_listed {
                publish_draft_inner(
                    PublishDraftRequest {
                        admin_key: admin_key.clone(),
                        gitlab_source_path: source_path.clone(),
                    },
                    &local_macs,
                )
                .await
                .expect("publish existing draft before archive");
            }

            archive_market_skill_inner(
                ArchiveMarketSkillRequest {
                    admin_key: admin_key.clone(),
                    namespace: "live".to_string(),
                    skill_id: "minio-live-draft".to_string(),
                    reason: Some("live integration archive test".to_string()),
                },
                &local_macs,
            )
            .await
            .expect("archive market skill");

            let archived_catalog = load_remote_catalog(&client)
                .await
                .expect("load catalog after archive");
            assert!(
                !archived_catalog
                    .skills
                    .iter()
                    .any(|skill| skill.namespace == "live" && skill.id == "minio-live-draft"),
                "skill should disappear from market catalog after archive"
            );

            let drafts = list_admin_drafts_inner(&admin_key, &local_macs)
                .await
                .expect("list drafts");
            assert!(
                drafts.iter().any(|draft| {
                    draft.gitlab_source_path == source_path
                        && draft.status == "已下架"
                        && draft.source_available
                }),
                "archived GitLab draft should be visible in draft list"
            );

            publish_draft_inner(
                PublishDraftRequest {
                    admin_key,
                    gitlab_source_path: source_path,
                },
                &local_macs,
            )
            .await
            .expect("republish archived existing version");

            let republished_catalog = load_remote_catalog(&client)
                .await
                .expect("load catalog after republish");
            assert!(
                republished_catalog
                    .skills
                    .iter()
                    .any(|skill| skill.namespace == "live" && skill.id == "minio-live-draft"),
                "skill should return to market catalog after republish"
            );
        });
    }
}

fn map_result<T>(result: Result<T>) -> CommandResult<T> {
    result.map_err(|error| CommandError::new("COMMAND_FAILED", error.to_string()))
}
