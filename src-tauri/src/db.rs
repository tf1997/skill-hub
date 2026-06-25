use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use tauri::AppHandle;
use uuid::Uuid;

use crate::{
    admin_config,
    models::{
        AppBootstrap, CachedPluginPackage, CachedSkillPackage, Category, LocalPlugin, LocalSkill,
        MarketPlugin, MarketProject, MarketSkill, PluginBinding, Project, SkillBinding, Source,
        TargetRoot, UpdateCandidate,
    },
};

pub use crate::minio_config::{
    COMPILED_SOURCE_BUCKET, COMPILED_SOURCE_ENDPOINT, COMPILED_SOURCE_ID, COMPILED_SOURCE_NAME,
    COMPILED_SOURCE_REGION,
};

pub const LOCAL_SOURCE_ID: &str = "__local__";

#[derive(Clone)]
pub struct AppState {
    pub conn: Arc<Mutex<Connection>>,
    pub app_dir: PathBuf,
    pub local_macs: Vec<String>,
}

pub fn init_state(app: &AppHandle) -> Result<AppState> {
    let app_dir = app
        .path_resolver()
        .app_data_dir()
        .context("failed to resolve app data directory")?;

    fs::create_dir_all(&app_dir)?;
    fs::create_dir_all(app_dir.join("packages"))?;
    fs::create_dir_all(app_dir.join("plugin-packages"))?;
    fs::create_dir_all(app_dir.join("plugin-marketplaces"))?;
    fs::create_dir_all(app_dir.join("plugin-backups"))?;
    fs::create_dir_all(app_dir.join("cache").join("catalog"))?;
    fs::create_dir_all(app_dir.join("cache").join("downloads"))?;
    fs::create_dir_all(app_dir.join("installs"))?;
    fs::create_dir_all(app_dir.join("backups"))?;
    fs::create_dir_all(app_dir.join("logs"))?;

    let db_path = app_dir.join("skillhub.sqlite");
    let conn = Connection::open(db_path)?;
    migrate(&conn)?;
    enforce_compiled_source(&conn)?;
    remove_legacy_sample_skills(&conn)?;
    seed_if_empty(&conn)?;

    Ok(AppState {
        conn: Arc::new(Mutex::new(conn)),
        app_dir,
        local_macs: admin_config::local_mac_addresses(),
    })
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS sources (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          endpoint TEXT NOT NULL,
          bucket TEXT NOT NULL,
          region TEXT,
          enabled INTEGER NOT NULL DEFAULT 1,
          last_sync_at TEXT
        );

        CREATE TABLE IF NOT EXISTS categories (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          ordering INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS catalog_cache (
          source_id TEXT NOT NULL,
          namespace TEXT NOT NULL,
          skill_id TEXT NOT NULL,
          latest_version TEXT NOT NULL,
          name TEXT NOT NULL,
          summary TEXT NOT NULL,
          categories_json TEXT NOT NULL,
          tags_json TEXT NOT NULL,
          targets_json TEXT NOT NULL,
          levels_json TEXT NOT NULL,
          manifest_path TEXT NOT NULL,
          raw_manifest TEXT NOT NULL,
          etag TEXT,
          updated_at TEXT NOT NULL,
          PRIMARY KEY (source_id, namespace, skill_id)
        );

        CREATE TABLE IF NOT EXISTS skill_packages (
          id TEXT PRIMARY KEY,
          source_id TEXT,
          namespace TEXT NOT NULL,
          skill_id TEXT NOT NULL,
          version TEXT NOT NULL,
          package_path TEXT NOT NULL,
          sha256 TEXT,
          cached_at TEXT NOT NULL,
          UNIQUE(source_id, namespace, skill_id, version)
        );

        CREATE TABLE IF NOT EXISTS local_package_metadata (
          package_id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          summary TEXT NOT NULL,
          tags_json TEXT NOT NULL,
          author TEXT,
          source_path TEXT NOT NULL,
          imported_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS skill_bindings (
          id TEXT PRIMARY KEY,
          package_id TEXT NOT NULL,
          source_id TEXT,
          namespace TEXT NOT NULL,
          skill_id TEXT NOT NULL,
          skill_name TEXT NOT NULL,
          version TEXT NOT NULL,
          target TEXT NOT NULL,
          level TEXT NOT NULL,
          project_path TEXT,
          install_path TEXT NOT NULL,
          enabled INTEGER NOT NULL DEFAULT 1,
          install_mode TEXT NOT NULL,
          update_policy TEXT NOT NULL,
          status TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS plugin_catalog_cache (
          source_id TEXT NOT NULL,
          namespace TEXT NOT NULL,
          plugin_id TEXT NOT NULL,
          latest_version TEXT NOT NULL,
          name TEXT NOT NULL,
          summary TEXT NOT NULL,
          categories_json TEXT NOT NULL,
          tags_json TEXT NOT NULL,
          targets_json TEXT NOT NULL,
          scopes_json TEXT NOT NULL,
          components_json TEXT NOT NULL,
          risk_level TEXT NOT NULL,
          manifest_path TEXT NOT NULL,
          raw_manifest TEXT NOT NULL,
          etag TEXT,
          updated_at TEXT NOT NULL,
          PRIMARY KEY (source_id, namespace, plugin_id)
        );

        CREATE TABLE IF NOT EXISTS plugin_packages (
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
          cached_at TEXT NOT NULL,
          UNIQUE(source_id, namespace, plugin_id, version, target)
        );

        CREATE TABLE IF NOT EXISTS plugin_marketplaces (
          id TEXT PRIMARY KEY,
          target TEXT NOT NULL,
          scope TEXT NOT NULL,
          project_path TEXT,
          marketplace_name TEXT NOT NULL,
          root_path TEXT NOT NULL,
          marketplace_path TEXT NOT NULL,
          status TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          UNIQUE(target, scope, project_path, marketplace_name)
        );

        CREATE TABLE IF NOT EXISTS plugin_bindings (
          id TEXT PRIMARY KEY,
          package_id TEXT NOT NULL,
          source_id TEXT,
          namespace TEXT NOT NULL,
          plugin_id TEXT NOT NULL,
          plugin_name TEXT NOT NULL,
          version TEXT NOT NULL,
          target TEXT NOT NULL,
          scope TEXT NOT NULL,
          project_path TEXT,
          marketplace_id TEXT,
          marketplace_name TEXT NOT NULL,
          platform_ref TEXT NOT NULL,
          enabled INTEGER NOT NULL DEFAULT 1,
          install_mode TEXT NOT NULL,
          update_policy TEXT NOT NULL,
          status TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS local_plugins (
          id TEXT PRIMARY KEY,
          target TEXT NOT NULL,
          scope TEXT NOT NULL,
          project_path TEXT,
          path TEXT NOT NULL,
          marketplace_name TEXT,
          plugin_id TEXT,
          version TEXT,
          enabled INTEGER NOT NULL DEFAULT 1,
          status TEXT NOT NULL,
          component_inventory_json TEXT NOT NULL DEFAULT '{}',
          managed_by_skillhub INTEGER NOT NULL DEFAULT 0,
          scanned_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS projects (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          path TEXT NOT NULL UNIQUE,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS target_roots (
          target TEXT PRIMARY KEY,
          personal_path TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS local_skills (
          id TEXT PRIMARY KEY,
          target TEXT NOT NULL,
          level TEXT NOT NULL,
          project_path TEXT,
          path TEXT NOT NULL,
          detected_manifest TEXT,
          managed_by_skillhub INTEGER NOT NULL DEFAULT 0,
          status TEXT NOT NULL,
          enabled INTEGER NOT NULL DEFAULT 1,
          scanned_at TEXT NOT NULL,
          origin TEXT NOT NULL DEFAULT 'unknown',
          skill_id TEXT,
          version TEXT,
          summary TEXT,
          tags_json TEXT NOT NULL DEFAULT '[]',
          matched_source_id TEXT,
          matched_namespace TEXT,
          matched_skill_id TEXT,
          matched_version TEXT,
          can_import_to_cache INTEGER NOT NULL DEFAULT 0,
          can_restore_binding INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS audit_logs (
          id TEXT PRIMARY KEY,
          action TEXT NOT NULL,
          skill_ref TEXT,
          result TEXT NOT NULL,
          detail TEXT,
          created_at TEXT NOT NULL
        );
        "#,
    )?;

    add_column_if_missing(
        conn,
        "local_skills",
        "origin",
        "TEXT NOT NULL DEFAULT 'unknown'",
    )?;
    add_column_if_missing(
        conn,
        "local_skills",
        "enabled",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    add_column_if_missing(conn, "local_skills", "skill_id", "TEXT")?;
    add_column_if_missing(conn, "local_skills", "version", "TEXT")?;
    add_column_if_missing(conn, "local_skills", "summary", "TEXT")?;
    add_column_if_missing(
        conn,
        "local_skills",
        "tags_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    add_column_if_missing(conn, "local_skills", "matched_source_id", "TEXT")?;
    add_column_if_missing(conn, "local_skills", "matched_namespace", "TEXT")?;
    add_column_if_missing(conn, "local_skills", "matched_skill_id", "TEXT")?;
    add_column_if_missing(conn, "local_skills", "matched_version", "TEXT")?;
    add_column_if_missing(
        conn,
        "local_skills",
        "can_import_to_cache",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "local_skills",
        "can_restore_binding",
        "INTEGER NOT NULL DEFAULT 0",
    )?;

    Ok(())
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    if columns.iter().any(|item| item == column) {
        return Ok(());
    }

    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

pub fn enforce_compiled_source(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM sources WHERE id <> ?1",
        params![COMPILED_SOURCE_ID],
    )?;
    conn.execute(
        "DELETE FROM catalog_cache WHERE source_id <> ?1",
        params![COMPILED_SOURCE_ID],
    )?;
    conn.execute(
        "DELETE FROM plugin_catalog_cache WHERE source_id <> ?1",
        params![COMPILED_SOURCE_ID],
    )?;
    conn.execute(
        "INSERT INTO sources (id, name, endpoint, bucket, region, enabled, last_sync_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, NULL)
         ON CONFLICT(id) DO UPDATE SET
           name = excluded.name,
           endpoint = excluded.endpoint,
           bucket = excluded.bucket,
           region = excluded.region,
           enabled = 1",
        params![
            COMPILED_SOURCE_ID,
            COMPILED_SOURCE_NAME,
            COMPILED_SOURCE_ENDPOINT,
            COMPILED_SOURCE_BUCKET,
            COMPILED_SOURCE_REGION
        ],
    )?;
    Ok(())
}

pub fn market_project_cache_path(app_dir: &Path) -> PathBuf {
    app_dir
        .join("cache")
        .join("catalog")
        .join("projects.v1.json")
}

fn seed_if_empty(conn: &Connection) -> Result<()> {
    let root_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM target_roots", [], |row| row.get(0))?;
    if root_count == 0 {
        let now = now();
        for (target, path) in default_target_roots() {
            conn.execute(
                "INSERT INTO target_roots (target, personal_path, updated_at) VALUES (?1, ?2, ?3)",
                params![target, path, now],
            )?;
        }
    }

    Ok(())
}

fn remove_legacy_sample_skills(conn: &Connection) -> Result<()> {
    for (namespace, skill_id, name) in [
        ("official", "frontend-reviewer", "前端审查员"),
        ("official", "api-contract-writer", "接口契约助手"),
        ("community", "prd-shaper", "PRD 打磨器"),
    ] {
        conn.execute(
            "DELETE FROM catalog_cache
             WHERE source_id = ?1
               AND namespace = ?2
               AND skill_id = ?3
               AND name = ?4",
            params![COMPILED_SOURCE_ID, namespace, skill_id, name],
        )?;
    }

    Ok(())
}

pub fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn app_bootstrap(
    state: &AppState,
    metadata_sync_error: Option<String>,
) -> Result<AppBootstrap> {
    let conn = state.conn.lock().expect("db mutex poisoned");
    let sources = list_sources_inner(&conn)?;
    let categories = list_categories_inner(&conn)?;
    let bindings = list_bindings_inner(&conn)?;
    let projects = list_projects_inner(&conn)?;
    let market_projects = list_market_projects_cached(&state.app_dir)?;
    let target_roots = list_target_roots_inner(&conn)?;
    let mut skills = list_market_skills_inner(&conn)?;
    let mut plugins = list_market_plugins_inner(&conn)?;
    let cached_packages = list_cached_packages_inner(&conn)?;
    let plugin_packages = list_cached_plugin_packages_inner(&conn)?;
    let plugin_bindings = list_plugin_bindings_inner(&conn)?;
    let local_plugins = list_local_plugins_inner(&conn)?;
    let local_skills = list_local_skills_inner(&conn)?;

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

    for plugin in &mut plugins {
        plugin.installed_bindings = plugin_bindings
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

    let updates = list_update_candidates_inner(&conn)?;

    Ok(AppBootstrap {
        sources,
        categories,
        skills,
        plugins,
        market_projects,
        bindings,
        cached_packages,
        plugin_packages,
        plugin_bindings,
        local_plugins,
        local_skills,
        projects,
        target_roots,
        updates,
        metadata_sync_error,
    })
}

pub fn list_market_projects_cached(app_dir: &Path) -> Result<Vec<MarketProject>> {
    let path = market_project_cache_path(app_dir);
    if !path.is_file() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(path)?;
    let doc: crate::models::ProjectsDoc = serde_json::from_str(&raw)?;
    Ok(doc.into_projects())
}

pub fn list_sources_inner(conn: &Connection) -> Result<Vec<Source>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, endpoint, bucket, region, enabled, last_sync_at
         FROM sources
         ORDER BY name ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(Source {
            id: row.get(0)?,
            name: row.get(1)?,
            endpoint: row.get(2)?,
            bucket: row.get(3)?,
            region: row.get(4)?,
            enabled: row.get::<_, i64>(5)? == 1,
            last_sync_at: row.get(6)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_categories_inner(conn: &Connection) -> Result<Vec<Category>> {
    let mut stmt =
        conn.prepare("SELECT id, name, ordering FROM categories ORDER BY ordering ASC, name ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok(Category {
            id: row.get(0)?,
            name: row.get(1)?,
            order: row.get(2)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_market_skills_inner(conn: &Connection) -> Result<Vec<MarketSkill>> {
    let mut stmt = conn.prepare(
        "SELECT source_id, namespace, skill_id, latest_version, name, summary, categories_json,
                tags_json, targets_json, levels_json, manifest_path, updated_at
         FROM catalog_cache
         ORDER BY name ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        let categories_json: String = row.get(6)?;
        let tags_json: String = row.get(7)?;
        let targets_json: String = row.get(8)?;
        let levels_json: String = row.get(9)?;

        Ok(MarketSkill {
            source_id: Some(row.get(0)?),
            namespace: row.get(1)?,
            id: row.get(2)?,
            latest_version: row.get(3)?,
            name: row.get(4)?,
            summary: row.get(5)?,
            categories: serde_json::from_str(&categories_json).unwrap_or_default(),
            tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            targets: serde_json::from_str(&targets_json).unwrap_or_default(),
            levels: serde_json::from_str(&levels_json).unwrap_or_default(),
            manifest_path: row.get(10)?,
            updated_at: row.get(11)?,
            installed_bindings: Vec::new(),
            cached_versions: Vec::new(),
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_market_plugins_inner(conn: &Connection) -> Result<Vec<MarketPlugin>> {
    let mut stmt = conn.prepare(
        "SELECT source_id, namespace, plugin_id, latest_version, name, summary, categories_json,
                tags_json, targets_json, scopes_json, components_json, risk_level, manifest_path, updated_at
         FROM plugin_catalog_cache
         ORDER BY name ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        let categories_json: String = row.get(6)?;
        let tags_json: String = row.get(7)?;
        let targets_json: String = row.get(8)?;
        let scopes_json: String = row.get(9)?;
        let components_json: String = row.get(10)?;

        Ok(MarketPlugin {
            source_id: Some(row.get(0)?),
            namespace: row.get(1)?,
            id: row.get(2)?,
            latest_version: row.get(3)?,
            name: row.get(4)?,
            summary: row.get(5)?,
            categories: serde_json::from_str(&categories_json).unwrap_or_default(),
            tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            targets: serde_json::from_str(&targets_json).unwrap_or_default(),
            scopes: serde_json::from_str(&scopes_json).unwrap_or_default(),
            components: serde_json::from_str(&components_json).unwrap_or_default(),
            risk_level: row.get(11)?,
            manifest_path: row.get(12)?,
            updated_at: row.get(13)?,
            installed_bindings: Vec::new(),
            cached_versions: Vec::new(),
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_cached_versions_inner(
    conn: &Connection,
    source_id: Option<&str>,
    namespace: &str,
    skill_id: &str,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT version FROM skill_packages
         WHERE COALESCE(source_id, '') = COALESCE(?1, '')
           AND namespace = ?2
           AND skill_id = ?3
         ORDER BY cached_at DESC",
    )?;

    let rows = stmt.query_map(params![source_id, namespace, skill_id], |row| row.get(0))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_cached_plugin_versions_inner(
    conn: &Connection,
    source_id: Option<&str>,
    namespace: &str,
    plugin_id: &str,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT version FROM plugin_packages
         WHERE COALESCE(source_id, '') = COALESCE(?1, '')
           AND namespace = ?2
           AND plugin_id = ?3
         ORDER BY cached_at DESC",
    )?;

    let rows = stmt.query_map(params![source_id, namespace, plugin_id], |row| row.get(0))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_cached_packages_inner(conn: &Connection) -> Result<Vec<CachedSkillPackage>> {
    let mut stmt = conn.prepare(
        "SELECT
             package.source_id,
             package.namespace,
             package.skill_id,
             COALESCE(local_meta.name, catalog.name, binding.skill_name, package.skill_id) AS skill_name,
             package.version,
             package.package_path,
             package.cached_at,
             COUNT(binding.id) AS binding_count,
             CASE WHEN package.source_id = '__local__' THEN 'local' ELSE 'market' END AS origin,
             local_meta.summary,
             local_meta.tags_json,
             local_meta.source_path
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
          AND binding.status = 'installed'
         GROUP BY package.id
         ORDER BY package.cached_at DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(CachedSkillPackage {
            source_id: row.get(0)?,
            namespace: row.get(1)?,
            skill_id: row.get(2)?,
            skill_name: row.get(3)?,
            version: row.get(4)?,
            package_path: row.get(5)?,
            cached_at: row.get(6)?,
            binding_count: row.get(7)?,
            origin: row.get(8)?,
            summary: row.get(9)?,
            tags: row
                .get::<_, Option<String>>(10)?
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or_default(),
            source_path: row.get(11)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_cached_plugin_packages_inner(conn: &Connection) -> Result<Vec<CachedPluginPackage>> {
    let mut stmt = conn.prepare(
        "SELECT
             package.source_id,
             package.namespace,
             package.plugin_id,
             package.plugin_name,
             package.version,
             package.target,
             package.package_path,
             package.cached_at,
             package.risk_level,
             package.component_inventory_json,
             COUNT(binding.id) AS binding_count
         FROM plugin_packages package
         LEFT JOIN plugin_bindings binding
           ON binding.package_id = package.id
          AND binding.status = 'installed'
         GROUP BY package.id
         ORDER BY package.cached_at DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(CachedPluginPackage {
            source_id: row.get(0)?,
            namespace: row.get(1)?,
            plugin_id: row.get(2)?,
            plugin_name: row.get(3)?,
            version: row.get(4)?,
            target: row.get(5)?,
            package_path: row.get(6)?,
            cached_at: row.get(7)?,
            risk_level: row.get(8)?,
            component_inventory_json: row.get(9)?,
            binding_count: row.get(10)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_bindings_inner(conn: &Connection) -> Result<Vec<SkillBinding>> {
    let mut stmt = conn.prepare(
        "SELECT id, package_id, source_id, namespace, skill_id, skill_name, version, target, level,
                project_path, install_path, enabled, install_mode, update_policy, status,
                created_at, updated_at
         FROM skill_bindings
         ORDER BY updated_at DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(SkillBinding {
            id: row.get(0)?,
            package_id: row.get(1)?,
            source_id: row.get(2)?,
            namespace: row.get(3)?,
            skill_id: row.get(4)?,
            skill_name: row.get(5)?,
            version: row.get(6)?,
            target: row.get(7)?,
            level: row.get(8)?,
            project_path: row.get(9)?,
            install_path: row.get(10)?,
            enabled: row.get::<_, i64>(11)? == 1,
            install_mode: row.get(12)?,
            update_policy: row.get(13)?,
            status: row.get(14)?,
            created_at: row.get(15)?,
            updated_at: row.get(16)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_plugin_bindings_inner(conn: &Connection) -> Result<Vec<PluginBinding>> {
    let mut stmt = conn.prepare(
        "SELECT id, package_id, source_id, namespace, plugin_id, plugin_name, version, target, scope,
                project_path, marketplace_id, marketplace_name, platform_ref, enabled,
                install_mode, update_policy, status, created_at, updated_at
         FROM plugin_bindings
         ORDER BY updated_at DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(PluginBinding {
            id: row.get(0)?,
            package_id: row.get(1)?,
            source_id: row.get(2)?,
            namespace: row.get(3)?,
            plugin_id: row.get(4)?,
            plugin_name: row.get(5)?,
            version: row.get(6)?,
            target: row.get(7)?,
            scope: row.get(8)?,
            project_path: row.get(9)?,
            marketplace_id: row.get(10)?,
            marketplace_name: row.get(11)?,
            platform_ref: row.get(12)?,
            enabled: row.get::<_, i64>(13)? == 1,
            install_mode: row.get(14)?,
            update_policy: row.get(15)?,
            status: row.get(16)?,
            created_at: row.get(17)?,
            updated_at: row.get(18)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_projects_inner(conn: &Connection) -> Result<Vec<Project>> {
    let mut stmt =
        conn.prepare("SELECT id, name, path, created_at, updated_at FROM projects ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        Ok(Project {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_target_roots_inner(conn: &Connection) -> Result<Vec<TargetRoot>> {
    let mut stmt = conn.prepare(
        "SELECT target, personal_path, updated_at FROM target_roots ORDER BY target ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(TargetRoot {
            target: row.get(0)?,
            personal_path: row.get(1)?,
            updated_at: row.get(2)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn default_target_roots() -> Vec<(String, String)> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    vec![
        (
            "codex".to_string(),
            canonical_display_path(&PathBuf::from(&home).join(".codex").join("skills")),
        ),
        (
            "claude".to_string(),
            canonical_display_path(&PathBuf::from(&home).join(".claude").join("skills")),
        ),
    ]
}

pub fn list_local_plugins_inner(conn: &Connection) -> Result<Vec<LocalPlugin>> {
    let mut stmt = conn.prepare(
        "SELECT id, target, scope, project_path, path, marketplace_name, plugin_id, version,
                enabled, status, component_inventory_json, managed_by_skillhub, scanned_at
         FROM local_plugins
         ORDER BY scanned_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(LocalPlugin {
            id: row.get(0)?,
            target: row.get(1)?,
            scope: row.get(2)?,
            project_path: row.get(3)?,
            path: row.get(4)?,
            marketplace_name: row.get(5)?,
            plugin_id: row.get(6)?,
            version: row.get(7)?,
            enabled: row.get::<_, i64>(8)? == 1,
            status: row.get(9)?,
            component_inventory_json: row.get(10)?,
            managed_by_skillhub: row.get::<_, i64>(11)? == 1,
            scanned_at: row.get(12)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_local_skills_inner(conn: &Connection) -> Result<Vec<LocalSkill>> {
    let mut stmt = conn.prepare(
        "SELECT id, target, level, project_path, path, detected_manifest, managed_by_skillhub,
                status, enabled, scanned_at, origin, skill_id, version, summary, tags_json,
                matched_source_id, matched_namespace, matched_skill_id, matched_version,
                can_import_to_cache, can_restore_binding
         FROM local_skills
         ORDER BY scanned_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(LocalSkill {
            id: row.get(0)?,
            target: row.get(1)?,
            level: row.get(2)?,
            project_path: row.get(3)?,
            path: row.get(4)?,
            detected_manifest: row.get(5)?,
            managed_by_skillhub: row.get::<_, i64>(6)? == 1,
            status: row.get(7)?,
            enabled: row.get::<_, i64>(8)? == 1,
            scanned_at: row.get(9)?,
            origin: row.get(10)?,
            skill_id: row.get(11)?,
            version: row.get(12)?,
            summary: row.get(13)?,
            tags: row
                .get::<_, String>(14)
                .ok()
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or_default(),
            matched_source_id: row.get(15)?,
            matched_namespace: row.get(16)?,
            matched_skill_id: row.get(17)?,
            matched_version: row.get(18)?,
            can_import_to_cache: row.get::<_, i64>(19)? == 1,
            can_restore_binding: row.get::<_, i64>(20)? == 1,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_update_candidates_inner(conn: &Connection) -> Result<Vec<UpdateCandidate>> {
    let bindings = list_bindings_inner(conn)?;
    let market = list_market_skills_inner(conn)?;
    let plugin_bindings = list_plugin_bindings_inner(conn)?;
    let plugin_market = list_market_plugins_inner(conn)?;
    let mut updates = Vec::new();

    for binding in bindings {
        let Some(skill) = market
            .iter()
            .find(|item| item.namespace == binding.namespace && item.id == binding.skill_id)
        else {
            continue;
        };

        if binding.version == skill.latest_version {
            continue;
        }

        updates.push(UpdateCandidate {
            kind: "skill".to_string(),
            binding_id: binding.id.clone(),
            namespace: binding.namespace.clone(),
            skill_id: binding.skill_id.clone(),
            skill_name: binding.skill_name.clone(),
            target: binding.target.clone(),
            level: binding.level.clone(),
            project_path: binding.project_path.clone(),
            current_version: binding.version.clone(),
            latest_version: skill.latest_version.clone(),
            update_policy: binding.update_policy.clone(),
            blocked_reason: if binding.update_policy == "pinned" {
                Some("版本已锁定".to_string())
            } else {
                None
            },
        });
    }

    for binding in plugin_bindings {
        let Some(plugin) = plugin_market
            .iter()
            .find(|item| item.namespace == binding.namespace && item.id == binding.plugin_id)
        else {
            continue;
        };

        if binding.version == plugin.latest_version {
            continue;
        }

        updates.push(UpdateCandidate {
            kind: "plugin".to_string(),
            binding_id: binding.id.clone(),
            namespace: binding.namespace.clone(),
            skill_id: binding.plugin_id.clone(),
            skill_name: binding.plugin_name.clone(),
            target: binding.target.clone(),
            level: binding.scope.clone(),
            project_path: binding.project_path.clone(),
            current_version: binding.version.clone(),
            latest_version: plugin.latest_version.clone(),
            update_policy: binding.update_policy.clone(),
            blocked_reason: if binding.update_policy == "pinned" {
                Some("版本已锁定".to_string())
            } else {
                None
            },
        });
    }

    Ok(updates)
}

pub fn insert_audit(
    conn: &Connection,
    action: &str,
    skill_ref: Option<&str>,
    result: &str,
    detail: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO audit_logs (id, action, skill_ref, result, detail, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![new_id(), action, skill_ref, result, detail, now()],
    )?;
    Ok(())
}

pub fn canonical_display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
