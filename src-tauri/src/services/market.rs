use std::{fs, path::Path};

use anyhow::{anyhow, Context, Result};
use rusqlite::params;

use crate::{
    db::{
        app_bootstrap, enforce_compiled_source, insert_audit, list_bindings_inner,
        list_cached_plugin_versions_inner, list_cached_versions_inner, list_market_plugins_inner,
        list_market_skills_inner, list_plugin_bindings_inner, list_projects_inner,
        list_sources_inner, list_target_roots_inner, list_update_candidates_inner,
        market_project_cache_path, new_id, now, AppState,
    },
    models::{
        AppBootstrap, CatalogDoc, CategoriesDoc, Category, MarketPlugin, MarketSkill,
        PluginCatalogDoc, Project, ProjectsDoc, SaveProjectRequest, SaveSourceRequest,
        SaveTargetRootRequest, Source, TargetRoot, UpdateCandidate,
    },
    services::{admin, object_store, validation},
};

pub(crate) async fn list_market_skills(state: &AppState) -> Result<Vec<MarketSkill>> {
    let _metadata_sync_error = refresh_catalog_best_effort(state).await;
    let conn = state.conn.lock().expect("db mutex poisoned");
    let bindings = list_bindings_inner(&conn)?;
    let mut skills = list_market_skills_inner(&conn)?;

    for skill in &mut skills {
        skill.installed_bindings = bindings
            .iter()
            .filter(|binding| binding.namespace == skill.namespace && binding.skill_id == skill.id)
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
}

pub(crate) async fn list_market_plugins(state: &AppState) -> Result<Vec<MarketPlugin>> {
    let _metadata_sync_error = refresh_catalog_best_effort(state).await;
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
}

pub(crate) fn list_sources(state: &AppState) -> Result<Vec<Source>> {
    let conn = state.conn.lock().expect("db mutex poisoned");
    enforce_compiled_source(&conn)?;
    list_sources_inner(&conn)
}

pub(crate) fn save_source(_request: SaveSourceRequest, _state: &AppState) -> Result<Source> {
    Err(anyhow!("数据源由代码配置强制控制，客户端不能修改"))
}

pub(crate) fn list_target_roots(state: &AppState) -> Result<Vec<TargetRoot>> {
    let conn = state.conn.lock().expect("db mutex poisoned");
    list_target_roots_inner(&conn)
}

pub(crate) fn save_target_root(
    request: SaveTargetRootRequest,
    state: &AppState,
) -> Result<TargetRoot> {
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
}

pub(crate) fn list_projects(state: &AppState) -> Result<Vec<Project>> {
    let conn = state.conn.lock().expect("db mutex poisoned");
    list_projects_inner(&conn)
}

pub(crate) fn save_project(request: SaveProjectRequest, state: &AppState) -> Result<Project> {
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
}

pub(crate) fn unbind_project(project_id: String, state: &AppState) -> Result<Vec<Project>> {
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
}

pub(crate) async fn refresh_app_bootstrap(state: &AppState) -> Result<AppBootstrap> {
    refresh_catalog_inner(state).await?;
    app_bootstrap(state, None)
}

pub(crate) async fn list_update_candidates(state: &AppState) -> Result<Vec<UpdateCandidate>> {
    let _metadata_sync_error = refresh_catalog_best_effort(state).await;
    let conn = state.conn.lock().expect("db mutex poisoned");
    list_update_candidates_inner(&conn)
}
pub(crate) async fn refresh_catalog_best_effort(state: &AppState) -> Option<String> {
    match refresh_catalog_inner(state).await {
        Ok(_) => None,
        Err(err) => Some(err.to_string()),
    }
}

pub(crate) async fn refresh_catalog_inner(state: &AppState) -> Result<Vec<MarketSkill>> {
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
        let catalog_url = object_store::object_url(&source, admin::CATALOG_OBJECT)?;
        let plugin_catalog_url = object_store::object_url(&source, admin::PLUGIN_CATALOG_OBJECT)?;
        let categories_url = object_store::object_url(&source, admin::CATEGORIES_OBJECT)?;
        let projects_url = object_store::object_url(&source, admin::PROJECTS_OBJECT)?;

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

        let plugin_catalog_response = client
            .get(plugin_catalog_url)
            .send()
            .await
            .context("connect plugin-catalog.v1.json failed")?;
        let plugin_catalog_status = plugin_catalog_response.status();
        let plugin_catalog_doc: Option<PluginCatalogDoc> = if plugin_catalog_status
            == reqwest::StatusCode::NOT_FOUND
        {
            None
        } else {
            Some(
                plugin_catalog_response
                    .error_for_status()
                    .with_context(|| {
                        format!("read plugin-catalog.v1.json failed: HTTP {plugin_catalog_status}")
                    })?
                    .json()
                    .await
                    .context("parse plugin-catalog.v1.json failed")?,
            )
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
            upsert_categories(&conn, admin::normalize_categories_doc(doc).items)?;
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

pub(crate) fn upsert_categories(
    conn: &rusqlite::Connection,
    categories: Vec<Category>,
) -> Result<()> {
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

pub(crate) fn public_categories_from_catalog(catalog: &CatalogDoc) -> Vec<Category> {
    catalog
        .categories
        .iter()
        .filter(|category| !category.starts_with("project:"))
        .enumerate()
        .map(|(index, category)| Category {
            id: category.clone(),
            name: admin::category_name_from_slug(category),
            order: 10 + index as i64 * 10,
        })
        .collect()
}

pub(crate) fn ensure_missing_categories(
    conn: &rusqlite::Connection,
    categories: Vec<Category>,
) -> Result<()> {
    for category in admin::normalize_categories_doc(CategoriesDoc {
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
