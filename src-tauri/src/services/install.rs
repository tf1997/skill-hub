use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, OptionalExtension};

use crate::{
    db::{
        canonical_display_path, insert_audit, list_bindings_inner, list_cached_packages_inner,
        list_local_skills_inner, list_market_skills_inner, list_plugin_bindings_inner,
        list_sources_inner, list_target_roots_inner, new_id, now, AppState, LOCAL_SOURCE_ID,
    },
    models::{
        CachedSkillPackage, DeleteCachedPluginRequest, DeleteCachedSkillRequest,
        DeleteLocalSkillRequest, ImportLocalSkillRequest, InstallCachedSkillRequest,
        InstallPluginRequest, InstallSkillRequest, LocalSkill, MarketPlugin, MarketSkill,
        PluginBinding, PluginManifest, PluginPackageRef, PluginVersion, SetBindingEnabledRequest,
        SetLocalSkillEnabledRequest, SetPluginBindingEnabledRequest, SkillBinding, SkillManifest,
        SkillVersion, Source, UninstallPluginRequest, UpgradeBindingRequest,
        UpgradePluginBindingRequest,
    },
    process_util::external_command,
    services::{local, market, object_store, package, preview, validation},
};
pub(crate) async fn install_skill_inner(
    request: InstallSkillRequest,
    state: &AppState,
) -> Result<SkillBinding> {
    validation::validate_target(&request.target)?;
    validation::validate_level(&request.level)?;
    let _metadata_sync_error = market::refresh_catalog_best_effort(state).await;

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

pub(crate) async fn install_plugin_inner(
    request: InstallPluginRequest,
    state: &AppState,
) -> Result<PluginBinding> {
    validation::validate_plugin_target(&request.target)?;
    validation::validate_plugin_scope(&request.scope)?;
    validation::validate_plugin_target_scope(&request.target, &request.scope)?;
    let _metadata_sync_error = market::refresh_catalog_best_effort(state).await;

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

pub(crate) fn delete_cached_skill_inner(
    request: DeleteCachedSkillRequest,
    state: &AppState,
) -> Result<()> {
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

fn remove_plugin_package_cache_dir(
    state: &AppState,
    namespace: &str,
    plugin_id: &str,
    version: &str,
    target: &str,
) -> Result<()> {
    let package_dir = plugin_package_cache_dir(state, namespace, plugin_id, version, target);
    if package_dir.exists() {
        package::ensure_safe_plugin_package_cache_path(state, &package_dir)?;
        fs::remove_dir_all(&package_dir)
            .context("PLUGIN_MARKETPLACE_WRITE_FAILED: remove cached plugin package failed")?;
    }
    Ok(())
}
pub(crate) fn delete_cached_plugin_inner(
    request: DeleteCachedPluginRequest,
    state: &AppState,
) -> Result<()> {
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
        remove_plugin_package_cache_dir(
            state,
            &request.namespace,
            &request.plugin_id,
            &request.version,
            &request.target,
        )?;
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
    remove_plugin_package_cache_dir(
        state,
        &request.namespace,
        &request.plugin_id,
        &request.version,
        &request.target,
    )?;

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

pub(crate) fn delete_local_skill_inner(
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

pub(crate) fn set_local_skill_enabled_inner(
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

pub(crate) fn import_local_skill_to_cache_inner(
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

pub(crate) fn install_cached_skill_inner(
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

fn plugin_package_cache_dir(
    state: &AppState,
    namespace: &str,
    plugin_id: &str,
    version: &str,
    target: &str,
) -> PathBuf {
    state
        .app_dir
        .join("plugin-packages")
        .join(format!("{namespace}.{plugin_id}"))
        .join(version)
        .join(target)
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
    let package_dir =
        plugin_package_cache_dir(state, &plugin.namespace, &plugin.id, version, target);

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

pub(crate) fn plugin_package_ref_for_target<'a>(
    version_info: &'a PluginVersion,
    target: &str,
) -> Option<&'a PluginPackageRef> {
    match target {
        "codex" => version_info.packages.codex.as_ref(),
        "claude" => version_info.packages.claude.as_ref(),
        _ => None,
    }
}

pub(crate) async fn plugin_component_inventory_json(
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

pub(crate) fn set_binding_enabled_inner(
    request: SetBindingEnabledRequest,
    state: &AppState,
) -> Result<SkillBinding> {
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
}

pub(crate) async fn upgrade_skill_binding_inner(
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

pub(crate) async fn upgrade_plugin_binding_inner(
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

pub(crate) fn uninstall_binding_inner(
    binding_id: String,
    state: &AppState,
) -> Result<Vec<SkillBinding>> {
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
}

pub(crate) fn ensure_scope_can_enable(
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

pub(crate) fn find_same_scope_binding(
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

pub(crate) fn ensure_plugin_scope_can_enable(
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

pub(crate) fn ensure_plugin_scope_can_enable_from_bindings(
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

pub(crate) fn find_same_plugin_binding(
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
pub(crate) struct MaterializedPluginMarketplace {
    id: String,
    name: String,
    root_path: PathBuf,
    marketplace_path: PathBuf,
}

pub(crate) fn materialize_plugin_marketplace(
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

pub(crate) fn prepare_plugin_marketplace_root(
    state: &AppState,
    target: &str,
    scope: &str,
    project_path: Option<&str>,
) -> Result<PathBuf> {
    let root = plugin_marketplace_root(state, target, scope, project_path)?;
    migrate_claude_project_marketplace_root(target, scope, project_path, &root)?;
    Ok(root)
}

pub(crate) fn migrate_claude_project_marketplace_root(
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

pub(crate) fn claude_marketplace_plugin_source_paths(doc: &serde_json::Value) -> Vec<PathBuf> {
    doc.get("plugins")
        .and_then(|plugins| plugins.as_array())
        .into_iter()
        .flatten()
        .filter_map(|plugin| plugin.get("source").and_then(|source| source.as_str()))
        .filter_map(normalize_claude_marketplace_plugin_source)
        .map(PathBuf::from)
        .collect()
}

pub(crate) fn normalize_claude_marketplace_plugin_source(source: &str) -> Option<String> {
    let normalized = source.replace('\\', "/");
    let relative = normalized.strip_prefix("./").unwrap_or(&normalized);
    let relative = package::normalize_zip_relative_path(relative)?;
    relative.starts_with("plugins/").then_some(relative)
}

pub(crate) fn remove_dir_if_empty(path: &Path) -> Result<()> {
    if path.is_dir() && fs::read_dir(path)?.next().is_none() {
        fs::remove_dir(path)?;
    }
    Ok(())
}

pub(crate) fn plugin_marketplace_name(
    target: &str,
    scope: &str,
    project_path: Option<&str>,
) -> String {
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

pub(crate) fn plugin_marketplace_display_name(marketplace_name: &str) -> String {
    if marketplace_name == "personal" {
        "Personal".to_string()
    } else {
        "Skill Hub".to_string()
    }
}

pub(crate) fn write_plugin_marketplace_file(
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

pub(crate) fn ensure_claude_marketplace_schema(mut doc: serde_json::Value) -> serde_json::Value {
    if !doc.get("owner").is_some_and(|value| value.is_object()) {
        doc["owner"] = plugin_marketplace_owner();
    }
    doc
}

pub(crate) fn plugin_marketplace_owner() -> serde_json::Value {
    serde_json::json!({
        "name": "Skill Hub"
    })
}

pub(crate) fn upsert_plugin_marketplace_entry(
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

pub(crate) fn remove_plugin_marketplace_entry(
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

pub(crate) fn rewrite_plugin_marketplace_entry(
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

pub(crate) fn remove_plugin_from_marketplace_file(
    target: &str,
    root: &Path,
    plugin_id: &str,
) -> Result<()> {
    let path = local::plugin_marketplace_path(target, root);
    let Some(doc) = read_json_file(&path) else {
        return Ok(());
    };
    let doc = remove_plugin_marketplace_entry(doc, plugin_id);
    fs::write(&path, serde_json::to_vec_pretty(&doc)?)
        .context("PLUGIN_MARKETPLACE_WRITE_FAILED: remove plugin marketplace entry failed")?;
    Ok(())
}

pub(crate) fn should_sync_codex_plugin_cli(target: &str, scope: &str) -> bool {
    target == "codex" && scope != "project"
}

pub(crate) fn sync_codex_plugin_install(
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

pub(crate) fn sync_codex_plugin_remove(
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

pub(crate) fn sync_claude_plugin_install(
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
pub(crate) enum ClaudePluginRemoveAction {
    Disable,
    Uninstall,
}

pub(crate) fn sync_claude_plugin_remove(
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

pub(crate) fn build_claude_plugin_install_commands(
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

pub(crate) fn build_claude_marketplace_remove_command(
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

pub(crate) fn build_claude_plugin_remove_command(
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

pub(crate) fn normalize_claude_plugin_scope(scope: &str) -> String {
    match scope {
        "project" | "local" => scope.to_string(),
        _ => "user".to_string(),
    }
}

pub(crate) fn is_claude_project_like_scope(scope: &str) -> bool {
    matches!(
        normalize_claude_plugin_scope(scope).as_str(),
        "project" | "local"
    )
}

pub(crate) fn claude_plugin_command_working_dir(
    scope: &str,
    project_path: Option<&str>,
) -> Option<PathBuf> {
    if !is_claude_project_like_scope(scope) {
        return None;
    }
    project_path
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaudeCommandFailureMode {
    IgnoreMissing,
    IgnoreAlreadyConfigured,
}

pub(crate) fn run_claude_plugin_command(
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

pub(crate) fn claude_plugin_command_candidates() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut candidates = Vec::new();
    if cfg!(windows) {
        candidates.push(("cmd", vec!["/C", "claude.cmd"]));
    }
    candidates.push(("claude", Vec::new()));
    candidates
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexCommandFailureMode {
    Strict,
    IgnoreMissing,
    IgnoreAlreadyConfigured,
}

pub(crate) fn run_codex_plugin_command(
    args: &[&str],
    failure_mode: CodexCommandFailureMode,
) -> Result<()> {
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

pub(crate) fn run_plugin_command<F>(
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

pub(crate) fn plugin_cli_missing_guidance(program: &str) -> String {
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

pub(crate) fn is_plugin_missing_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("not installed")
        || lower.contains("not found")
        || lower.contains("no plugin")
        || lower.contains("unknown plugin")
}

pub(crate) fn is_plugin_already_configured_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    (lower.contains("already") || lower.contains("duplicate")) && lower.contains("marketplace")
}

pub(crate) fn is_plugin_already_installed_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("already") && lower.contains("installed")
}

pub(crate) fn is_plugin_already_enabled_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("already") && lower.contains("enabled")
}

pub(crate) fn user_home_dir() -> Result<PathBuf> {
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

pub(crate) fn set_plugin_binding_enabled_inner(
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

pub(crate) fn uninstall_plugin_inner(
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

pub(crate) fn market_plugin_from_binding(binding: &PluginBinding) -> MarketPlugin {
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

pub(crate) fn ensure_package_record(
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

pub(crate) fn ensure_plugin_package_record(
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

pub(crate) fn build_install_path(
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

pub(crate) fn ensure_install_path_not_bound_to_other_skill(
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

pub(crate) fn is_sqlite_managed_install_path(binding: &SkillBinding, path: &Path) -> bool {
    if !path.exists() || !path.is_dir() {
        return false;
    }

    let legacy_leaf = format!("{}.{}", binding.namespace, binding.skill_id);
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|leaf| leaf == binding.skill_id || leaf == legacy_leaf)
}

pub(crate) fn disabled_local_skill_path(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("本地 skill 路径缺少父目录"))?;
    let leaf = path
        .file_name()
        .ok_or_else(|| anyhow!("本地 skill 路径缺少目录名"))?;
    Ok(parent.join(local::DISABLED_SKILLS_DIR).join(leaf))
}

pub(crate) fn enabled_local_skill_path(path: &Path) -> Result<PathBuf> {
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
