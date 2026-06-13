use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use reqwest::{header, Url};
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use tauri::State;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

use crate::{
    admin_config,
    db::{
        app_bootstrap, canonical_display_path, enforce_compiled_source, insert_audit,
        list_bindings_inner, list_cached_versions_inner, list_local_skills_inner,
        list_market_skills_inner, list_projects_inner, list_sources_inner, list_target_roots_inner,
        list_update_candidates_inner, market_project_cache_path, new_id, now, AppState,
        COMPILED_SOURCE_BUCKET, COMPILED_SOURCE_ENDPOINT, COMPILED_SOURCE_REGION,
    },
    models::{
        AdminDraftPreviewRequest, AdminDraftSkill, AdminSession, AdminUnlockRequest,
        AppBootstrap, ArchiveMarketSkillRequest, CatalogDoc, CategoriesDoc, Category,
        CommandError, DeleteCachedSkillRequest, DeleteMarketCategoryRequest,
        DeleteMarketProjectRequest, InstallSkillRequest, LocalSkill, MarketProject,
        MarketSkill, PackageInfo, Project, ProjectsDoc, PublishDraftRequest, PublishMeta,
        QuickRepublishRequest, SaveMarketCategoryRequest, SaveMarketProjectRequest,
        SaveProjectRequest, SavePublishMetaRequest, SaveSourceRequest, SaveTargetRootRequest,
        SetBindingEnabledRequest, SkillBinding, SkillManifest, SkillPreview,
        SkillPreviewFile, SkillPreviewFileEntry, SkillPreviewRequest, SkillVersion, Source,
        TargetRoot, UpdateCandidate, UpgradeBindingRequest,
    },
};

type CommandResult<T> = std::result::Result<T, CommandError>;

const DRAFT_GITLAB_PREFIX: &str = "draft/gitlab/skills/";
const DRAFT_ADMIN_PREFIX: &str = "draft/admin/gitlab/skills/";
const ARCHIVED_ADMIN_PREFIX: &str = "draft/admin/archived/skills/";
const PROJECTS_OBJECT: &str = "projects.v1.json";
const CATALOG_OBJECT: &str = "catalog.v1.json";
const CATEGORIES_OBJECT: &str = "categories.v1.json";
const FIXED_PUBLISH_NAMESPACE: &str = "DT";
const PREVIEW_MAX_FILES: usize = 8;
const PREVIEW_MAX_FILE_LIST: usize = 500;
const PREVIEW_MAX_BYTES: usize = 24 * 1024;

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
pub async fn unlock_admin_mode(request: AdminUnlockRequest) -> CommandResult<AdminSession> {
    map_result(unlock_admin_mode_inner(request).await)
}

async fn unlock_admin_mode_inner(request: AdminUnlockRequest) -> Result<AdminSession> {
    let authorization = ensure_admin_allowed(&request.admin_key).await?;

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

async fn ensure_admin_allowed(admin_key: &str) -> Result<admin_config::AdminAuthorization> {
    if !admin_config::is_admin_key_valid(admin_key) {
        return Err(anyhow!("管理员密钥错误"));
    }

    let allowlist = fetch_admin_mac_allowlist().await?;
    let local_macs = admin_config::local_mac_addresses();
    let Some(authorization) = admin_config::admin_authorization(&local_macs, &allowlist) else {
        let allowed = admin_config::allowed_admin_macs(&allowlist)
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");
        let detected = if local_macs.is_empty() {
            "未识别到本机 MAC".to_string()
        } else {
            local_macs.join(", ")
        };
        return Err(anyhow!(
            "本机 MAC 地址不在管理员白名单中；本机识别到: {detected}；白名单: {allowed}"
        ));
    };

    if authorization.role == "project" && authorization.projects.is_empty() {
        return Err(anyhow!("项目管理员未配置授权项目"));
    }

    Ok(authorization)
}

#[tauri::command]
pub async fn list_admin_drafts(
    admin_key: String,
    _state: State<'_, AppState>,
) -> CommandResult<Vec<AdminDraftSkill>> {
    map_result(list_admin_drafts_inner(&admin_key).await)
}

#[tauri::command]
pub async fn preview_admin_draft(
    request: AdminDraftPreviewRequest,
    _state: State<'_, AppState>,
) -> CommandResult<SkillPreview> {
    map_result(preview_admin_draft_inner(request).await)
}

#[tauri::command]
pub async fn save_publish_meta(
    request: SavePublishMetaRequest,
    _state: State<'_, AppState>,
) -> CommandResult<PublishMeta> {
    map_result(save_publish_meta_inner(request).await)
}

#[tauri::command]
pub async fn save_market_project_remote(
    request: SaveMarketProjectRequest,
    state: State<'_, AppState>,
) -> CommandResult<Vec<MarketProject>> {
    map_result(save_market_project_remote_inner(request, &state).await)
}

#[tauri::command]
pub async fn delete_market_project_remote(
    request: DeleteMarketProjectRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        delete_market_project_remote_inner(request, &state).await?;
        refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn save_market_category_remote(
    request: SaveMarketCategoryRequest,
    _state: State<'_, AppState>,
) -> CommandResult<Vec<Category>> {
    map_result(save_market_category_remote_inner(request).await)
}

#[tauri::command]
pub async fn delete_market_category_remote(
    request: DeleteMarketCategoryRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        delete_market_category_remote_inner(request).await?;
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
        archive_market_skill_inner(request).await?;
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
        publish_draft_inner(request).await?;
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
        quick_republish_archived_skill_inner(request).await?;
        refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

async fn list_admin_drafts_inner(admin_key: &str) -> Result<Vec<AdminDraftSkill>> {
    ensure_admin_allowed(admin_key).await?;
    let client = AdminObjectClient::new();
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
            .map(|meta| merge_publish_meta_defaults(
                normalize_publish_meta_for_source(meta, &source_path),
                default_meta.clone(),
            ))
            .or(Some(default_meta));
        let state_json = client
            .get_optional_json::<serde_json::Value>(&state_path)
            .await?;
        let published_version = state_json
            .as_ref()
            .and_then(|value| value.get("publishedVersion").or_else(|| value.get("published_version")))
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
        let (gitlab_category_code, draft_slug) = split_gitlab_source_path(&source_path);

        drafts.push(AdminDraftSkill {
            gitlab_source_path: source_path,
            draft_slug,
            gitlab_category_code,
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
    for object in archived_objects
        .iter()
        .filter(|object| object.ends_with("/state.v1.json") && object.starts_with(ARCHIVED_ADMIN_PREFIX))
    {
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

        drafts.push(AdminDraftSkill {
            gitlab_source_path: source_path,
            draft_slug: if skill_id.is_empty() { None } else { Some(skill_id.to_string()) },
            gitlab_category_code: if namespace.is_empty() { None } else { Some(namespace.to_string()) },
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

async fn preview_admin_draft_inner(request: AdminDraftPreviewRequest) -> Result<SkillPreview> {
    let authorization = ensure_admin_allowed(&request.admin_key).await?;
    let client = AdminObjectClient::new();
    let source_path = normalize_relative_object_path(&request.gitlab_source_path)?;
    let selected_path = normalize_preview_file_path(request.file_path.as_deref())?;
    let draft_prefix = format!("{}{}/", DRAFT_GITLAB_PREFIX, source_path);
    let skill_md_path = format!("{draft_prefix}SKILL.md");
    let objects = client.list_objects(&draft_prefix).await?;
    if !objects.iter().any(|object| object == &skill_md_path) {
        return Err(anyhow!("Draft SKILL.md not found: {source_path}"));
    }

    let skill_md = client.get_text(&skill_md_path).await?;
    let (_, draft_slug) = split_gitlab_source_path(&source_path);
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
                    value.publish_project_slug.as_deref().unwrap_or("unselected")
                )
            } else {
                format!(
                    "public / {}",
                    value.publish_category_slug.as_deref().unwrap_or("general")
                )
            }
        })
        .unwrap_or_else(|| "publish metadata missing".to_string());
    let mut file_list = collect_draft_preview_file_list(&draft_prefix, &objects);
    if meta.is_some()
        && !file_list
            .iter()
            .any(|file| file.path == "publish-meta.v1.json")
    {
        file_list.push(preview_file_entry("publish-meta.v1.json"));
    }
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

async fn save_publish_meta_inner(request: SavePublishMetaRequest) -> Result<PublishMeta> {
    let authorization = ensure_admin_allowed(&request.admin_key).await?;
    let client = AdminObjectClient::new();
    let source_path = normalize_relative_object_path(&request.gitlab_source_path)?;
    let mut meta = normalize_publish_meta_for_source(request.meta, &source_path);
    validate_publish_meta(&meta)?;
    ensure_can_manage_publish_target(&authorization, &meta)?;
    validate_publish_target(&client, &meta).await?;
    meta.updated_at = Some(now());
    if meta.updated_by.as_deref().unwrap_or("").trim().is_empty() {
        meta.updated_by = Some(admin_actor(&authorization));
    }
    let path = admin_object_path(&source_path, "publish-meta.v1.json")?;
    client.put_json(&path, &meta).await?;
    Ok(meta)
}

async fn save_market_project_remote_inner(
    request: SaveMarketProjectRequest,
    state: &AppState,
) -> Result<Vec<MarketProject>> {
    let authorization = ensure_admin_allowed(&request.admin_key).await?;
    let project_slug = request.project.slug.trim().to_string();
    validate_object_segment("project slug", &project_slug)?;
    ensure_can_manage_project(&authorization, &project_slug)?;

    let client = AdminObjectClient::new();
    let mut projects = load_remote_projects(&client).await?;
    let mut project = request.project;
    project.slug = project.slug.trim().to_string();
    project.name = project.name.trim().to_string();
    project.description = project.description.trim().to_string();
    project.status = project.status.trim().to_string();
    if project.name.is_empty() {
        project.name = project.slug.clone();
    }
    if project.status.trim().is_empty() {
        project.status = "active".to_string();
    }
    let timestamp = now();
    if project.created_at.is_none() {
        project.created_at = Some(timestamp.clone());
    }
    project.updated_at = Some(timestamp);
    if project.updated_by.is_none() {
        project.updated_by = Some(admin_actor(&authorization));
    }

    projects.retain(|item| item.slug != project.slug);
    projects.push(project);
    projects.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.slug.cmp(&b.slug)));
    save_remote_projects(&client, &projects).await?;
    fs::write(
        market_project_cache_path(&state.app_dir),
        serde_json::to_string_pretty(&projects_doc(projects.clone()))?,
    )?;
    Ok(projects)
}

async fn delete_market_project_remote_inner(
    request: DeleteMarketProjectRequest,
    state: &AppState,
) -> Result<()> {
    let authorization = ensure_admin_allowed(&request.admin_key).await?;
    let slug = request.slug.trim().to_string();
    validate_object_segment("project slug", &slug)?;
    ensure_can_manage_project(&authorization, &slug)?;

    let client = AdminObjectClient::new();
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

    catalog.categories.retain(|category| category != &project_category);
    catalog.generated_at = Some(now());
    save_remote_projects(&client, &projects).await?;
    write_all_market_indexes(&client, &catalog).await?;
    write_admin_audit(
        &client,
        "deleteMarketProject",
        serde_json::json!({
            "slug": slug,
            "actor": admin_actor(&authorization),
            "role": authorization.role,
            "createdAt": now()
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
) -> Result<Vec<Category>> {
    let authorization = ensure_admin_allowed(&request.admin_key).await?;
    ensure_system_admin(&authorization)?;
    let category_id = request.category.id.trim().to_string();
    validate_object_segment("category slug", &category_id)?;

    let client = AdminObjectClient::new();
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
    categories.items.push(category);
    categories = normalize_categories_doc(categories);
    categories.generated_at = Some(now());
    client.put_json(CATEGORIES_OBJECT, &categories).await?;
    write_admin_audit(
        &client,
        "saveMarketCategory",
        serde_json::json!({
            "actor": admin_actor(&authorization),
            "role": authorization.role,
            "createdAt": now()
        }),
    )
    .await?;
    Ok(categories.items)
}

async fn delete_market_category_remote_inner(
    request: DeleteMarketCategoryRequest,
) -> Result<()> {
    let authorization = ensure_admin_allowed(&request.admin_key).await?;
    ensure_system_admin(&authorization)?;
    let category_id = request.category_id.trim().to_string();
    validate_object_segment("category slug", &category_id)?;
    if category_id == "general" || category_id == "public" {
        return Err(anyhow!("内置通用分类不能删除"));
    }

    let client = AdminObjectClient::new();
    let mut catalog = load_remote_catalog(&client).await?;
    if catalog
        .skills
        .iter()
        .any(|skill| skill.categories.iter().any(|category| category == &category_id))
    {
        return Err(anyhow!("公共分类仍有关联 skill，请先下架相关 skill"));
    }

    let mut categories = load_remote_categories(&client).await?;
    categories.items.retain(|category| category.id != category_id);
    categories = normalize_categories_doc(categories);
    catalog.categories.retain(|category| category != &category_id);
    categories.generated_at = Some(now());
    catalog.generated_at = Some(now());
    client.put_json(CATEGORIES_OBJECT, &categories).await?;
    write_all_market_indexes(&client, &catalog).await?;
    write_admin_audit(
        &client,
        "deleteMarketCategory",
        serde_json::json!({
            "categoryId": category_id,
            "actor": admin_actor(&authorization),
            "role": authorization.role,
            "createdAt": now()
        }),
    )
    .await?;
    Ok(())
}

async fn publish_draft_inner(request: PublishDraftRequest) -> Result<()> {
    let authorization = ensure_admin_allowed(&request.admin_key).await?;
    let client = AdminObjectClient::new();
    let source_path = normalize_relative_object_path(&request.gitlab_source_path)?;
    let skill_md_path = format!("{}{}/SKILL.md", DRAFT_GITLAB_PREFIX, source_path);
    let skill_md = client
        .get_text(&skill_md_path)
        .await
        .with_context(|| format!("读取草稿 SKILL.md 失败: {skill_md_path}"))?;
    let draft_metadata = parse_skill_frontmatter(&skill_md);
    let version = draft_metadata.version.clone()
        .ok_or_else(|| anyhow!("草稿 SKILL.md 缺少 version"))?;
    let author = draft_metadata.author.clone()
        .ok_or_else(|| anyhow!("草稿 SKILL.md 缺少 author"))?;

    let meta_path = admin_object_path(&source_path, "publish-meta.v1.json")?;
    let state_path = admin_object_path(&source_path, "state.v1.json")?;
    let default_meta = default_publish_meta_from_draft(&source_path, &draft_metadata);
    let meta = client
        .get_optional_json::<PublishMeta>(&meta_path)
        .await?
        .map(|meta| merge_publish_meta_defaults(
            normalize_publish_meta_for_source(meta, &source_path),
            default_meta.clone(),
        ))
        .unwrap_or(default_meta);
    let state_json = client
        .get_optional_json::<serde_json::Value>(&state_path)
        .await?;
    let published_version = state_json
        .as_ref()
        .and_then(|value| value.get("publishedVersion").or_else(|| value.get("published_version")))
        .and_then(|value| value.as_str());
    let state_archived = state_json
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("archived"));
    if !state_archived && published_version == Some(version.as_str()) {
        return Err(anyhow!("该草稿当前版本已发布，禁止重复发布"));
    }
    validate_publish_meta(&meta)?;
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
    let package_bytes = build_package_zip(&files)?;
    let package_hash = sha256_hex(&package_bytes);
    let package_size = package_bytes.len() as i64;
    let skill_json = build_skill_json(&meta, &version, &author, &package_hash, package_size);
    let job_id = new_id();
    let job_path = format!("admin/publish-jobs/{job_id}.json");

    let base = format!("skills/{}/{}/versions/{}", meta.namespace, meta.skill_id, version);
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
    catalog.skills.retain(|skill| !(skill.namespace == meta.namespace && skill.id == meta.skill_id));
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
        client.put_bytes(&package_object, package_bytes, "application/zip").await?;
        client
            .put_text(&sha_object, &(package_hash.clone() + "\n"), "text/plain; charset=utf-8")
            .await?;
        client
            .put_text(&changelog_object, &meta.changelog, "text/markdown; charset=utf-8")
            .await?;
    }
    client.put_json(&manifest_object, &manifest).await?;
    client.put_json(CATEGORIES_OBJECT, &categories).await?;
    client.put_json(PROJECTS_OBJECT, &projects_doc(projects)).await?;
    write_market_indexes_for_categories(&client, &catalog, &affected_categories).await?;
    client.put_json("indexes/search-lite.v1.json", &search_index).await?;

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
    let audit_path = format!("admin/audit/{}/publish-{}.json", now()[0..10].replace('-', "/"), new_id());
    client
        .put_json(
            &audit_path,
            &serde_json::json!({
                "schema": "skillhub.admin-audit.v1",
                "action": "publishDraft",
                "actor": admin_actor(&authorization),
                "role": authorization.role,
                "job": publish_job,
                "state": state,
                "createdAt": now()
            }),
        )
        .await?;

    client.put_json(CATALOG_OBJECT, &catalog).await?;
    Ok(())
}

async fn quick_republish_archived_skill_inner(request: QuickRepublishRequest) -> Result<()> {
    let authorization = ensure_admin_allowed(&request.admin_key).await?;
    let client = AdminObjectClient::new();
    let source_path = normalize_relative_object_path(&request.gitlab_source_path)?;

    // 1. 读取保存的元数据（而不是 SKILL.md）
    // 尝试从 gitlab 路径和 archived 路径读取
    let meta_path_gitlab = admin_object_path(&source_path, "publish-meta.v1.json")?;
    let meta_path_archived = format!("{}{}/publish-meta.v1.json", ARCHIVED_ADMIN_PREFIX, source_path);

    let meta = match client.get_optional_json::<PublishMeta>(&meta_path_gitlab).await? {
        Some(m) => m,
        None => client
            .get_optional_json::<PublishMeta>(&meta_path_archived)
            .await?
            .ok_or_else(|| anyhow!("未找到已保存的发布元数据。该 skill 可能未曾发布过，无法使用快速重新上架功能。"))?,
    };

    // 2. 验证元数据完整性
    validate_publish_meta(&meta)?;
    ensure_can_manage_publish_target(&authorization, &meta)?;

    // 3. 检查状态：必须是已下架状态
    // 同样尝试两个路径
    let state_path_gitlab = admin_object_path(&source_path, "state.v1.json")?;
    let state_path_archived = format!("{}{}/state.v1.json", ARCHIVED_ADMIN_PREFIX, source_path);

    let (state_json, actual_state_path) = match client.get_optional_json::<serde_json::Value>(&state_path_gitlab).await? {
        Some(s) => (Some(s), state_path_gitlab),
        None => (
            client.get_optional_json::<serde_json::Value>(&state_path_archived).await?,
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
        let skill_json_exists = client.get_optional_text(&version_info.skill_path).await?.is_some();
        if !skill_json_exists {
            return Err(anyhow!("市场中的 skill 包文件不存在: {}。无法重新上架。", version_info.skill_path));
        }

        manifest.latest_version.clone()
    } else {
        // 5b. 如果 manifest 不存在，尝试从 state.v1.json 读取版本
        state_json
            .as_ref()
            .and_then(|v| v.get("publishedVersion").or_else(|| v.get("published_version")))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!(
                "无法确定 skill 版本信息。该 skill 的下架记录不完整，缺少版本号。\n\n\
                可能的原因：\n\
                1. 该 skill 从未成功发布过，只是创建了草稿\n\
                2. 下架时版本信息未被保存\n\
                3. 市场中的包文件已被完全删除\n\n\
                建议：请使用正常发布流程重新发布该 skill。"
            ))?
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
    client.put_json(PROJECTS_OBJECT, &projects_doc(projects)).await?;
    write_market_indexes_for_categories(&client, &catalog, &affected_categories).await?;
    client.put_json("indexes/search-lite.v1.json", &search_index).await?;
    client.put_json(CATALOG_OBJECT, &catalog).await?;

    // 11. 记录审计日志
    let audit_path = format!("admin/audit/{}/republish-{}.json", now()[0..10].replace('-', "/"), new_id());
    client
        .put_json(
            &audit_path,
            &serde_json::json!({
                "schema": "skillhub.admin-audit.v1",
                "action": "quickRepublishArchivedSkill",
                "actor": admin_actor(&authorization),
                "role": authorization.role,
                "namespace": meta.namespace,
                "skillId": meta.skill_id,
                "version": latest_version,
                "state": new_state,
                "createdAt": now()
            }),
        )
        .await?;

    Ok(())
}

async fn archive_market_skill_inner(request: ArchiveMarketSkillRequest) -> Result<()> {
    let authorization = ensure_admin_allowed(&request.admin_key).await?;
    validate_object_segment("namespace", &request.namespace)?;
    validate_object_segment("skill id", &request.skill_id)?;

    let client = AdminObjectClient::new();
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
        .put_json("indexes/search-lite.v1.json", &build_search_lite_index(&catalog))
        .await?;
    client.put_json(CATALOG_OBJECT, &catalog).await?;

    let source_path = find_draft_source_for_skill(&client, &request.namespace, &request.skill_id).await?;
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
        "archiveMarketSkill",
        serde_json::json!({
            "namespace": request.namespace,
            "skillId": request.skill_id,
            "categories": skill.categories,
            "statePath": state_path,
            "actor": admin_actor(&authorization),
            "role": authorization.role,
            "createdAt": now()
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
        validate_target(&request.target)?;
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
pub async fn delete_cached_skill(
    request: DeleteCachedSkillRequest,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    map_result(delete_cached_skill_inner(request, &state))
}

async fn install_skill_inner(
    request: InstallSkillRequest,
    state: &AppState,
) -> Result<SkillBinding> {
    validate_target(&request.target)?;
    validate_level(&request.level)?;
    let _metadata_sync_error = refresh_catalog_best_effort(state).await;

    if request.level == "project" && request.project_path.as_deref().unwrap_or("").is_empty() {
        return Err(anyhow!("项目级启用必须选择项目目录"));
    }

    let (source_id, skill, source) = {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let source_id = request.source_id.clone().or_else(|| {
            default_source_for_skill(&conn, &request.namespace, &request.skill_id)
                .ok()
                .flatten()
        });
        let skill = find_market_skill(
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
        copy_package_to_install(&package_path, &install_path)?;
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
        ensure_safe_package_cache_path(state, &path)?;
        fs::remove_dir_all(&path).context("删除本地包缓存失败")?;
    }

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

async fn fetch_manifest_version(
    source: &Source,
    manifest_path: &str,
    version: &str,
) -> Result<SkillVersion> {
    let manifest_url = object_url(source, manifest_path)?;
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

async fn prepare_package(
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
        remove_json_files_recursive(&package_dir)?;
        return Ok(package_dir);
    }

    let source = source.ok_or_else(|| anyhow!("缺少 MinIO 源，无法下载 skill 包"))?;
    let version_info =
        version_info.ok_or_else(|| anyhow!("缺少远端版本信息，无法下载 skill 包"))?;

    let package_url = object_url(source, &version_info.package_path)?;
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
        verify_sha256(&bytes, expected)?;
    } else {
        let hash_url = object_url(source, &version_info.sha256_path)?;
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
        verify_sha256(&bytes, expected.trim())?;
    }

    fs::create_dir_all(&package_dir)?;
    remove_json_files_recursive(&package_dir)?;
    extract_zip_safely(&bytes, &package_dir)?;
    Ok(package_dir)
}

fn verify_sha256(bytes: &[u8], expected: &str) -> Result<()> {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let actual = format!("{:x}", hasher.finalize());
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err(anyhow!("SHA-256 校验失败"))
    }
}

fn extract_zip_safely(bytes: &[u8], destination: &Path) -> Result<()> {
    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader).context("打开 zip 包失败")?;

    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let Some(enclosed_name) = file.enclosed_name().map(|path| path.to_owned()) else {
            return Err(anyhow!("zip 包包含不安全路径"));
        };

        let out_path = destination.join(enclosed_name);
        if !file.is_dir() && is_json_file(&out_path) {
            continue;
        }

        if file.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = fs::File::create(&out_path)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }

    Ok(())
}

fn remove_json_files_recursive(root: &Path) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            remove_json_files_recursive(&path)?;
        } else if is_json_file(&path) {
            fs::remove_file(&path).with_context(|| {
                format!(
                    "清理本地包缓存 JSON 文件失败: {}",
                    canonical_display_path(&path)
                )
            })?;
        }
    }

    Ok(())
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
        copy_package_to_install(&package_path, &install_path)?;
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
    map_result((|| {
        let conn = state.conn.lock().expect("db mutex poisoned");
        conn.execute("DELETE FROM local_skills", [])?;

        let bindings = list_bindings_inner(&conn)?;
        let target_roots = list_target_roots_inner(&conn)?;
        let projects = list_projects_inner(&conn)?;
        let scanned_at = now();
        let mut seen_paths = HashSet::new();

        for binding in &bindings {
            let exists = PathBuf::from(&binding.install_path).exists();
            seen_paths.insert(canonical_display_path(Path::new(&binding.install_path)));
            conn.execute(
                "INSERT INTO local_skills
                 (id, target, level, project_path, path, detected_manifest, managed_by_skillhub, status, scanned_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8)",
                params![
                    new_id(),
                    binding.target,
                    binding.level,
                    binding.project_path,
                    binding.install_path,
                    format!("{}@{}", binding.skill_name, binding.version),
                    if exists { "managed" } else { "missing" },
                    scanned_at
                ],
            )?;
        }

        for root in target_roots {
            scan_skill_root(
                &conn,
                &mut seen_paths,
                &root.target,
                "personal",
                None,
                Path::new(&root.personal_path),
                &scanned_at,
            )?;
        }

        for project in projects {
            for target in ["codex", "claude"] {
                let root = resolve_project_skill_root(target, Path::new(&project.path));
                scan_skill_root(
                    &conn,
                    &mut seen_paths,
                    target,
                    "project",
                    Some(project.path.as_str()),
                    &root,
                    &scanned_at,
                )?;
            }
        }

        list_local_skills_inner(&conn)
    })())
}

#[tauri::command]
pub async fn preview_skill(
    request: SkillPreviewRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillPreview> {
    map_result(preview_skill_inner(request, &state).await)
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

async fn refresh_catalog_best_effort(state: &AppState) -> Option<String> {
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
        let catalog_url = object_url(&source, "catalog.v1.json")?;
        let categories_url = object_url(&source, "categories.v1.json")?;
        let projects_url = object_url(&source, "projects.v1.json")?;

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
        if !is_valid_object_segment_value(&category.id) || category.id.starts_with("project:") {
            continue;
        }
        category.name = category.name.trim().to_string();
        if category.id == "public" {
            category.name = "公共".to_string();
        } else if category.name.is_empty() {
            category.name = category_name_from_slug(&category.id);
        }
        by_id.insert(category.id.clone(), category);
    }

    let mut items = by_id.into_values().collect::<Vec<_>>();
    items.sort_by(|a, b| {
        category_sort_priority(&a.id)
            .cmp(&category_sort_priority(&b.id))
            .then_with(|| a.order.cmp(&b.order))
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

    items.sort_by(|a, b| {
        category_sort_priority(&a.id)
            .cmp(&category_sort_priority(&b.id))
            .then_with(|| a.order.cmp(&b.order))
            .then_with(|| a.id.cmp(&b.id))
    });

    doc.items = items;
    doc
}

fn category_sort_priority(id: &str) -> i32 {
    if id == "public" || id == "general" {
        0
    } else {
        1
    }
}

fn category_name_from_slug(slug: &str) -> String {
    if slug == "general" || slug == "public" {
        return "通用".to_string();
    }

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
struct DraftSkillMetadata {
    name: Option<String>,
    description: Option<String>,
    tags: Vec<String>,
    version: Option<String>,
    author: Option<String>,
}

fn default_publish_meta_from_draft(
    source_path: &str,
    metadata: &DraftSkillMetadata,
) -> PublishMeta {
    let skill_id = draft_skill_id_from_source_path(source_path);
    PublishMeta {
        namespace: FIXED_PUBLISH_NAMESPACE.to_string(),
        skill_id: skill_id.clone(),
        name: metadata.name.clone().unwrap_or_else(|| skill_id.clone()),
        summary: metadata.description.clone().unwrap_or_default(),
        tags: metadata.tags.clone(),
        targets: Vec::new(),
        levels: vec!["personal".to_string(), "project".to_string()],
        publish_scope: "public".to_string(),
        publish_category_slug: Some("general".to_string()),
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

fn draft_skill_id_from_source_path(source_path: &str) -> String {
    let (_, draft_slug) = split_gitlab_source_path(source_path);
    draft_slug
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "skill".to_string())
}

fn parse_skill_frontmatter(content: &str) -> DraftSkillMetadata {
    let mut metadata = DraftSkillMetadata::default();
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
            if section.as_deref() == Some("tags") {
                if let Some(tag) = clean_frontmatter_value(item) {
                    push_unique_tag(&mut metadata.tags, tag);
                }
            }
            continue;
        }

        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        if indent == 0 {
            section = Some(key.clone());
        }

        match (indent == 0, section.as_deref(), key.as_str()) {
            (true, _, "name") => metadata.name = clean_frontmatter_value(value),
            (true, _, "description") => metadata.description = clean_frontmatter_value(value),
            (true, _, "tags") => {
                for tag in parse_frontmatter_tags(value) {
                    push_unique_tag(&mut metadata.tags, tag);
                }
            }
            (true, _, "version") => metadata.version = clean_frontmatter_value(value),
            (true, _, "author") => metadata.author = clean_frontmatter_value(value),
            (false, Some("metadata"), "version") => {
                metadata.version = clean_frontmatter_value(value)
            }
            (false, Some("metadata"), "author") => {
                metadata.author = clean_frontmatter_value(value)
            }
            _ => {}
        }
    }

    metadata
}

fn parse_frontmatter_tags(value: &str) -> Vec<String> {
    let Some(cleaned) = clean_frontmatter_value(value) else {
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
            .filter_map(clean_frontmatter_value)
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
            return clean_frontmatter_value(value);
        }
    }
    None
}

fn split_gitlab_source_path(source_path: &str) -> (Option<String>, Option<String>) {
    let parts = source_path.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        [] => (None, None),
        [single] => (None, Some((*single).to_string())),
        [category, rest @ ..] => (
            Some((*category).to_string()),
            rest.last().map(|value| (*value).to_string()),
        ),
    }
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
        Some(value) => !is_publish_meta_complete(value),
        None => true,
    } {
        return "元数据待补充".to_string();
    }
    match published_version {
        Some(published) if Some(published) == version => "已发布".to_string(),
        Some(published) if version.is_some_and(|value| value > published) => {
            "可升级".to_string()
        }
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
        .map(|value| !matches!(value.trim().to_ascii_lowercase().as_str(), "" | "passed" | "ok" | "success"))
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

fn is_publish_meta_complete(meta: &PublishMeta) -> bool {
    !meta.namespace.trim().is_empty()
        && !meta.skill_id.trim().is_empty()
        && !meta.name.trim().is_empty()
        && !meta.summary.trim().is_empty()
        && ((meta.publish_scope == "project"
            && meta
                .publish_project_slug
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()))
            || (meta.publish_scope != "project"
                && meta
                    .publish_category_slug
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())))
}

fn validate_publish_meta(meta: &PublishMeta) -> Result<()> {
    if !is_publish_meta_complete(meta) {
        return Err(anyhow!("发布元数据不完整"));
    }
    validate_object_segment("namespace", &meta.namespace)?;
    validate_object_segment("skill id", &meta.skill_id)?;
    if meta.publish_scope == "project" {
        validate_object_segment(
            "project slug",
            meta.publish_project_slug.as_deref().unwrap_or(""),
        )?;
    } else {
        validate_object_segment(
            "category slug",
            meta.publish_category_slug.as_deref().unwrap_or(""),
        )?;
    }
    Ok(())
}

async fn validate_publish_target(client: &AdminObjectClient, meta: &PublishMeta) -> Result<()> {
    if meta.publish_scope == "project" {
        let project_slug = meta.publish_project_slug.as_deref().unwrap_or("");
        let projects = load_remote_projects(client).await?;
        if projects
            .iter()
            .any(|project| project.slug == project_slug && project.status != "archived")
        {
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
            meta.skill_id, version
        ))
    } else {
        Ok(version_exists)
    }
}

fn validate_object_segment(name: &str, value: &str) -> Result<()> {
    if is_valid_object_segment_value(value) {
        Ok(())
    } else {
        Err(anyhow!("{name} 只能包含字母、数字、点、下划线和短横线"))
    }
}

fn is_valid_object_segment_value(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && !trimmed.contains("..")
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn normalize_relative_object_path(value: &str) -> Result<String> {
    let path = value.trim().trim_matches('/');
    if path.is_empty()
        || path.contains("..")
        || path.contains('\\')
        || path.split('/').any(|part| part.trim().is_empty())
    {
        return Err(anyhow!("对象路径不合法"));
    }
    Ok(path.to_string())
}

fn admin_object_path(source_path: &str, leaf: &str) -> Result<String> {
    Ok(format!(
        "{}{}/{}",
        DRAFT_ADMIN_PREFIX,
        normalize_relative_object_path(source_path)?,
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
        vec![meta
            .publish_category_slug
            .clone()
            .unwrap_or_else(|| "general".to_string())]
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

async fn load_remote_catalog(client: &AdminObjectClient) -> Result<CatalogDoc> {
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

async fn load_remote_categories(client: &AdminObjectClient) -> Result<CategoriesDoc> {
    let doc = client
        .get_optional_json::<CategoriesDoc>(CATEGORIES_OBJECT)
        .await?
        .unwrap_or_else(|| CategoriesDoc {
            schema: "skillhub.categories.v1".to_string(),
            generated_at: Some(now()),
            items: vec![Category {
                id: "general".to_string(),
                name: "通用".to_string(),
                order: 10,
            }],
        });
    Ok(normalize_categories_doc(doc))
}

fn ensure_publish_category(mut doc: CategoriesDoc, meta: &PublishMeta) -> CategoriesDoc {
    if meta.publish_scope != "project" {
        let slug = meta
            .publish_category_slug
            .clone()
            .unwrap_or_else(|| "general".to_string());
        if !doc.items.iter().any(|item| item.id == slug) {
            doc.items.push(Category {
                id: slug.clone(),
                name: if slug == "general" {
                    "通用".to_string()
                } else {
                    slug
                },
                order: 10 + doc.items.len() as i64 * 10,
            });
        }
    }
    doc.generated_at = Some(now());
    normalize_categories_doc(doc)
}

async fn load_remote_projects(client: &AdminObjectClient) -> Result<Vec<MarketProject>> {
    Ok(client
        .get_optional_json::<ProjectsDoc>(PROJECTS_OBJECT)
        .await?
        .map(ProjectsDoc::into_projects)
        .unwrap_or_default())
}

async fn save_remote_projects(
    client: &AdminObjectClient,
    projects: &[MarketProject],
) -> Result<()> {
    client.put_json(PROJECTS_OBJECT, &projects_doc(projects.to_vec())).await
}

fn projects_doc(projects: Vec<MarketProject>) -> ProjectsDoc {
    ProjectsDoc {
        schema: "skillhub.projects.v1".to_string(),
        generated_at: Some(now()),
        projects,
        items: Vec::new(),
    }
}

async fn write_all_market_indexes(client: &AdminObjectClient, catalog: &CatalogDoc) -> Result<()> {
    let categories = rebuild_catalog_categories(&catalog.skills);
    let search_index = build_search_lite_index(catalog);
    write_market_indexes_for_categories(client, catalog, &categories).await?;
    client.put_json("indexes/search-lite.v1.json", &search_index).await?;
    client.put_json(CATALOG_OBJECT, catalog).await?;
    Ok(())
}

async fn write_market_indexes_for_categories(
    client: &AdminObjectClient,
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
    client: &AdminObjectClient,
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
            &serde_json::json!({
                "schema": "skillhub.admin-audit.v1",
                "action": action,
                "payload": payload,
                "createdAt": now()
            }),
        )
        .await
}

async fn find_draft_source_for_skill(
    client: &AdminObjectClient,
    namespace: &str,
    skill_id: &str,
) -> Result<Option<String>> {
    let objects = client.list_objects(DRAFT_ADMIN_PREFIX).await?;
    for object in objects
        .iter()
        .filter(|object| object.ends_with("/state.v1.json") && object.starts_with(DRAFT_ADMIN_PREFIX))
    {
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

async fn read_draft_files(
    client: &AdminObjectClient,
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

fn draft_source_fingerprint(files: &[(String, Vec<u8>)]) -> serde_json::Value {
    let mut items = files
        .iter()
        .map(|(path, bytes)| {
            serde_json::json!({
                "path": path,
                "size": bytes.len(),
                "sha256": sha256_hex(bytes)
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
        "digest": sha256_hex(&canonical),
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
    relatives.truncate(PREVIEW_MAX_FILE_LIST);
    relatives
        .iter()
        .map(|relative| preview_file_entry(relative))
        .collect()
}

async fn collect_draft_preview_files(
    client: &AdminObjectClient,
    draft_prefix: &str,
    file_list: &[SkillPreviewFileEntry],
    selected_path: Option<&str>,
    meta: Option<&PublishMeta>,
) -> Result<Vec<SkillPreviewFile>> {
    let candidates = preview_candidate_paths(file_list, selected_path);

    let mut files = Vec::new();
    for relative in candidates {
        if files.len() >= PREVIEW_MAX_FILES {
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
        if let Some(file) = preview_file_from_bytes(&relative, &bytes, PREVIEW_MAX_BYTES) {
            files.push(file);
        }
    }

    Ok(files)
}

fn preview_candidate_paths(
    file_list: &[SkillPreviewFileEntry],
    selected_path: Option<&str>,
) -> Vec<String> {
    let mut candidates = Vec::new();
    let push_candidate = |candidates: &mut Vec<String>, relative: &str| {
        if candidates.iter().any(|item| item == relative) {
            return;
        }
        if file_list
            .iter()
            .any(|file| file.path == relative && file.previewable)
        {
            candidates.push(relative.to_string());
        }
    };

    if let Some(path) = selected_path {
        push_candidate(&mut candidates, path);
    }

    for relative in [
        "SKILL.md",
        "publish-meta.v1.json",
        "README.md",
        "CHANGELOG.md",
        "changelog.md",
        "skill.json",
        "validation.json",
    ] {
        push_candidate(&mut candidates, relative);
    }

    for file in file_list {
        if candidates.len() >= PREVIEW_MAX_FILES {
            break;
        }
        if file.previewable && !candidates.iter().any(|item| item == &file.path) {
            candidates.push(file.path.clone());
        }
    }

    candidates
}

fn build_package_zip(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (relative, bytes) in files {
        if relative.ends_with('/') || is_package_control_file(relative) {
            continue;
        }
        writer.start_file(relative.replace('\\', "/"), options)?;
        std::io::Write::write_all(&mut writer, bytes)?;
    }
    Ok(writer.finish()?.into_inner())
}

fn is_package_control_file(relative: &str) -> bool {
    matches!(
        relative.replace('\\', "/").to_ascii_lowercase().as_str(),
        "skill.json" | "publish-meta.v1.json" | "state.v1.json" | "validation.json"
    )
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

fn extract_xml_values(input: &str, tag: &str) -> Vec<String> {
    let start = format!("<{tag}>");
    let end = format!("</{tag}>");
    let mut rest = input;
    let mut values = Vec::new();
    while let Some(start_index) = rest.find(&start) {
        let after_start = &rest[start_index + start.len()..];
        let Some(end_index) = after_start.find(&end) else {
            break;
        };
        values.push(xml_unescape(&after_start[..end_index]));
        rest = &after_start[end_index + end.len()..];
    }
    values
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn object_url(source: &Source, object_path: &str) -> Result<Url> {
    let base = format!(
        "{}/{}/{}",
        source.endpoint.trim_end_matches('/'),
        source.bucket.trim_matches('/'),
        object_path.trim_start_matches('/')
    );
    Url::parse(&base).context("对象 URL 无效")
}

struct AdminObjectClient {
    source: Source,
    client: reqwest::Client,
}

impl AdminObjectClient {
    fn new() -> Self {
        Self {
            source: Source {
                id: "admin-publisher".to_string(),
                name: "Admin Publisher".to_string(),
                endpoint: COMPILED_SOURCE_ENDPOINT.to_string(),
                bucket: COMPILED_SOURCE_BUCKET.to_string(),
                region: COMPILED_SOURCE_REGION.map(ToString::to_string),
                enabled: true,
                last_sync_at: None,
            },
            client: reqwest::Client::new(),
        }
    }

    async fn get_text(&self, object_path: &str) -> Result<String> {
        let url = object_url(&self.source, object_path)?;
        let signed = signed_request_headers("GET", &url, self.source.region.as_deref(), b"")?;
        let mut request = self.client.get(url);
        for (name, value) in signed {
            request = request.header(name, value);
        }
        request
            .send()
            .await
            .with_context(|| format!("读取 MinIO 对象失败: {object_path}"))?
            .error_for_status()
            .with_context(|| format!("MinIO 对象响应失败: {object_path}"))?
            .text()
            .await
            .with_context(|| format!("读取 MinIO 对象内容失败: {object_path}"))
    }

    async fn get_optional_text(&self, object_path: &str) -> Result<Option<String>> {
        let url = object_url(&self.source, object_path)?;
        let signed = signed_request_headers("GET", &url, self.source.region.as_deref(), b"")?;
        let mut request = self.client.get(url);
        for (name, value) in signed {
            request = request.header(name, value);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("读取 MinIO 对象失败: {object_path}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(
            response
                .error_for_status()
                .with_context(|| format!("MinIO 对象响应失败: {object_path}"))?
                .text()
                .await
                .with_context(|| format!("读取 MinIO 对象内容失败: {object_path}"))?,
        ))
    }

    async fn get_bytes(&self, object_path: &str) -> Result<Vec<u8>> {
        let url = object_url(&self.source, object_path)?;
        let signed = signed_request_headers("GET", &url, self.source.region.as_deref(), b"")?;
        let mut request = self.client.get(url);
        for (name, value) in signed {
            request = request.header(name, value);
        }
        Ok(request
            .send()
            .await
            .with_context(|| format!("读取 MinIO 对象失败: {object_path}"))?
            .error_for_status()
            .with_context(|| format!("MinIO 对象响应失败: {object_path}"))?
            .bytes()
            .await
            .with_context(|| format!("读取 MinIO 对象内容失败: {object_path}"))?
            .to_vec())
    }

    async fn get_optional_json<T>(&self, object_path: &str) -> Result<Option<T>>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let Some(text) = self.get_optional_text(object_path).await? else {
            return Ok(None);
        };
        serde_json::from_str(&text)
            .with_context(|| format!("解析 JSON 对象失败: {object_path}"))
            .map(Some)
    }

    async fn put_json<T: serde::Serialize>(&self, object_path: &str, value: &T) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(value)?;
        self.put_bytes(object_path, bytes, "application/json; charset=utf-8")
            .await
    }

    async fn put_text(&self, object_path: &str, value: &str, content_type: &str) -> Result<()> {
        self.put_bytes(object_path, value.as_bytes().to_vec(), content_type)
            .await
    }

    async fn put_bytes(
        &self,
        object_path: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<()> {
        let url = object_url(&self.source, object_path)?;
        let signed = signed_request_headers("PUT", &url, self.source.region.as_deref(), &bytes)?;
        let mut request = self
            .client
            .put(url)
            .header(header::CONTENT_TYPE, content_type)
            .body(bytes);
        for (name, value) in signed {
            request = request.header(name, value);
        }
        request
            .send()
            .await
            .with_context(|| format!("写入 MinIO 对象失败: {object_path}"))?
            .error_for_status()
            .with_context(|| format!("MinIO 写入响应失败: {object_path}"))?;
        Ok(())
    }

    async fn list_objects(&self, prefix: &str) -> Result<Vec<String>> {
        let mut results = Vec::new();
        let mut continuation: Option<String> = None;

        loop {
            let mut url = Url::parse(&format!(
                "{}/{}",
                self.source.endpoint.trim_end_matches('/'),
                self.source.bucket.trim_matches('/')
            ))
            .context("对象列表 URL 无效")?;
            {
                let mut pairs = url.query_pairs_mut();
                pairs.append_pair("list-type", "2");
                pairs.append_pair("prefix", prefix);
                pairs.append_pair("max-keys", "1000");
                if let Some(token) = continuation.as_deref() {
                    pairs.append_pair("continuation-token", token);
                }
            }

            let signed = signed_request_headers("GET", &url, self.source.region.as_deref(), b"")?;
            let mut request = self.client.get(url);
            for (name, value) in signed {
                request = request.header(name, value);
            }
            let text = request
                .send()
                .await
                .with_context(|| format!("列出 MinIO 前缀失败: {prefix}"))?
                .error_for_status()
                .with_context(|| format!("MinIO 前缀列表响应失败: {prefix}"))?
                .text()
                .await
                .context("读取 MinIO 前缀列表失败")?;

            results.extend(extract_xml_values(&text, "Key"));
            continuation = extract_xml_values(&text, "NextContinuationToken")
                .into_iter()
                .next();
            if continuation.is_none() {
                break;
            }
        }

        Ok(results)
    }
}

async fn fetch_admin_mac_allowlist() -> Result<admin_config::MacAllowlist> {
    let client = AdminObjectClient::new();
    let object_path = admin_config::allowlist_path();
    let text = client.get_text(object_path).await?;

    admin_config::parse_mac_allowlist(&text).with_context(|| {
        format!(
            "解析 MinIO MAC 白名单失败，请检查 {} 的 JSON 格式",
            object_path
        )
    })
}

fn signed_request_headers(
    method: &str,
    url: &Url,
    region: Option<&str>,
    payload: &[u8],
) -> Result<Vec<(&'static str, String)>> {
    let request_time = Utc::now();
    let amz_date = request_time.format("%Y%m%dT%H%M%SZ").to_string();
    let short_date = request_time.format("%Y%m%d").to_string();
    let region = region.filter(|value| !value.trim().is_empty()).unwrap_or("us-east-1");
    let host = url_host(url)?;
    let payload_hash = sha256_hex(payload);

    let canonical_request = format!(
        "{}\n{}\n{}\nhost:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n\nhost;x-amz-content-sha256;x-amz-date\n{}",
        method,
        canonical_uri(url),
        canonical_query(url),
        host,
        payload_hash,
        amz_date,
        payload_hash
    );
    let credential_scope = format!("{}/{}/s3/aws4_request", short_date, region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date,
        credential_scope,
        sha256_hex(canonical_request.as_bytes())
    );
    let signing_key = sigv4_signing_key(admin_config::publisher_secret_key(), &short_date, region);
    let signature = hex_lower(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature={}",
        admin_config::publisher_access_key(),
        credential_scope,
        signature
    );

    Ok(vec![
        ("host", host),
        ("x-amz-content-sha256", payload_hash),
        ("x-amz-date", amz_date),
        ("authorization", authorization),
    ])
}

fn url_host(url: &Url) -> Result<String> {
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("MinIO endpoint 缺少 host"))?;
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

fn canonical_uri(url: &Url) -> String {
    let path = url.path();
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

fn canonical_query(url: &Url) -> String {
    let mut pairs = url
        .query_pairs()
        .map(|(key, value)| (uri_encode(&key, true), uri_encode(&value, true)))
        .collect::<Vec<_>>();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn uri_encode(value: &str, encode_slash: bool) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        let ch = *byte as char;
        let keep = ch.is_ascii_alphanumeric()
            || matches!(ch, '-' | '_' | '.' | '~')
            || (!encode_slash && ch == '/');
        if keep {
            out.push(ch);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn sigv4_signing_key(secret: &str, short_date: &str, region: &str) -> Vec<u8> {
    let date_key = hmac_sha256(format!("AWS4{secret}").as_bytes(), short_date.as_bytes());
    let date_region_key = hmac_sha256(&date_key, region.as_bytes());
    let date_region_service_key = hmac_sha256(&date_region_key, b"s3");
    hmac_sha256(&date_region_service_key, b"aws4_request")
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    const BLOCK_SIZE: usize = 64;

    let mut normalized_key = if key.len() > BLOCK_SIZE {
        Sha256::digest(key).to_vec()
    } else {
        key.to_vec()
    };
    normalized_key.resize(BLOCK_SIZE, 0);

    let mut outer_key_pad = [0x5c_u8; BLOCK_SIZE];
    let mut inner_key_pad = [0x36_u8; BLOCK_SIZE];
    for index in 0..BLOCK_SIZE {
        outer_key_pad[index] ^= normalized_key[index];
        inner_key_pad[index] ^= normalized_key[index];
    }

    let mut inner = Sha256::new();
    inner.update(inner_key_pad);
    inner.update(message);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(outer_key_pad);
    outer.update(inner_hash);
    outer.finalize().to_vec()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn validate_target(target: &str) -> Result<()> {
    match target {
        "codex" | "claude" => Ok(()),
        _ => Err(anyhow!("不支持的目标平台: {target}")),
    }
}

fn validate_level(level: &str) -> Result<()> {
    match level {
        "personal" | "project" => Ok(()),
        _ => Err(anyhow!("不支持的作用域: {level}")),
    }
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

fn scan_skill_root(
    conn: &rusqlite::Connection,
    seen_paths: &mut HashSet<String>,
    target: &str,
    level: &str,
    project_path: Option<&str>,
    root: &Path,
    scanned_at: &str,
) -> Result<()> {
    if !root.exists() || !root.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let display_path = canonical_display_path(&path);
        if !seen_paths.insert(display_path.clone()) {
            continue;
        }

        let Some(detected) = detect_local_skill_label(&path) else {
            continue;
        };

        conn.execute(
            "INSERT INTO local_skills
             (id, target, level, project_path, path, detected_manifest, managed_by_skillhub, status, scanned_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 'unmanaged', ?7)",
            params![
                new_id(),
                target,
                level,
                project_path,
                display_path,
                detected,
                scanned_at
            ],
        )?;
    }

    Ok(())
}

fn detect_local_skill_label(path: &Path) -> Option<String> {
    let skill_md = path.join("SKILL.md");
    if !skill_md.is_file() {
        return None;
    }

    skill_name_from_dir(path)
        .or_else(|| read_skill_markdown_name(&skill_md))
        .or_else(|| read_skill_markdown_title(&skill_md))
        .or_else(|| Some("local-skill".to_string()))
}

fn skill_name_from_dir(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
}

fn read_skill_markdown_name(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    parse_skill_markdown_name(&content)
}

fn read_skill_markdown_title(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    parse_skill_markdown_title(&content)
}

fn parse_skill_markdown_name(content: &str) -> Option<String> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }

    for line in lines.take(80) {
        let trimmed = line.trim();
        if trimmed == "---" || trimmed == "..." {
            break;
        }

        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };

        if key.trim().eq_ignore_ascii_case("name") {
            return clean_frontmatter_value(value);
        }
    }

    None
}

fn clean_frontmatter_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let unquoted = if trimmed.len() >= 2 {
        let first = trimmed.as_bytes()[0] as char;
        let last = trimmed.as_bytes()[trimmed.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            &trimmed[1..trimmed.len() - 1]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    let name = unquoted.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn parse_skill_markdown_title(content: &str) -> Option<String> {
    for line in content.lines().take(80) {
        let trimmed = line.trim();
        if !trimmed.starts_with('#') {
            continue;
        }

        let title = trimmed.trim_start_matches('#').trim();
        if !title.is_empty() {
            return Some(title.to_string());
        }
    }

    None
}

fn display_skill_name_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("local-skill")
        .to_string()
}

async fn preview_skill_inner(
    request: SkillPreviewRequest,
    state: &AppState,
) -> Result<SkillPreview> {
    let should_refresh_market_metadata =
        request.binding_id.is_none() && request.path.is_none() && request.version.is_none();
    if should_refresh_market_metadata {
        let _metadata_sync_error = refresh_catalog_best_effort(state).await;
    }
    let selected_path = normalize_preview_file_path(request.file_path.as_deref())?;

    let (title, origin, root_path) = if let Some(binding_id) = request.binding_id.as_deref() {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let binding = find_binding(&conn, binding_id)?;
        (
            binding.skill_name,
            format!("{} / {}", binding.target, binding.level),
            PathBuf::from(binding.install_path),
        )
    } else if let Some(path) = request.path.as_deref() {
        let root_path = PathBuf::from(path);
        let title = detect_local_skill_label(&root_path)
            .unwrap_or_else(|| display_skill_name_from_path(&root_path));
        (title, "本地目录".to_string(), root_path)
    } else {
        let namespace = request
            .namespace
            .as_deref()
            .ok_or_else(|| anyhow!("缺少 namespace"))?;
        let skill_id = request
            .skill_id
            .as_deref()
            .ok_or_else(|| anyhow!("缺少 skill id"))?;
        let requested_source_id = request.source_id.clone();
        let requested_version = request.version.clone();
        let cached_preview = {
            let conn = state.conn.lock().expect("db mutex poisoned");
            let source_id = requested_source_id.clone().or_else(|| {
                default_source_for_skill(&conn, namespace, skill_id)
                    .ok()
                    .flatten()
            });
            match requested_version.as_deref() {
                Some(version) => find_cached_package_preview(
                    &conn,
                    source_id.as_deref(),
                    namespace,
                    skill_id,
                    version,
                )?,
                None => None,
            }
        };

        if let Some((skill_name, package_path)) = cached_preview {
            (
                skill_name,
                "本地缓存".to_string(),
                PathBuf::from(package_path),
            )
        } else {
            let (source_id, skill, source) = {
                let conn = state.conn.lock().expect("db mutex poisoned");
                let source_id = requested_source_id.or_else(|| {
                    default_source_for_skill(&conn, namespace, skill_id)
                        .ok()
                        .flatten()
                });
                let skill = find_market_skill(&conn, source_id.as_deref(), namespace, skill_id)?;
                let source = source_id.as_deref().and_then(|id| {
                    list_sources_inner(&conn)
                        .ok()?
                        .into_iter()
                        .find(|item| item.id == id)
                });
                (source_id, skill, source)
            };
            let version = requested_version.unwrap_or_else(|| skill.latest_version.clone());
            let version_info = match source.as_ref() {
                Some(source) => {
                    Some(fetch_manifest_version(source, &skill.manifest_path, &version).await?)
                }
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
            (
                skill.name,
                source_id.unwrap_or_else(|| "本地缓存".to_string()),
                package_path,
            )
        }
    };

    if !root_path.exists() || !root_path.is_dir() {
        return Err(anyhow!("预览目录不存在"));
    }

    let (file_list, files) = collect_preview_files(&root_path, selected_path.as_deref())?;

    Ok(SkillPreview {
        title,
        root_path: canonical_display_path(&root_path),
        origin,
        files,
        file_list,
    })
}

fn collect_preview_files(
    root: &Path,
    selected_path: Option<&str>,
) -> Result<(Vec<SkillPreviewFileEntry>, Vec<SkillPreviewFile>)> {
    let file_list = collect_preview_file_list(root)?;
    let candidates = preview_candidate_paths(&file_list, selected_path);

    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for relative in candidates {
        if files.len() >= PREVIEW_MAX_FILES || !seen.insert(relative.clone()) {
            continue;
        }
        let path = root.join(&relative);
        if !path.exists() || !path.is_file() {
            continue;
        }
        if let Some(file) = read_preview_file(&relative, &path, PREVIEW_MAX_BYTES)? {
            files.push(file);
        }
    }

    Ok((file_list, files))
}

fn collect_preview_file_list(root: &Path) -> Result<Vec<SkillPreviewFileEntry>> {
    let mut entries = Vec::new();
    collect_preview_candidates(root, root, &mut entries, PREVIEW_MAX_FILE_LIST)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn collect_preview_candidates(
    root: &Path,
    current: &Path,
    candidates: &mut Vec<SkillPreviewFileEntry>,
    max_files: usize,
) -> Result<()> {
    if candidates.len() >= max_files {
        return Ok(());
    }

    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }

        if path.is_dir() {
            collect_preview_candidates(root, &path, candidates, max_files)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            candidates.push(preview_file_entry(&relative));
        }

        if candidates.len() >= max_files {
            break;
        }
    }

    Ok(())
}

fn is_previewable_relative_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "txt"
                    | "json"
                    | "toml"
                    | "yaml"
                    | "yml"
                    | "rs"
                    | "ts"
                    | "tsx"
                    | "js"
                    | "jsx"
                    | "py"
                    | "ps1"
            )
        })
        .unwrap_or(false)
}

fn preview_file_entry(relative: &str) -> SkillPreviewFileEntry {
    SkillPreviewFileEntry {
        path: relative.to_string(),
        language: language_for_relative_path(relative),
        previewable: is_previewable_relative_path(relative),
    }
}

fn normalize_preview_file_path(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let path = value.trim().trim_matches('/');
    if path.is_empty()
        || path.contains("..")
        || path.contains('\\')
        || path.split('/').any(|part| part.trim().is_empty())
    {
        return Err(anyhow!("预览文件路径不合法"));
    }
    Ok(Some(path.to_string()))
}

fn read_preview_file(
    relative: &str,
    path: &Path,
    max_bytes: usize,
) -> Result<Option<SkillPreviewFile>> {
    let bytes = fs::read(path)?;
    let truncated = bytes.len() > max_bytes;
    let slice = if truncated {
        &bytes[..max_bytes]
    } else {
        bytes.as_slice()
    };
    let Ok(content) = String::from_utf8(slice.to_vec()) else {
        return Ok(None);
    };

    Ok(Some(SkillPreviewFile {
        path: relative.to_string(),
        language: language_for_path(path),
        content,
        truncated,
    }))
}

fn preview_file_from_bytes(
    relative: &str,
    bytes: &[u8],
    max_bytes: usize,
) -> Option<SkillPreviewFile> {
    let truncated = bytes.len() > max_bytes;
    let slice = if truncated { &bytes[..max_bytes] } else { bytes };
    let Ok(content) = String::from_utf8(slice.to_vec()) else {
        return None;
    };

    Some(SkillPreviewFile {
        path: relative.to_string(),
        language: language_for_relative_path(relative),
        content,
        truncated,
    })
}

fn language_for_path(path: &Path) -> String {
    language_for_relative_path(&path.to_string_lossy())
}

fn language_for_relative_path(path: &str) -> String {
    match path
        .rsplit('.')
        .next()
        .filter(|extension| *extension != path)
        .or_else(|| {
            Path::new(path)
                .extension()
                .and_then(|extension| extension.to_str())
        })
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "md" => "markdown",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "ps1" => "powershell",
        _ => "text",
    }
    .to_string()
}

fn default_source_for_skill(
    conn: &rusqlite::Connection,
    namespace: &str,
    skill_id: &str,
) -> Result<Option<String>> {
    conn.query_row(
        "SELECT source_id FROM catalog_cache WHERE namespace = ?1 AND skill_id = ?2 LIMIT 1",
        params![namespace, skill_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn find_market_skill(
    conn: &rusqlite::Connection,
    source_id: Option<&str>,
    namespace: &str,
    skill_id: &str,
) -> Result<MarketSkill> {
    let skills = list_market_skills_inner(conn)?;
    skills
        .into_iter()
        .find(|skill| {
            skill.namespace == namespace
                && skill.id == skill_id
                && source_id
                    .map(|id| skill.source_id.as_deref() == Some(id))
                    .unwrap_or(true)
        })
        .ok_or_else(|| anyhow!("未找到 skill: {namespace}/{skill_id}"))
}

fn find_cached_package_preview(
    conn: &rusqlite::Connection,
    source_id: Option<&str>,
    namespace: &str,
    skill_id: &str,
    version: &str,
) -> Result<Option<(String, String)>> {
    conn.query_row(
        "SELECT COALESCE(catalog.name, binding.skill_name, package.skill_id), package.package_path
         FROM skill_packages package
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
        params![source_id, namespace, skill_id, version],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(Into::into)
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

fn ensure_safe_package_cache_path(state: &AppState, package_path: &Path) -> Result<()> {
    let package_root = state
        .app_dir
        .join("packages")
        .canonicalize()
        .context("读取包缓存根目录失败")?;
    let target = package_path.canonicalize().context("读取包缓存目录失败")?;

    if target.starts_with(&package_root) && target != package_root {
        Ok(())
    } else {
        Err(anyhow!("拒绝删除非包缓存目录"))
    }
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

fn copy_package_to_install(package_path: &Path, install_path: &Path) -> Result<()> {
    if install_path.exists() {
        fs::remove_dir_all(install_path).context("清理旧安装目录失败")?;
    }
    fs::create_dir_all(install_path)?;
    copy_dir_recursive(package_path, install_path)
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if source_path.is_dir() {
            fs::create_dir_all(&destination_path)?;
            copy_dir_recursive(&source_path, &destination_path)?;
        } else if is_json_file(&source_path) {
            continue;
        } else {
            fs::copy(&source_path, &destination_path)?;
        }
    }

    Ok(())
}

fn is_json_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

fn resolve_project_skill_root(target: &str, project_path: &Path) -> PathBuf {
    match target {
        "codex" => project_path.join(".codex").join("skills"),
        "claude" => project_path.join(".claude").join("skills"),
        _ => project_path.join(".skillhub").join(target).join("skills"),
    }
}

fn find_binding(conn: &rusqlite::Connection, binding_id: &str) -> Result<SkillBinding> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_skill_name_from_frontmatter() {
        let content = r#"---
name: api-conventions
description: API design patterns for this codebase
---

When writing API endpoints:
- Use RESTful naming conventions
"#;

        assert_eq!(
            parse_skill_markdown_name(content).as_deref(),
            Some("api-conventions")
        );
    }

    #[test]
    fn parses_quoted_skill_name_from_frontmatter() {
        let content = r#"---
name: "API Conventions"
---

# Fallback title
"#;

        assert_eq!(
            parse_skill_markdown_name(content).as_deref(),
            Some("API Conventions")
        );
    }

    #[test]
    fn falls_back_to_markdown_title_when_frontmatter_name_is_missing() {
        let content = r#"---
description: API design patterns for this codebase
---

# API Conventions
"#;

        assert_eq!(parse_skill_markdown_name(content), None);
        assert_eq!(
            parse_skill_markdown_title(content).as_deref(),
            Some("API Conventions")
        );
    }

    #[test]
    fn local_skill_label_prefers_directory_name() {
        let root = std::env::temp_dir().join(format!("skillhub-test-{}", new_id()));
        let skill_dir = root.join("api-conventions");
        fs::create_dir_all(&skill_dir).expect("create temp skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: frontmatter-name
---

# Heading Name
"#,
        )
        .expect("write temp SKILL.md");

        assert_eq!(
            detect_local_skill_label(&skill_dir).as_deref(),
            Some("api-conventions")
        );

        fs::remove_dir_all(root).expect("remove temp skill dir");
    }

    #[test]
    fn parses_skill_markdown_admin_fields() {
        let content = r#"---
version: 1.2.3
author: "Skill Hub"
---

# Demo
"#;

        assert_eq!(parse_skill_markdown_field(content, "version").as_deref(), Some("1.2.3"));
        assert_eq!(parse_skill_markdown_field(content, "author").as_deref(), Some("Skill Hub"));
    }

    #[test]
    fn validates_publish_meta_completeness() {
        let meta = PublishMeta {
            namespace: "community".to_string(),
            skill_id: "demo".to_string(),
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

        assert!(validate_publish_meta(&meta).is_ok());
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
        assert!(should_republish_existing_version(&manifest, &active_catalog, &meta, "1.0.0").is_err());
    }

    #[test]
    fn rejects_unsafe_object_segments() {
        assert!(validate_object_segment("namespace", "team_alpha-1").is_ok());
        assert!(validate_object_segment("namespace", "../team").is_err());
        assert!(normalize_relative_object_path("cat/demo").is_ok());
        assert!(normalize_relative_object_path("cat/../demo").is_err());
    }

    #[test]
    fn extracts_s3_list_keys() {
        let xml = r#"<ListBucketResult>
          <Contents><Key>draft/gitlab/skills/a/SKILL.md</Key></Contents>
          <Contents><Key>draft/gitlab/skills/a/README.md</Key></Contents>
          <NextContinuationToken>abc&amp;123</NextContinuationToken>
        </ListBucketResult>"#;

        assert_eq!(
            extract_xml_values(xml, "Key"),
            vec![
                "draft/gitlab/skills/a/SKILL.md".to_string(),
                "draft/gitlab/skills/a/README.md".to_string()
            ]
        );
        assert_eq!(
            extract_xml_values(xml, "NextContinuationToken"),
            vec!["abc&123".to_string()]
        );
    }

    #[test]
    fn package_zip_filters_control_json_but_keeps_subdirectory_files() {
        let bytes = build_package_zip(&[
            ("SKILL.md".to_string(), b"hello".to_vec()),
            ("skill.json".to_string(), b"{}".to_vec()),
            ("validation.json".to_string(), b"{}".to_vec()),
            ("references/schema.json".to_string(), b"{}".to_vec()),
            ("scripts/main.py".to_string(), b"print('ok')".to_vec()),
        ])
        .expect("zip should build");
        let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("zip should open");
        assert!(archive.by_name("SKILL.md").is_ok());
        assert!(archive.by_name("skill.json").is_err());
        assert!(archive.by_name("validation.json").is_err());
        assert!(archive.by_name("references/schema.json").is_ok());
        assert!(archive.by_name("scripts/main.py").is_ok());
    }

    #[test]
    fn normalize_categories_cleans_builtin_names_and_duplicate_order() {
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
            doc.items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            vec!["public", "backend", "yy"]
        );
        assert_eq!(doc.items[0].name, "公共");
        assert_eq!(
            doc.items.iter().map(|item| item.order).collect::<Vec<_>>(),
            vec![10, 20, 30]
        );
    }

    #[test]
    fn preview_file_from_bytes_detects_language_and_truncates() {
        let file = preview_file_from_bytes("src/main.ts", b"abcdef", 3)
            .expect("text preview should parse");

        assert_eq!(file.language, "typescript");
        assert_eq!(file.content, "abc");
        assert!(file.truncated);
        assert!(preview_file_from_bytes("asset.bin", &[0, 159], 10).is_none());
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
        unlock_admin_mode_inner(AdminUnlockRequest {
            admin_key: admin_key.clone(),
        })
        .await
        .expect("admin mode should unlock against live MinIO");

        let project = MarketProject {
            slug: "live-project".to_string(),
            name: "Live Project".to_string(),
            description: "Created by live MinIO integration test".to_string(),
            status: "active".to_string(),
            created_at: None,
            updated_at: None,
            updated_by: None,
        };
        let client = AdminObjectClient::new();
        let mut projects = load_remote_projects(&client).await.expect("load projects");
        projects.retain(|item| item.slug != project.slug);
        projects.push(project);
        save_remote_projects(&client, &projects).await.expect("save projects");

        let source_path = "product/minio-live-draft".to_string();
        let drafts = list_admin_drafts_inner(&admin_key)
            .await
            .expect("drafts should list");
        assert!(
            drafts
                .iter()
                .any(|draft| draft.gitlab_source_path == source_path && draft.version.as_deref() == Some("0.1.0")),
            "live draft should be visible"
        );

        let meta = PublishMeta {
            namespace: "live".to_string(),
            skill_id: "minio-live-draft".to_string(),
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

        save_publish_meta_inner(SavePublishMetaRequest {
            admin_key: admin_key.clone(),
            gitlab_source_path: source_path.clone(),
            meta,
        })
        .await
        .expect("save publish metadata");

        let preview = preview_admin_draft_inner(AdminDraftPreviewRequest {
            admin_key: admin_key.clone(),
            gitlab_source_path: source_path.clone(),
            file_path: None,
        })
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

        publish_draft_inner(PublishDraftRequest {
            admin_key,
            gitlab_source_path: source_path,
        })
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
            unlock_admin_mode_inner(AdminUnlockRequest {
                admin_key: admin_key.clone(),
            })
            .await
            .expect("admin mode should unlock against live MinIO");

            let client = AdminObjectClient::new();
            let project = MarketProject {
                slug: "live-project".to_string(),
                name: "Live Project".to_string(),
                description: "Created by live MinIO integration test".to_string(),
                status: "active".to_string(),
                created_at: None,
                updated_at: None,
                updated_by: None,
            };
            let mut projects = load_remote_projects(&client).await.expect("load projects");
            projects.retain(|item| item.slug != project.slug);
            projects.push(project);
            save_remote_projects(&client, &projects).await.expect("save projects");

            let source_path = "product/minio-live-draft".to_string();
            let meta = PublishMeta {
                namespace: "live".to_string(),
                skill_id: "minio-live-draft".to_string(),
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
            save_publish_meta_inner(SavePublishMetaRequest {
                admin_key: admin_key.clone(),
                gitlab_source_path: source_path.clone(),
                meta,
            })
            .await
            .expect("save publish metadata");

            let catalog = load_remote_catalog(&client).await.expect("load catalog");
            let currently_listed = catalog
                .skills
                .iter()
                .any(|skill| skill.namespace == "live" && skill.id == "minio-live-draft");
            if !currently_listed {
                publish_draft_inner(PublishDraftRequest {
                    admin_key: admin_key.clone(),
                    gitlab_source_path: source_path.clone(),
                })
                .await
                .expect("publish existing draft before archive");
            }

            archive_market_skill_inner(ArchiveMarketSkillRequest {
                admin_key: admin_key.clone(),
                namespace: "live".to_string(),
                skill_id: "minio-live-draft".to_string(),
                reason: Some("live integration archive test".to_string()),
            })
            .await
            .expect("archive market skill");

            let archived_catalog = load_remote_catalog(&client).await.expect("load catalog after archive");
            assert!(
                !archived_catalog
                    .skills
                    .iter()
                    .any(|skill| skill.namespace == "live" && skill.id == "minio-live-draft"),
                "skill should disappear from market catalog after archive"
            );

            let drafts = list_admin_drafts_inner(&admin_key).await.expect("list drafts");
            assert!(
                drafts.iter().any(|draft| {
                    draft.gitlab_source_path == source_path
                        && draft.status == "已下架"
                        && draft.source_available
                }),
                "archived GitLab draft should be visible in draft list"
            );

            publish_draft_inner(PublishDraftRequest {
                admin_key,
                gitlab_source_path: source_path,
            })
            .await
            .expect("republish archived existing version");

            let republished_catalog = load_remote_catalog(&client).await.expect("load catalog after republish");
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
