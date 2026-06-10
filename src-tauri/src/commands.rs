use std::{
    collections::HashSet,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use reqwest::Url;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use tauri::State;
use zip::ZipArchive;

use crate::{
    db::{
        app_bootstrap, canonical_display_path, enforce_compiled_source, insert_audit,
        list_bindings_inner, list_cached_versions_inner, list_local_skills_inner,
        list_market_skills_inner, list_projects_inner, list_sources_inner, list_target_roots_inner,
        list_update_candidates_inner, new_id, now, AppState,
    },
    models::{
        AppBootstrap, CatalogDoc, CategoriesDoc, Category, CommandError, DeleteCachedSkillRequest,
        InstallSkillRequest, LocalSkill, MarketSkill, Project, SaveProjectRequest,
        SaveSourceRequest, SaveTargetRootRequest, SetBindingEnabledRequest, SkillBinding,
        SkillManifest, SkillPreview, SkillPreviewFile, SkillPreviewRequest, SkillVersion, Source,
        TargetRoot, UpdateCandidate,
    },
};

type CommandResult<T> = std::result::Result<T, CommandError>;

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

        let conn = state.conn.lock().expect("db mutex poisoned");
        if let Some(doc) = categories_doc {
            upsert_categories(&conn, doc.items)?;
        }

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

fn object_url(source: &Source, object_path: &str) -> Result<Url> {
    let base = format!(
        "{}/{}/{}",
        source.endpoint.trim_end_matches('/'),
        source.bucket.trim_matches('/'),
        object_path.trim_start_matches('/')
    );
    Url::parse(&base).context("对象 URL 无效")
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

    Ok(SkillPreview {
        title,
        root_path: canonical_display_path(&root_path),
        origin,
        files: collect_preview_files(&root_path)?,
    })
}

fn collect_preview_files(root: &Path) -> Result<Vec<SkillPreviewFile>> {
    const MAX_FILES: usize = 8;
    const MAX_BYTES: usize = 24 * 1024;

    let mut candidates = Vec::new();
    for relative in [
        "SKILL.md",
        "README.md",
        "CHANGELOG.md",
        "changelog.md",
        "skill.json",
    ] {
        let path = root.join(relative);
        if path.exists() && path.is_file() {
            candidates.push((relative.to_string(), path));
        }
    }

    collect_preview_candidates(root, root, &mut candidates, MAX_FILES)?;

    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for (relative, path) in candidates {
        if files.len() >= MAX_FILES || !seen.insert(relative.clone()) {
            continue;
        }
        if let Some(file) = read_preview_file(&relative, &path, MAX_BYTES)? {
            files.push(file);
        }
    }

    Ok(files)
}

fn collect_preview_candidates(
    root: &Path,
    current: &Path,
    candidates: &mut Vec<(String, PathBuf)>,
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
        } else if is_previewable_file(&path) {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            candidates.push((relative, path));
        }

        if candidates.len() >= max_files {
            break;
        }
    }

    Ok(())
}

fn is_previewable_file(path: &Path) -> bool {
    path.extension()
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

fn language_for_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
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
}

fn map_result<T>(result: Result<T>) -> CommandResult<T> {
    result.map_err(|error| CommandError::new("COMMAND_FAILED", error.to_string()))
}
