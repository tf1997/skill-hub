use std::{
    collections::{BTreeMap, HashSet},
    fs,
};

use anyhow::{anyhow, Context, Result};

use crate::{
    admin_config,
    db::{
        market_project_cache_path, new_id, now, AppState, COMPILED_SOURCE_BUCKET,
        COMPILED_SOURCE_ENDPOINT, COMPILED_SOURCE_REGION,
    },
    models::{
        AdminAuditLog, AdminDraftPlugin, AdminDraftPreviewRequest, AdminDraftSkill, AdminSession,
        AdminUnlockRequest, ArchiveMarketPluginRequest, ArchiveMarketSkillRequest, CatalogDoc,
        CategoriesDoc, Category, DeleteMarketCategoryRequest, DeleteMarketProjectRequest,
        ListAdminAuditLogsRequest, MarketPlugin, MarketProject, MarketSkill, PackageInfo,
        PluginCatalogDoc, PluginManifest, PluginPackageRef, PluginSourceMeta, PluginVersion,
        PluginVersionPackages, ProjectsDoc, PublishDraftRequest, PublishMeta,
        PublishPluginDraftRequest, QuickRepublishRequest, SaveMarketCategoryRequest,
        SaveMarketProjectRequest, SavePublishMetaRequest, SkillManifest, SkillPreview,
        SkillPreviewFile, SkillPreviewFileEntry, SkillVersion,
    },
    services::{local, object_store, package, preview, validation},
};

pub(crate) const DRAFT_GITLAB_PREFIX: &str = "draft/gitlab/skills/";
pub(crate) const PLUGIN_DRAFT_PREFIX: &str = "draft/gitlab/plugins/";
pub(crate) const DRAFT_ADMIN_PREFIX: &str = "draft/admin/gitlab/skills/";
pub(crate) const PLUGIN_ADMIN_PREFIX: &str = "draft/admin/gitlab/plugins/";
pub(crate) const ARCHIVED_ADMIN_PREFIX: &str = "draft/admin/archived/skills/";
pub(crate) const PLUGIN_ARCHIVED_ADMIN_PREFIX: &str = "draft/admin/archived/plugins/";
pub(crate) const PROJECTS_OBJECT: &str = "projects.v1.json";
pub(crate) const CATALOG_OBJECT: &str = "catalog.v1.json";
pub(crate) const PLUGIN_CATALOG_OBJECT: &str = "plugin-catalog.v1.json";
pub(crate) const CATEGORIES_OBJECT: &str = "categories.v1.json";
pub(crate) const FIXED_PUBLISH_NAMESPACE: &str = "DT";
pub(crate) async fn unlock_admin_mode_inner(
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

pub(crate) async fn ensure_admin_allowed(
    admin_key: &str,
    local_macs: &[String],
) -> Result<admin_config::AdminAuthorization> {
    if !admin_config::is_admin_key_valid(admin_key) {
        return Err(anyhow!("管理员密钥错误"));
    }

    let allowlist = object_store::fetch_admin_mac_allowlist().await?;
    authorize_admin_from_allowlist(admin_key, local_macs, &allowlist)
}

pub(crate) fn authorize_admin_from_allowlist(
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

pub(crate) async fn list_admin_drafts_inner(
    admin_key: &str,
    local_macs: &[String],
) -> Result<Vec<AdminDraftSkill>> {
    ensure_admin_allowed(admin_key, local_macs).await?;
    let client = object_store::AdminObjectClient::new();
    let objects = client.list_objects(DRAFT_GITLAB_PREFIX).await?;
    let mut drafts = Vec::new();
    let mut seen_sources = HashSet::new();
    let mut catalog_for_recovery: Option<CatalogDoc> = None;

    for source_path in collect_skill_draft_source_paths(&objects) {
        seen_sources.insert(source_path.clone());

        let skill_md_path = format!("{DRAFT_GITLAB_PREFIX}{source_path}/SKILL.md");
        let skill_md = client.get_text(&skill_md_path).await.unwrap_or_default();
        let draft_metadata = parse_skill_frontmatter(&skill_md);
        let version = draft_metadata.version.clone();
        let author = draft_metadata.author.clone();
        let meta_path = admin_object_path(&source_path, "publish-meta.v1.json")?;
        let state_path = admin_object_path(&source_path, "state.v1.json")?;
        let validation_path = format!("{}{}/validation.json", DRAFT_GITLAB_PREFIX, source_path);
        let default_meta = default_publish_meta_from_draft(&source_path, &draft_metadata);
        let saved_publish_meta = client.get_optional_json::<PublishMeta>(&meta_path).await?;
        let mut state_json = client
            .get_optional_json::<serde_json::Value>(&state_path)
            .await?;
        let recovered = if saved_publish_meta.is_none() || state_json.is_none() {
            if catalog_for_recovery.is_none() {
                catalog_for_recovery = Some(load_remote_catalog(&client).await?);
            }
            recover_skill_admin_artifacts_from_market(
                &source_path,
                &draft_metadata,
                catalog_for_recovery.as_ref().expect("catalog loaded"),
            )
        } else {
            None
        };
        let publish_meta = match saved_publish_meta {
            Some(meta) => Some(merge_publish_meta_defaults(
                normalize_publish_meta_for_source(meta, &source_path),
                default_meta.clone(),
            )),
            None => {
                if let Some(recovered) = recovered.as_ref() {
                    client.put_json(&meta_path, &recovered.publish_meta).await?;
                    Some(recovered.publish_meta.clone())
                } else {
                    Some(default_meta)
                }
            }
        };
        if state_json.is_none() {
            if let Some(recovered) = recovered.as_ref() {
                client.put_json(&state_path, &recovered.state).await?;
                state_json = Some(recovered.state.clone());
            }
        }
        let published_version = state_json
            .as_ref()
            .and_then(skill_published_version_from_state);
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

pub(crate) async fn list_admin_plugin_drafts_inner(
    admin_key: &str,
    local_macs: &[String],
) -> Result<Vec<AdminDraftPlugin>> {
    ensure_admin_allowed(admin_key, local_macs).await?;
    let client = object_store::AdminObjectClient::new();
    let objects = client.list_objects(PLUGIN_DRAFT_PREFIX).await?;
    let mut drafts = Vec::new();
    let mut seen_sources = HashSet::new();

    for source_path in collect_plugin_draft_source_paths(&objects) {
        if source_path.trim().is_empty() {
            continue;
        }

        let state_path = plugin_admin_object_path(&source_path, "state.v1.json")?;
        let state_json = client
            .get_optional_json::<serde_json::Value>(&state_path)
            .await?;
        let published_version = state_json.as_ref().and_then(published_version_from_state);
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
                    plugin_builtin_targets(),
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

        seen_sources.insert(source_path.clone());
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

    for prefix in [PLUGIN_ADMIN_PREFIX, PLUGIN_ARCHIVED_ADMIN_PREFIX] {
        let admin_objects = client.list_objects(prefix).await?;
        for object in admin_objects
            .iter()
            .filter(|object| object.ends_with("/state.v1.json") && object.starts_with(prefix))
        {
            let state_json = client
                .get_optional_json::<serde_json::Value>(object)
                .await?
                .unwrap_or_default();
            let Some(draft) = plugin_admin_state_draft_from_json(object, &state_json) else {
                continue;
            };
            if seen_sources.insert(draft.gitlab_source_path.clone()) {
                drafts.push(draft);
            }
        }
    }
    drafts.sort_by(|a, b| a.gitlab_source_path.cmp(&b.gitlab_source_path));
    Ok(drafts)
}

pub(crate) fn plugin_admin_state_draft_from_json(
    object_path: &str,
    state_json: &serde_json::Value,
) -> Option<AdminDraftPlugin> {
    let object_source_path = object_path
        .strip_prefix(PLUGIN_ADMIN_PREFIX)
        .or_else(|| object_path.strip_prefix(PLUGIN_ARCHIVED_ADMIN_PREFIX))?
        .strip_suffix("/state.v1.json")?
        .to_string();
    let source_path = state_json
        .get("gitlabSourcePath")
        .or_else(|| state_json.get("gitlab_source_path"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
        .unwrap_or(object_source_path);
    if !is_valid_draft_source_path(&source_path) {
        return None;
    }

    let namespace = state_json
        .get("namespace")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let plugin_id = state_json
        .get("pluginId")
        .or_else(|| state_json.get("plugin_id"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let name = state_json
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| plugin_id.clone());
    let summary = state_json
        .get("summary")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| "Plugin source is no longer available from GitLab.".to_string());
    let published_version = published_version_from_state(state_json);
    let state_status = state_json
        .get("status")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    let categories = state_json
        .get("categories")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let publish_scope = state_json
        .get("publishScope")
        .or_else(|| state_json.get("publish_scope"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            categories.iter().find_map(|category| {
                category
                    .strip_prefix("project:")
                    .map(|_| "project".to_string())
            })
        })
        .unwrap_or_else(|| "public".to_string());
    let publish_project_slug = state_json
        .get("publishProjectSlug")
        .or_else(|| state_json.get("publish_project_slug"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            categories
                .iter()
                .find_map(|category| category.strip_prefix("project:").map(ToString::to_string))
        });
    let publish_category_slug = state_json
        .get("publishCategorySlug")
        .or_else(|| state_json.get("publish_category_slug"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            categories
                .iter()
                .find(|category| !category.starts_with("project:"))
                .cloned()
        });
    let updated_at = state_json
        .get("updatedAt")
        .or_else(|| state_json.get("updated_at"))
        .or_else(|| state_json.get("archivedAt"))
        .or_else(|| state_json.get("archived_at"))
        .or_else(|| state_json.get("publishedAt"))
        .or_else(|| state_json.get("published_at"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    let updated_by = state_json
        .get("archivedBy")
        .or_else(|| state_json.get("archived_by"))
        .or_else(|| state_json.get("publishedBy"))
        .or_else(|| state_json.get("published_by"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string);

    let mut draft_location = parse_gitlab_source_path(&source_path);
    if draft_location.draft_slug.is_none() {
        draft_location.draft_slug = plugin_id.clone();
    }
    if draft_location.category_path.is_empty() {
        if let Some(namespace) = namespace.as_ref() {
            draft_location.category_path.push(namespace.clone());
        }
    }

    let targets = plugin_builtin_targets();
    let scopes = vec!["user".to_string(), "project".to_string()];
    let publish_meta = match (namespace.clone(), plugin_id.clone(), name.clone()) {
        (Some(namespace), Some(plugin_id), Some(name)) => Some(PublishMeta {
            namespace,
            skill_id: plugin_id,
            version: published_version.clone(),
            name,
            summary: summary.clone(),
            tags: Vec::new(),
            targets: targets.clone(),
            levels: scopes.clone(),
            publish_scope: publish_scope.clone(),
            publish_category_slug: if publish_scope == "project" {
                None
            } else {
                publish_category_slug.clone()
            },
            publish_project_slug: if publish_scope == "project" {
                publish_project_slug.clone()
            } else {
                None
            },
            changelog: String::new(),
            updated_at: updated_at.clone(),
            updated_by,
        }),
        _ => None,
    };
    let status = plugin_draft_status(
        false,
        false,
        published_version.as_deref(),
        published_version.as_deref(),
        state_status.as_deref(),
        &targets,
        publish_meta.as_ref(),
        None,
    );

    Some(AdminDraftPlugin {
        gitlab_source_path: source_path,
        draft_slug: draft_location.draft_slug,
        gitlab_category_path: draft_location.category_path,
        source_available: false,
        readme_metadata_complete: false,
        namespace,
        plugin_id,
        name,
        summary: Some(summary),
        version: published_version.clone(),
        targets,
        scopes,
        components: Vec::new(),
        risk_level: None,
        status,
        validation_status: None,
        publish_meta,
        published_version,
        updated_at,
    })
}
pub(crate) async fn list_admin_audit_logs_inner(
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

pub(crate) async fn preview_admin_draft_inner(
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

pub(crate) async fn preview_admin_plugin_draft_inner(
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

pub(crate) async fn save_publish_meta_inner(
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

pub(crate) async fn save_plugin_publish_meta_inner(
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

pub(crate) async fn save_market_project_remote_inner(
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

pub(crate) async fn delete_market_project_remote_inner(
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

pub(crate) async fn save_market_category_remote_inner(
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

pub(crate) async fn delete_market_category_remote_inner(
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

pub(crate) async fn publish_draft_inner(
    request: PublishDraftRequest,
    local_macs: &[String],
) -> Result<()> {
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
        .and_then(skill_published_version_from_state);
    let state_archived = state_json
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("archived"));
    if !state_archived && published_version.as_deref() == Some(version.as_str()) {
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

pub(crate) async fn publish_plugin_draft_inner(
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

    let published_version = state_json.as_ref().and_then(published_version_from_state);
    if !state_archived && published_version.as_deref() == Some(meta.version.as_str()) {
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

pub(crate) async fn quick_republish_archived_skill_inner(
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
            .and_then(skill_published_version_from_state)
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

pub(crate) async fn archive_market_skill_inner(
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

pub(crate) async fn archive_market_plugin_inner(
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

pub(crate) fn normalize_categories_doc(mut doc: CategoriesDoc) -> CategoriesDoc {
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

pub(crate) fn category_name_from_slug(slug: &str) -> String {
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

pub(crate) fn default_publish_meta_from_draft(
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

pub(crate) fn merge_publish_meta_defaults(
    mut meta: PublishMeta,
    defaults: PublishMeta,
) -> PublishMeta {
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

pub(crate) fn normalize_publish_meta_for_source(
    meta: PublishMeta,
    source_path: &str,
) -> PublishMeta {
    let mut meta = normalize_publish_meta(meta);
    meta.namespace = FIXED_PUBLISH_NAMESPACE.to_string();
    meta.skill_id = draft_skill_id_from_source_path(source_path);
    meta
}

pub(crate) fn plugin_builtin_targets() -> Vec<String> {
    vec!["codex".to_string(), "claude".to_string()]
}
pub(crate) fn default_plugin_publish_meta(meta: &PluginSourceMeta) -> PublishMeta {
    PublishMeta {
        namespace: meta.namespace.clone(),
        skill_id: meta.id.clone(),
        version: Some(meta.version.clone()).filter(|value| !value.trim().is_empty()),
        name: meta.name.clone(),
        summary: meta.summary.clone(),
        tags: meta.tags.clone(),
        targets: plugin_builtin_targets(),
        levels: meta.scopes.clone(),
        publish_scope: "public".to_string(),
        publish_category_slug: None,
        publish_project_slug: None,
        changelog: String::new(),
        updated_at: None,
        updated_by: None,
    }
}

pub(crate) fn default_plugin_publish_meta_from_readme(
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
        targets: plugin_builtin_targets(),
        levels: vec!["user".to_string(), "project".to_string()],
        publish_scope: "public".to_string(),
        publish_category_slug: None,
        publish_project_slug: None,
        changelog: String::new(),
        updated_at: None,
        updated_by: None,
    }
}

pub(crate) fn merge_plugin_readme_defaults(
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

pub(crate) fn normalize_plugin_publish_meta(
    meta: PublishMeta,
    defaults: Option<&PublishMeta>,
) -> PublishMeta {
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
    meta.targets = plugin_builtin_targets();
    meta
}

pub(crate) fn apply_plugin_publish_meta(
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

pub(crate) fn apply_plugin_readme_metadata(
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

pub(crate) fn plugin_source_meta_from_readme(
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
        targets: plugin_builtin_targets(),
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

pub(crate) fn required_plugin_readme_field(value: Option<&str>, field: &str) -> Result<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("PLUGIN_SOURCE_INVALID: README.md 缺少 {field}"))
}

pub(crate) fn is_plugin_readme_metadata_complete(metadata: &DraftSkillMetadata) -> bool {
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

pub(crate) fn plugin_id_from_readme_name(name: &str) -> String {
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

pub(crate) fn draft_skill_id_from_source_path(source_path: &str) -> String {
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

pub(crate) fn parse_frontmatter_tags(value: &str) -> Vec<String> {
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

pub(crate) fn push_unique_tag(tags: &mut Vec<String>, tag: String) {
    if !tags.iter().any(|item| item.eq_ignore_ascii_case(&tag)) {
        tags.push(tag);
    }
}

pub(crate) fn parse_skill_markdown_field(content: &str, field: &str) -> Option<String> {
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
pub(crate) struct GitlabDraftLocation {
    pub(crate) category_path: Vec<String>,
    pub(crate) draft_slug: Option<String>,
}

impl GitlabDraftLocation {
    pub(crate) fn category_code(&self) -> Option<String> {
        if self.category_path.is_empty() {
            None
        } else {
            Some(self.category_path.join("/"))
        }
    }
}

pub(crate) fn parse_gitlab_source_path(source_path: &str) -> GitlabDraftLocation {
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

pub(crate) fn non_empty_string(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RecoveredSkillAdminArtifacts {
    pub(crate) publish_meta: PublishMeta,
    pub(crate) state: serde_json::Value,
}

pub(crate) fn recover_skill_admin_artifacts_from_market(
    source_path: &str,
    metadata: &DraftSkillMetadata,
    catalog: &CatalogDoc,
) -> Option<RecoveredSkillAdminArtifacts> {
    let version = metadata.version.as_deref()?.trim();
    if version.is_empty() {
        return None;
    }
    let skill_id = draft_skill_id_from_source_path(source_path);
    let market_skill = catalog.skills.iter().find(|skill| {
        skill.namespace == FIXED_PUBLISH_NAMESPACE
            && skill.id == skill_id
            && skill.latest_version == version
    })?;

    let mut publish_meta = PublishMeta {
        namespace: market_skill.namespace.clone(),
        skill_id: market_skill.id.clone(),
        version: Some(market_skill.latest_version.clone()),
        name: market_skill.name.clone(),
        summary: market_skill.summary.clone(),
        tags: market_skill.tags.clone(),
        targets: market_skill.targets.clone(),
        levels: if market_skill.levels.is_empty() {
            vec!["personal".to_string(), "project".to_string()]
        } else {
            market_skill.levels.clone()
        },
        publish_scope: "public".to_string(),
        publish_category_slug: None,
        publish_project_slug: None,
        changelog: String::new(),
        updated_at: market_skill.updated_at.clone(),
        updated_by: None,
    };
    apply_market_categories_to_publish_meta(&mut publish_meta, &market_skill.categories);

    let state = serde_json::json!({
        "gitlabSourcePath": source_path,
        "namespace": market_skill.namespace,
        "skillId": market_skill.id,
        "name": market_skill.name,
        "summary": market_skill.summary,
        "categories": market_skill.categories,
        "publishedVersion": market_skill.latest_version,
        "publishScope": publish_meta.publish_scope,
        "publishCategorySlug": publish_meta.publish_category_slug,
        "publishProjectSlug": publish_meta.publish_project_slug,
        "status": "published",
        "updatedAt": market_skill.updated_at
    });

    Some(RecoveredSkillAdminArtifacts {
        publish_meta,
        state,
    })
}

fn apply_market_categories_to_publish_meta(meta: &mut PublishMeta, categories: &[String]) {
    if let Some(project_slug) = categories
        .iter()
        .filter_map(|category| category.strip_prefix("project:"))
        .map(str::trim)
        .find(|slug| !slug.is_empty())
    {
        meta.publish_scope = "project".to_string();
        meta.publish_project_slug = Some(project_slug.to_string());
        meta.publish_category_slug = None;
        return;
    }

    meta.publish_scope = "public".to_string();
    meta.publish_category_slug = categories
        .iter()
        .map(|category| category.trim())
        .find(|category| !category.is_empty())
        .map(ToString::to_string);
    meta.publish_project_slug = None;
}
pub(crate) fn skill_published_version_from_state(value: &serde_json::Value) -> Option<String> {
    if let Some(version) = published_version_from_state(value) {
        return Some(version);
    }

    let state_status = value
        .get("status")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .unwrap_or_default();
    if !state_status.eq_ignore_ascii_case("published")
        && !state_status.eq_ignore_ascii_case("archived")
    {
        return None;
    }

    for key in ["version", "latestVersion", "latest_version"] {
        if let Some(version) = state_version_field(value, key) {
            return Some(version);
        }
    }

    None
}

pub(crate) fn published_version_from_state(value: &serde_json::Value) -> Option<String> {
    for key in ["publishedVersion", "published_version"] {
        if let Some(version) = state_version_field(value, key) {
            return Some(version);
        }
    }

    None
}

fn state_version_field(value: &serde_json::Value, key: &str) -> Option<String> {
    match value.get(key)? {
        serde_json::Value::String(value) => non_empty_string(value.clone()),
        serde_json::Value::Number(value) => non_empty_string(value.to_string()),
        _ => None,
    }
}
pub(crate) fn plugin_draft_status(
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

pub(crate) fn draft_status(
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

pub(crate) fn validation_status_from_json(value: &serde_json::Value) -> Option<String> {
    value
        .get("status")
        .and_then(|status| status.as_str())
        .map(|status| status.trim().to_ascii_lowercase())
}

pub(crate) fn validation_failed(status: Option<&str>) -> bool {
    status
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "" | "passed" | "ok" | "success"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn normalize_publish_meta(mut meta: PublishMeta) -> PublishMeta {
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

pub(crate) async fn validate_publish_target(
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

pub(crate) fn ensure_system_admin(authorization: &admin_config::AdminAuthorization) -> Result<()> {
    if authorization.is_system() {
        Ok(())
    } else {
        Err(anyhow!("该操作需要系统管理员权限"))
    }
}

pub(crate) fn ensure_can_view_admin_audit(
    authorization: &admin_config::AdminAuthorization,
) -> Result<()> {
    if authorization.is_system() {
        Ok(())
    } else {
        Err(anyhow!("审计日志仅系统管理员可查看"))
    }
}

pub(crate) fn ensure_can_manage_project(
    authorization: &admin_config::AdminAuthorization,
    project_slug: &str,
) -> Result<()> {
    if authorization.can_manage_project(project_slug) {
        Ok(())
    } else {
        Err(anyhow!("未授权管理项目: {project_slug}"))
    }
}

pub(crate) fn ensure_can_manage_publish_target(
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

pub(crate) fn ensure_can_manage_skill_categories(
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

pub(crate) fn admin_actor(authorization: &admin_config::AdminAuthorization) -> String {
    authorization
        .name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| authorization.mac_address.clone())
}

pub(crate) fn admin_audit_envelope(
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

pub(crate) fn admin_audit_log_from_value(
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

pub(crate) fn audit_field_string(
    value: &serde_json::Value,
    payload: &serde_json::Value,
    keys: &[&str],
) -> Option<String> {
    keys.iter()
        .find_map(|key| audit_value_string(value, key).or_else(|| audit_value_string(payload, key)))
}

pub(crate) fn audit_value_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
}

pub(crate) fn admin_audit_target(
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

pub(crate) fn namespace_artifact_target(value: &serde_json::Value) -> Option<String> {
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

pub(crate) fn admin_audit_summary(action: &str, target: Option<&str>) -> String {
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

pub(crate) fn audit_action_from_path(object_path: &str) -> String {
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

pub(crate) fn audit_date_from_path(object_path: &str) -> Option<String> {
    let mut parts = object_path.split('/');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("admin"), Some("audit"), Some(year), Some(month)) => {
            let day = parts.next()?;
            Some(format!("{year}-{month}-{day}T00:00:00Z"))
        }
        _ => None,
    }
}

pub(crate) fn should_republish_existing_version(
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

pub(crate) fn admin_object_path(source_path: &str, leaf: &str) -> Result<String> {
    Ok(format!(
        "{}{}/{}",
        DRAFT_ADMIN_PREFIX,
        validation::normalize_relative_object_path(source_path)?,
        leaf
    ))
}

pub(crate) fn plugin_admin_object_path(source_path: &str, leaf: &str) -> Result<String> {
    Ok(format!(
        "{}{}/{}",
        PLUGIN_ADMIN_PREFIX,
        validation::normalize_relative_object_path(source_path)?,
        leaf
    ))
}

pub(crate) fn publish_categories(meta: &PublishMeta) -> Vec<String> {
    if meta.publish_scope == "project" {
        vec![format!(
            "project:{}",
            meta.publish_project_slug.as_deref().unwrap_or("")
        )]
    } else {
        vec![meta.publish_category_slug.clone().unwrap_or_default()]
    }
}

pub(crate) fn rebuild_catalog_categories(skills: &[MarketSkill]) -> Vec<String> {
    let mut categories = skills
        .iter()
        .flat_map(|skill| skill.categories.iter().cloned())
        .collect::<Vec<_>>();
    categories.sort();
    categories.dedup();
    categories
}

pub(crate) fn merge_categories(mut first: Vec<String>, second: Vec<String>) -> Vec<String> {
    first.extend(second);
    first.sort();
    first.dedup();
    first
}

pub(crate) fn plugin_publish_categories(meta: &PluginSourceMeta) -> Vec<String> {
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

pub(crate) fn plugin_publish_category_slug(meta: &PluginSourceMeta) -> Option<String> {
    if meta.publish_scope.as_deref() == Some("project") {
        None
    } else {
        meta.categories.first().cloned()
    }
}

pub(crate) fn build_plugin_search_lite_index(catalog: &PluginCatalogDoc) -> serde_json::Value {
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

pub(crate) fn should_publish_plugin_version(
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

pub(crate) async fn validate_plugin_publish_target(
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

pub(crate) async fn load_remote_catalog(
    client: &object_store::AdminObjectClient,
) -> Result<CatalogDoc> {
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

pub(crate) async fn load_remote_plugin_catalog(
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

pub(crate) async fn load_remote_categories(
    client: &object_store::AdminObjectClient,
) -> Result<CategoriesDoc> {
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

pub(crate) fn ensure_publish_category(mut doc: CategoriesDoc, meta: &PublishMeta) -> CategoriesDoc {
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

pub(crate) async fn load_remote_projects(
    client: &object_store::AdminObjectClient,
) -> Result<Vec<MarketProject>> {
    Ok(client
        .get_optional_json::<ProjectsDoc>(PROJECTS_OBJECT)
        .await?
        .map(ProjectsDoc::into_projects)
        .map(normalize_market_projects)
        .unwrap_or_default())
}

pub(crate) async fn save_remote_projects(
    client: &object_store::AdminObjectClient,
    projects: &[MarketProject],
) -> Result<()> {
    client
        .put_json(PROJECTS_OBJECT, &projects_doc(projects.to_vec()))
        .await
}

pub(crate) fn projects_doc(projects: Vec<MarketProject>) -> ProjectsDoc {
    ProjectsDoc {
        schema: "skillhub.projects.v1".to_string(),
        generated_at: Some(now()),
        projects: normalize_market_projects(projects),
        items: Vec::new(),
    }
}

pub(crate) fn normalize_market_projects(projects: Vec<MarketProject>) -> Vec<MarketProject> {
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

pub(crate) async fn write_all_market_indexes(
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

pub(crate) async fn write_plugin_market_indexes_for_categories(
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

pub(crate) async fn write_market_indexes_for_categories(
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

pub(crate) async fn write_admin_audit(
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

pub(crate) async fn find_draft_source_for_skill(
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

pub(crate) async fn find_draft_source_for_plugin(
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

pub(crate) fn collect_skill_draft_source_paths(objects: &[String]) -> Vec<String> {
    let mut paths = objects
        .iter()
        .filter_map(|object| {
            if !object.starts_with(DRAFT_GITLAB_PREFIX) || !object.ends_with("/SKILL.md") {
                return None;
            }
            let source_path = object
                .trim_start_matches(DRAFT_GITLAB_PREFIX)
                .trim_end_matches("/SKILL.md");
            if !is_valid_draft_source_path(source_path) {
                return None;
            }
            Some(source_path.to_string())
        })
        .collect::<Vec<_>>();
    prune_nested_draft_source_paths(&mut paths);
    paths
}

fn is_valid_draft_source_path(source_path: &str) -> bool {
    !source_path.trim().is_empty() && !source_path.contains("..") && !source_path.contains('\\')
}

fn prune_nested_draft_source_paths(paths: &mut Vec<String>) {
    paths.sort();
    paths.dedup();

    let mut roots: Vec<String> = Vec::new();
    for path in paths.iter() {
        if !roots
            .iter()
            .any(|root| is_nested_draft_source_path(path, root))
        {
            roots.push(path.clone());
        }
    }
    *paths = roots;
}

fn is_nested_draft_source_path(path: &str, root: &str) -> bool {
    path.len() > root.len()
        && path.starts_with(root)
        && path.as_bytes().get(root.len()) == Some(&b'/')
}
pub(crate) fn collect_plugin_draft_source_paths(objects: &[String]) -> Vec<String> {
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
    prune_nested_draft_source_paths(&mut paths);
    paths
}

pub(crate) fn infer_plugin_draft_source_path_from_relative(relative: &str) -> Option<String> {
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

pub(crate) fn is_plugin_draft_root_marker(segment: &str) -> bool {
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
pub(crate) struct PluginDraftContentRoot {
    pub(crate) prefix: String,
    pub(crate) pluginhub_path: String,
}

pub(crate) fn resolve_plugin_draft_content_prefix(
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

pub(crate) fn plugin_draft_root_has_generated_artifacts(
    draft_root: &str,
    objects: &[String],
) -> bool {
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

pub(crate) fn collect_plugin_draft_preview_file_list(
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

pub(crate) fn is_plugin_draft_preview_file(relative: &str) -> bool {
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

pub(crate) fn is_plugin_source_file(relative: &str) -> bool {
    is_plugin_draft_preview_file(relative)
        && relative.replace('\\', "/").to_ascii_lowercase() != "validation.json"
}

pub(crate) fn has_plugin_source_files(draft_prefix: &str, objects: &[String]) -> bool {
    objects.iter().any(|object| {
        object.starts_with(draft_prefix)
            && !object.ends_with('/')
            && is_plugin_source_file(object.trim_start_matches(draft_prefix))
    })
}

pub(crate) fn infer_plugin_components_from_object_paths(
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

pub(crate) fn draft_source_fingerprint(files: &[(String, Vec<u8>)]) -> serde_json::Value {
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

pub(crate) fn collect_draft_preview_file_list(
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
pub(crate) struct PreparedPluginPublish {
    pub(crate) meta: PluginSourceMeta,
    pub(crate) packages: BTreeMap<String, PreparedPluginPackage>,
    pub(crate) component_inventory: serde_json::Value,
    pub(crate) risk_report: serde_json::Value,
    pub(crate) risk_level: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedPluginPackage {
    pub(crate) bytes: Vec<u8>,
    pub(crate) sha256: String,
    pub(crate) size: i64,
}

pub(crate) fn prepare_plugin_publish(
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

pub(crate) fn read_plugin_readme_metadata(
    files: &[(String, Vec<u8>)],
) -> Result<DraftSkillMetadata> {
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

pub(crate) fn read_optional_plugin_source_meta(
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

pub(crate) fn validate_plugin_source_meta(meta: &PluginSourceMeta) -> Result<()> {
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

pub(crate) fn validate_plugin_source_files(files: &[(String, Vec<u8>)]) -> Result<()> {
    for (path, _) in files {
        if package::normalize_zip_relative_path(path).is_none() {
            return Err(anyhow!(
                "PLUGIN_PACKAGE_BUILD_FAILED: 包含不安全路径 {path}"
            ));
        }
    }
    Ok(())
}

pub(crate) fn is_plugin_platform_generated_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    matches!(
        normalized.as_str(),
        ".codex-plugin/plugin.json" | ".claude-plugin/plugin.json"
    ) || normalized.starts_with(".codex-plugin/")
        || normalized.starts_with(".claude-plugin/")
        || normalized.starts_with("codex/")
        || normalized.starts_with("claude/")
}
pub(crate) fn build_plugin_component_inventory(
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

pub(crate) fn codex_component_inventory(files: &[(String, Vec<u8>)]) -> serde_json::Value {
    let paths = package::common_plugin_paths(files);
    serde_json::json!({
        "skills": package::direct_child_dirs(&paths, "skills/"),
        "hooks": direct_child_files(&paths, "hooks/"),
        "mcpServers": file_present(&paths, ".mcp.json"),
        "apps": file_present(&paths, ".app.json"),
        "assets": files_under(&paths, "assets/")
    })
}

pub(crate) fn claude_component_inventory(files: &[(String, Vec<u8>)]) -> serde_json::Value {
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

pub(crate) fn infer_plugin_components(files: &[(String, Vec<u8>)]) -> Vec<String> {
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

pub(crate) fn direct_child_files(paths: &[String], prefix: &str) -> Vec<String> {
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

pub(crate) fn files_under(paths: &[String], prefix: &str) -> Vec<String> {
    direct_child_files(paths, prefix)
}

pub(crate) fn file_present(paths: &[String], file: &str) -> Vec<String> {
    if paths.iter().any(|path| path == file) {
        vec![file.to_string()]
    } else {
        Vec::new()
    }
}

pub(crate) fn build_plugin_risk_report(
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

    let risk_level = declared_level
        .filter(|level| !level.trim().is_empty())
        .unwrap_or("low");
    let requires_user_review = false;
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

pub(crate) fn build_plugin_json(prepared: &PreparedPluginPublish) -> serde_json::Value {
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

pub(crate) fn plugin_changelog(files: &[(String, Vec<u8>)]) -> String {
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

pub(crate) fn build_skill_json(
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

pub(crate) fn build_search_lite_index(catalog: &CatalogDoc) -> serde_json::Value {
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

pub(crate) fn build_market_index_for_category(
    catalog: &CatalogDoc,
    category: &str,
) -> serde_json::Value {
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
