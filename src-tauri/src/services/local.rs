use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use rusqlite::params;

use crate::{
    db::{
        canonical_display_path, list_bindings_inner, list_local_skills_inner,
        list_market_skills_inner, list_plugin_bindings_inner, list_projects_inner,
        list_target_roots_inner, new_id, now, AppState, LOCAL_SOURCE_ID,
    },
    models::{CachedSkillPackage, LocalPlugin, LocalSkill, MarketSkill, PluginBinding},
};

use super::{admin, install, object_store};

pub(crate) const LOCAL_NAMESPACE: &str = "local";
pub(crate) const LOCAL_DEFAULT_VERSION: &str = "0.0.0-local";
pub(crate) const DISABLED_SKILLS_DIR: &str = ".skill-hub-disabled";

pub(crate) fn scan_local_skills(state: &AppState) -> Result<Vec<LocalSkill>> {
    let conn = state.conn.lock().expect("db mutex poisoned");
    conn.execute("DELETE FROM local_skills", [])?;

    let bindings = list_bindings_inner(&conn)?;
    let market_skills = list_market_skills_inner(&conn)?;
    let target_roots = list_target_roots_inner(&conn)?;
    let projects = list_projects_inner(&conn)?;
    let cached_local_index = list_cached_local_index(&conn)?;
    let scanned_at = now();
    let mut seen_paths = HashSet::new();

    for binding in &bindings {
        let exists = PathBuf::from(&binding.install_path).exists();
        seen_paths.insert(canonical_display_path(Path::new(&binding.install_path)));
        conn.execute(
            "INSERT INTO local_skills
         (id, target, level, project_path, path, detected_manifest, managed_by_skillhub,
          status, enabled, scanned_at, origin, skill_id, version, summary, tags_json,
          matched_source_id, matched_namespace, matched_skill_id, matched_version,
          can_import_to_cache, can_restore_binding)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9, 'managed', ?10, ?11, NULL, '[]',
                 ?12, ?13, ?14, ?15, 0, 0)",
            params![
                new_id(),
                binding.target,
                binding.level,
                binding.project_path,
                binding.install_path,
                format!("{}@{}", binding.skill_name, binding.version),
                if exists { "managed" } else { "missing" },
                if binding.enabled { 1_i64 } else { 0_i64 },
                scanned_at,
                binding.skill_id,
                binding.version,
                binding.source_id,
                binding.namespace,
                binding.skill_id,
                binding.version
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
            &market_skills,
            &cached_local_index,
            true,
        )?;
        scan_disabled_skill_root(
            &conn,
            &mut seen_paths,
            &root.target,
            "personal",
            None,
            Path::new(&root.personal_path),
            &scanned_at,
            &market_skills,
            &cached_local_index,
        )?;
    }

    for project in projects {
        for target in ["codex", "claude"] {
            let root = install::resolve_project_skill_root(target, Path::new(&project.path));
            scan_skill_root(
                &conn,
                &mut seen_paths,
                target,
                "project",
                Some(project.path.as_str()),
                &root,
                &scanned_at,
                &market_skills,
                &cached_local_index,
                true,
            )?;
            scan_disabled_skill_root(
                &conn,
                &mut seen_paths,
                target,
                "project",
                Some(project.path.as_str()),
                &root,
                &scanned_at,
                &market_skills,
                &cached_local_index,
            )?;
        }
    }

    list_local_skills_inner(&conn)
}

pub(crate) fn scan_local_plugins(state: &AppState) -> Result<Vec<LocalPlugin>> {
    scan_local_plugins_inner(state)
}
fn scan_local_plugins_inner(state: &AppState) -> Result<Vec<LocalPlugin>> {
    let conn = state.conn.lock().expect("db mutex poisoned");
    conn.execute("DELETE FROM local_plugins", [])?;
    let scanned_at = now();
    let mut seen_paths = HashSet::new();
    let mut seen_plugin_keys = HashSet::new();

    for binding in list_plugin_bindings_inner(&conn)? {
        insert_local_plugin_from_binding(
            state,
            &conn,
            &binding,
            &scanned_at,
            &mut seen_paths,
            &mut seen_plugin_keys,
        )?;
    }

    for (target, scope, project_path, root) in plugin_scan_roots(state, &conn)? {
        scan_plugin_marketplace_root(
            &conn,
            &mut seen_paths,
            &mut seen_plugin_keys,
            &target,
            &scope,
            project_path.as_deref(),
            &root,
            &scanned_at,
        )?;
    }

    crate::db::list_local_plugins_inner(&conn)
}

fn plugin_scan_roots(
    state: &AppState,
    conn: &rusqlite::Connection,
) -> Result<Vec<(String, String, Option<String>, PathBuf)>> {
    let mut roots = Vec::new();
    // Skill Hub's own Claude user marketplace lives under app_dir and is represented by bindings.
    roots.push((
        "codex".to_string(),
        "user".to_string(),
        None,
        install::plugin_marketplace_root(state, "codex", "user", None)?,
    ));

    if let Some(home) = home_dir_path() {
        roots.push((
            "claude".to_string(),
            "user".to_string(),
            None,
            home.join(".claude-plugin"),
        ));
    }

    for project in list_projects_inner(conn)? {
        roots.push((
            "codex".to_string(),
            "project".to_string(),
            Some(project.path.clone()),
            install::plugin_marketplace_root(state, "codex", "project", Some(&project.path))?,
        ));
        roots.push((
            "claude".to_string(),
            "project".to_string(),
            Some(project.path.clone()),
            install::plugin_marketplace_root(state, "claude", "project", Some(&project.path))?,
        ));
    }

    Ok(roots)
}

fn insert_local_plugin_from_binding(
    state: &AppState,
    conn: &rusqlite::Connection,
    binding: &PluginBinding,
    scanned_at: &str,
    seen_paths: &mut HashSet<String>,
    seen_plugin_keys: &mut HashSet<String>,
) -> Result<()> {
    let (_, component_inventory_json) =
        plugin_package_path_and_inventory(conn, &binding.package_id).unwrap_or_else(|| {
            (
                binding.platform_ref.clone(),
                serde_json::json!({ "schema": "skillhub.plugin-component-inventory.v1" })
                    .to_string(),
            )
        });
    let path = install::plugin_marketplace_root(
        state,
        &binding.target,
        &binding.scope,
        binding.project_path.as_deref(),
    )?
    .join("plugins")
    .join(format!("{}.{}", binding.namespace, binding.plugin_id));
    let path = canonical_display_path(&path);
    seen_paths.insert(canonical_display_path(Path::new(&path)));
    let exists = Path::new(&path).exists();
    conn.execute(
        "INSERT INTO local_plugins
         (id, target, scope, project_path, path, marketplace_name, plugin_id, version,
          enabled, status, component_inventory_json, managed_by_skillhub, scanned_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12)",
        params![
            new_id(),
            binding.target,
            binding.scope,
            binding.project_path,
            path,
            binding.marketplace_name,
            binding.plugin_id,
            binding.version,
            if binding.enabled { 1_i64 } else { 0_i64 },
            if exists {
                binding.status.as_str()
            } else {
                "missing"
            },
            component_inventory_json,
            scanned_at
        ],
    )?;
    if !binding.plugin_id.is_empty() {
        seen_plugin_keys.insert(local_plugin_identity_key(
            &binding.target,
            &binding.scope,
            binding.project_path.as_deref(),
            &binding.plugin_id,
        ));
    }
    Ok(())
}

fn plugin_package_path_and_inventory(
    conn: &rusqlite::Connection,
    package_id: &str,
) -> Option<(String, String)> {
    conn.query_row(
        "SELECT package_path, component_inventory_json FROM plugin_packages WHERE id = ?1",
        params![package_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )
    .ok()
}

fn scan_plugin_marketplace_root(
    conn: &rusqlite::Connection,
    seen_paths: &mut HashSet<String>,
    seen_plugin_keys: &mut HashSet<String>,
    target: &str,
    scope: &str,
    project_path: Option<&str>,
    root: &Path,
    scanned_at: &str,
) -> Result<()> {
    let marketplace_path = plugin_marketplace_path(target, root);
    if let Some(doc) = install::read_json_file(&marketplace_path) {
        let marketplace_name = doc
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("external")
            .to_string();
        if let Some(plugins) = doc.get("plugins").and_then(|value| value.as_array()) {
            for entry in plugins {
                let Some(path) = entry
                    .get("source")
                    .and_then(|source| source.get("path"))
                    .and_then(|value| value.as_str())
                else {
                    continue;
                };
                let plugin_path = resolve_plugin_source_path(target, root, path);
                insert_local_plugin_from_path(
                    conn,
                    seen_paths,
                    seen_plugin_keys,
                    target,
                    scope,
                    project_path,
                    &plugin_path,
                    Some(&marketplace_name),
                    entry.get("name").and_then(|value| value.as_str()),
                    entry.get("version").and_then(|value| value.as_str()),
                    scanned_at,
                    true,
                )?;
            }
        }
    }

    let plugins_root = root.join("plugins");
    if plugins_root.is_dir() {
        for entry in fs::read_dir(plugins_root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                insert_local_plugin_from_path(
                    conn,
                    seen_paths,
                    seen_plugin_keys,
                    target,
                    scope,
                    project_path,
                    &path,
                    None,
                    None,
                    None,
                    scanned_at,
                    true,
                )?;
            }
        }
    }

    Ok(())
}

fn insert_local_plugin_from_path(
    conn: &rusqlite::Connection,
    seen_paths: &mut HashSet<String>,
    seen_plugin_keys: &mut HashSet<String>,
    target: &str,
    scope: &str,
    project_path: Option<&str>,
    path: &Path,
    marketplace_name: Option<&str>,
    entry_plugin_id: Option<&str>,
    entry_version: Option<&str>,
    scanned_at: &str,
    enabled: bool,
) -> Result<()> {
    let display_path = canonical_display_path(path);
    if !seen_paths.insert(display_path.clone()) {
        return Ok(());
    }

    let profile = read_local_plugin_profile(target, path);
    let status = if path.exists() {
        if profile.is_some() {
            "unmanaged"
        } else {
            "invalid"
        }
    } else {
        "missing"
    };
    let plugin_id = profile
        .as_ref()
        .and_then(|profile| profile.plugin_id.clone())
        .or_else(|| entry_plugin_id.map(ToString::to_string));
    if let Some(plugin_id) = plugin_id.as_deref() {
        let key = local_plugin_identity_key(target, scope, project_path, plugin_id);
        if seen_plugin_keys.contains(&key) {
            return Ok(());
        }
    }
    let version = profile
        .as_ref()
        .and_then(|profile| profile.version.clone())
        .or_else(|| entry_version.map(ToString::to_string));
    let component_inventory_json = profile
        .as_ref()
        .map(|profile| profile.component_inventory_json.clone())
        .unwrap_or_else(|| "{}".to_string());

    conn.execute(
        "INSERT INTO local_plugins
         (id, target, scope, project_path, path, marketplace_name, plugin_id, version,
          enabled, status, component_inventory_json, managed_by_skillhub, scanned_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, ?12)",
        params![
            new_id(),
            target,
            scope,
            project_path,
            display_path,
            marketplace_name,
            plugin_id,
            version,
            if enabled { 1_i64 } else { 0_i64 },
            status,
            component_inventory_json,
            scanned_at
        ],
    )?;
    if let Some(plugin_id) = plugin_id.as_deref() {
        seen_plugin_keys.insert(local_plugin_identity_key(
            target,
            scope,
            project_path,
            plugin_id,
        ));
    }
    Ok(())
}

fn local_plugin_identity_key(
    target: &str,
    scope: &str,
    project_path: Option<&str>,
    plugin_id: &str,
) -> String {
    format!(
        "{target}|{scope}|{}|{plugin_id}",
        project_path.unwrap_or_default()
    )
}

#[derive(Debug, Clone)]
struct LocalPluginProfile {
    plugin_id: Option<String>,
    pub(crate) version: Option<String>,
    component_inventory_json: String,
}

fn read_local_plugin_profile(target: &str, path: &Path) -> Option<LocalPluginProfile> {
    let manifest_path = match target {
        "codex" => path.join(".codex-plugin").join("plugin.json"),
        "claude" => path.join(".claude-plugin").join("plugin.json"),
        _ => return None,
    };
    let manifest = install::read_json_file(&manifest_path)?;
    let plugin_id = manifest
        .get("name")
        .or_else(|| manifest.get("id"))
        .or_else(|| manifest.get("pluginId"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    let version = manifest
        .get("version")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    Some(LocalPluginProfile {
        plugin_id,
        version,
        component_inventory_json: local_plugin_component_inventory_json(target, path, &manifest),
    })
}

fn local_plugin_component_inventory_json(
    target: &str,
    path: &Path,
    manifest: &serde_json::Value,
) -> String {
    let target_inventory = serde_json::json!({
        "manifest": manifest,
        "skills": child_dir_names(path.join("skills")),
        "commands": child_file_names(path.join("commands")),
        "agents": child_file_names(path.join("agents")),
        "hooks": child_file_names(path.join("hooks")),
        "mcpServers": path.join(".mcp.json").exists(),
        "lspServers": path.join(".lsp.json").exists(),
        "monitors": child_file_names(path.join("monitors")),
        "bin": child_file_names(path.join("bin")),
        "assets": child_file_names(path.join("assets")),
        "settings": path.join("settings.json").exists(),
        "apps": path.join(".app.json").exists()
    });
    let mut targets = serde_json::Map::new();
    targets.insert(target.to_string(), target_inventory);
    let inventory = serde_json::json!({
        "schema": "skillhub.plugin-component-inventory.v1",
        "targets": targets
    });
    serde_json::to_string(&inventory).unwrap_or_else(|_| "{}".to_string())
}

fn child_dir_names(path: PathBuf) -> Vec<String> {
    child_names(path, true)
}

fn child_file_names(path: PathBuf) -> Vec<String> {
    child_names(path, false)
}

fn child_names(path: PathBuf, dirs: bool) -> Vec<String> {
    let mut items = fs::read_dir(path)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if file_type.is_dir() != dirs {
                return None;
            }
            entry.file_name().to_str().map(ToString::to_string)
        })
        .collect::<Vec<_>>();
    items.sort();
    items
}

pub(crate) fn plugin_marketplace_path(target: &str, root: &Path) -> PathBuf {
    match target {
        "codex" if !is_codex_marketplace_dir(root) => root
            .join(".agents")
            .join("plugins")
            .join("marketplace.json"),
        "claude" if root.file_name().and_then(|name| name.to_str()) != Some(".claude-plugin") => {
            root.join(".claude-plugin").join("marketplace.json")
        }
        _ => root.join("marketplace.json"),
    }
}

fn is_codex_marketplace_dir(root: &Path) -> bool {
    root.file_name().and_then(|name| name.to_str()) == Some("plugins")
        && root
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            == Some(".agents")
}

pub(crate) fn resolve_plugin_source_path(target: &str, root: &Path, source_path: &str) -> PathBuf {
    let raw = PathBuf::from(source_path);
    if raw.is_absolute() {
        return raw;
    }
    if target == "claude" {
        if root.file_name().and_then(|name| name.to_str()) == Some(".claude-plugin") {
            return root.parent().unwrap_or(root).join(raw);
        }
        return root.join(raw);
    }
    root.join(raw)
}

fn home_dir_path() -> Option<PathBuf> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(PathBuf::from)
}

pub(crate) fn path_hash(value: &str) -> String {
    object_store::sha256_hex(value.as_bytes())[..16].to_string()
}

fn scan_skill_root(
    conn: &rusqlite::Connection,
    seen_paths: &mut HashSet<String>,
    target: &str,
    level: &str,
    project_path: Option<&str>,
    root: &Path,
    scanned_at: &str,
    market_skills: &[MarketSkill],
    cached_local_index: &CachedLocalSkillIndex,
    enabled: bool,
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
        if enabled
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == DISABLED_SKILLS_DIR)
        {
            continue;
        }

        let display_path = canonical_display_path(&path);
        if !seen_paths.insert(display_path.clone()) {
            continue;
        }

        let Some(profile) = local_skill_profile_from_path(&path)? else {
            continue;
        };
        let mut classification = classify_local_skill(&profile, market_skills);
        if cached_local_index.contains(&display_path, &profile) {
            classification.origin = "local".to_string();
            classification.status = "cached".to_string();
            classification.can_import_to_cache = false;
        }

        conn.execute(
            "INSERT INTO local_skills
             (id, target, level, project_path, path, detected_manifest, managed_by_skillhub,
              status, enabled, scanned_at, origin, skill_id, version, summary, tags_json,
              matched_source_id, matched_namespace, matched_skill_id, matched_version,
              can_import_to_cache, can_restore_binding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                new_id(),
                target,
                level,
                project_path,
                display_path,
                profile.name,
                if enabled { classification.status } else { "disabled".to_string() },
                if enabled { 1_i64 } else { 0_i64 },
                scanned_at,
                classification.origin,
                profile.skill_id,
                profile.version,
                profile.summary,
                serde_json::to_string(&profile.tags)?,
                classification.matched_source_id,
                classification.matched_namespace,
                classification.matched_skill_id,
                classification.matched_version,
                if classification.can_import_to_cache { 1_i64 } else { 0_i64 },
                if classification.can_restore_binding { 1_i64 } else { 0_i64 }
            ],
        )?;
    }

    Ok(())
}

fn scan_disabled_skill_root(
    conn: &rusqlite::Connection,
    seen_paths: &mut HashSet<String>,
    target: &str,
    level: &str,
    project_path: Option<&str>,
    root: &Path,
    scanned_at: &str,
    market_skills: &[MarketSkill],
    cached_local_index: &CachedLocalSkillIndex,
) -> Result<()> {
    let disabled_root = root.join(DISABLED_SKILLS_DIR);
    scan_skill_root(
        conn,
        seen_paths,
        target,
        level,
        project_path,
        &disabled_root,
        scanned_at,
        market_skills,
        cached_local_index,
        false,
    )
}

pub(crate) fn detect_local_skill_label(path: &Path) -> Option<String> {
    let skill_md = path.join("SKILL.md");
    if !skill_md.is_file() {
        return None;
    }

    skill_name_from_dir(path)
        .or_else(|| read_skill_markdown_name(&skill_md))
        .or_else(|| read_skill_markdown_title(&skill_md))
        .or_else(|| Some("local-skill".to_string()))
}

#[derive(Debug, Clone, Default)]
struct CachedLocalSkillIndex {
    source_paths: HashSet<String>,
    fingerprints: HashSet<String>,
}

impl CachedLocalSkillIndex {
    fn contains(&self, display_path: &str, profile: &LocalSkillProfile) -> bool {
        self.source_paths.contains(display_path)
            || self
                .fingerprints
                .contains(&local_skill_fingerprint(profile))
    }
}

fn list_cached_local_index(conn: &rusqlite::Connection) -> Result<CachedLocalSkillIndex> {
    let mut stmt = conn.prepare(
        "SELECT package.skill_id, package.version, local_meta.source_path
         FROM skill_packages package
         LEFT JOIN local_package_metadata local_meta
           ON local_meta.package_id = package.id
         WHERE package.source_id = ?1
           AND package.namespace = ?2",
    )?;
    let rows = stmt.query_map(params![LOCAL_SOURCE_ID, LOCAL_NAMESPACE], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;

    let mut index = CachedLocalSkillIndex::default();
    for row in rows {
        let (skill_id, version, source_path) = row?;
        index
            .fingerprints
            .insert(normalized_local_skill_fingerprint(&skill_id, &version));
        if let Some(source_path) = source_path
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty())
        {
            index
                .source_paths
                .insert(canonical_display_path(Path::new(&source_path)));
        }
    }
    Ok(index)
}

#[derive(Debug, Clone)]
pub(crate) struct LocalSkillProfile {
    pub(crate) name: String,
    pub(crate) summary: String,
    pub(crate) skill_id: String,
    pub(crate) version: String,
    pub(crate) author: Option<String>,
    pub(crate) tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct LocalSkillClassification {
    pub(crate) origin: String,
    pub(crate) status: String,
    pub(crate) matched_source_id: Option<String>,
    pub(crate) matched_namespace: Option<String>,
    pub(crate) matched_skill_id: Option<String>,
    pub(crate) matched_version: Option<String>,
    pub(crate) can_import_to_cache: bool,
    pub(crate) can_restore_binding: bool,
}

fn local_skill_profile_from_path(path: &Path) -> Result<Option<LocalSkillProfile>> {
    let skill_md = path.join("SKILL.md");
    if !skill_md.is_file() {
        return Ok(None);
    }
    read_local_skill_profile(path).map(Some)
}

pub(crate) fn read_local_skill_profile(path: &Path) -> Result<LocalSkillProfile> {
    let skill_md = path.join("SKILL.md");
    let content = fs::read_to_string(&skill_md).context("读取本地 SKILL.md 失败")?;
    let metadata = admin::parse_skill_frontmatter(&content);
    let dir_name = skill_name_from_dir(path).unwrap_or_else(|| "local-skill".to_string());
    let title = parse_skill_markdown_title(&content);
    let name = metadata
        .name
        .clone()
        .or(title)
        .unwrap_or_else(|| dir_name.clone());
    let summary = metadata
        .description
        .clone()
        .or_else(|| first_markdown_paragraph(&content))
        .unwrap_or_default();
    let skill_id = slugify_skill_id(&dir_name).if_empty_then(|| slugify_skill_id(&name));
    let skill_id = if skill_id.is_empty() {
        "local-skill".to_string()
    } else {
        skill_id
    };

    Ok(LocalSkillProfile {
        name,
        summary,
        skill_id,
        version: metadata
            .version
            .unwrap_or_else(|| LOCAL_DEFAULT_VERSION.to_string()),
        author: metadata.author,
        tags: metadata.tags,
    })
}

fn local_skill_fingerprint(profile: &LocalSkillProfile) -> String {
    normalized_local_skill_fingerprint(&profile.skill_id, &profile.version)
}

fn normalized_local_skill_fingerprint(skill_id: &str, version: &str) -> String {
    let skill_id = slugify_skill_id(skill_id);
    let version = version.trim();
    let version = if version.is_empty() {
        LOCAL_DEFAULT_VERSION
    } else {
        version
    };
    format!("{skill_id}@{version}")
}

pub(crate) fn cached_package_matches_local_profile(
    package: &CachedSkillPackage,
    profile: &LocalSkillProfile,
) -> bool {
    package.source_id.as_deref() == Some(LOCAL_SOURCE_ID)
        && package.namespace == LOCAL_NAMESPACE
        && normalized_local_skill_fingerprint(&package.skill_id, &package.version)
            == local_skill_fingerprint(profile)
}

trait EmptyStringFallback {
    fn if_empty_then<F>(self, fallback: F) -> String
    where
        F: FnOnce() -> String;
}

impl EmptyStringFallback for String {
    fn if_empty_then<F>(self, fallback: F) -> String
    where
        F: FnOnce() -> String,
    {
        if self.is_empty() {
            fallback()
        } else {
            self
        }
    }
}

fn first_markdown_paragraph(content: &str) -> Option<String> {
    let mut in_frontmatter = content.lines().next().map(str::trim) == Some("---");
    let mut seen_frontmatter_end = !in_frontmatter;
    for line in content.lines().take(160) {
        let trimmed = line.trim();
        if in_frontmatter {
            if trimmed == "---" || trimmed == "..." {
                in_frontmatter = false;
                seen_frontmatter_end = true;
            }
            continue;
        }
        if !seen_frontmatter_end || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        return Some(trimmed.to_string());
    }
    None
}

pub(crate) fn slugify_skill_id(value: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            output.push('-');
            last_dash = true;
        }
    }
    output.trim_matches('-').to_string()
}

pub(crate) fn classify_local_skill(
    profile: &LocalSkillProfile,
    market_skills: &[MarketSkill],
) -> LocalSkillClassification {
    if let Some(skill) = market_skills.iter().find(|skill| {
        skill.id.eq_ignore_ascii_case(&profile.skill_id) && skill.latest_version == profile.version
    }) {
        return LocalSkillClassification {
            origin: "market".to_string(),
            status: "market".to_string(),
            matched_source_id: skill.source_id.clone(),
            matched_namespace: Some(skill.namespace.clone()),
            matched_skill_id: Some(skill.id.clone()),
            matched_version: Some(skill.latest_version.clone()),
            can_import_to_cache: false,
            can_restore_binding: true,
        };
    }

    if let Some(skill) = market_skills.iter().find(|skill| {
        skill.id.eq_ignore_ascii_case(&profile.skill_id)
            || skill.name.eq_ignore_ascii_case(&profile.name)
    }) {
        return LocalSkillClassification {
            origin: "unknown".to_string(),
            status: "possible_market".to_string(),
            matched_source_id: skill.source_id.clone(),
            matched_namespace: Some(skill.namespace.clone()),
            matched_skill_id: Some(skill.id.clone()),
            matched_version: Some(skill.latest_version.clone()),
            can_import_to_cache: true,
            can_restore_binding: true,
        };
    }

    LocalSkillClassification {
        origin: "local".to_string(),
        status: "local".to_string(),
        matched_source_id: None,
        matched_namespace: None,
        matched_skill_id: None,
        matched_version: None,
        can_import_to_cache: true,
        can_restore_binding: false,
    }
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

pub(crate) fn clean_frontmatter_value(value: &str) -> Option<String> {
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

pub(crate) fn display_skill_name_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("local-skill")
        .to_string()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{canonical_display_path, new_id, AppState},
        models::MarketSkill,
    };
    use std::fs;
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
    fn local_skill_profile_allows_minimal_skill_md() {
        let root = std::env::temp_dir().join(format!("skillhub-test-{}", new_id()));
        let skill_dir = root.join("Daily Note Helper");
        fs::create_dir_all(&skill_dir).expect("create temp skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"# Daily Note Helper

Capture a concise daily note.
"#,
        )
        .expect("write temp SKILL.md");

        let profile = read_local_skill_profile(&skill_dir).expect("parse local profile");
        assert_eq!(profile.name, "Daily Note Helper");
        assert_eq!(profile.skill_id, "daily-note-helper");
        assert_eq!(profile.version, LOCAL_DEFAULT_VERSION);
        assert_eq!(profile.summary, "Capture a concise daily note.");
        assert!(profile.tags.is_empty());

        fs::remove_dir_all(root).expect("remove temp skill dir");
    }

    #[test]
    fn local_skill_classification_requires_strong_market_match() {
        let market = vec![MarketSkill {
            namespace: "live".to_string(),
            id: "daily-note-helper".to_string(),
            name: "Daily Note Helper".to_string(),
            summary: String::new(),
            latest_version: "1.0.0".to_string(),
            categories: vec![],
            tags: vec![],
            targets: vec![],
            levels: vec![],
            manifest_path: "skills/live/daily-note-helper/manifest.json".to_string(),
            updated_at: None,
            source_id: Some("compiled-source".to_string()),
            installed_bindings: vec![],
            cached_versions: vec![],
        }];
        let profile = LocalSkillProfile {
            name: "Daily Note Helper".to_string(),
            summary: String::new(),
            skill_id: "daily-note-helper".to_string(),
            version: LOCAL_DEFAULT_VERSION.to_string(),
            author: None,
            tags: vec![],
        };

        let weak = classify_local_skill(&profile, &market);
        assert_eq!(weak.origin, "unknown");
        assert!(weak.can_import_to_cache);

        let strong = classify_local_skill(
            &LocalSkillProfile {
                version: "1.0.0".to_string(),
                ..profile
            },
            &market,
        );
        assert_eq!(strong.origin, "market");
        assert!(!strong.can_import_to_cache);
    }

    #[test]
    fn local_plugin_identity_key_groups_same_plugin_across_paths() {
        assert_eq!(
            local_plugin_identity_key("codex", "user", None, "commit-workflow"),
            local_plugin_identity_key("codex", "user", None, "commit-workflow")
        );
        assert_ne!(
            local_plugin_identity_key("codex", "user", None, "commit-workflow"),
            local_plugin_identity_key("claude", "user", None, "commit-workflow")
        );
        assert_ne!(
            local_plugin_identity_key("codex", "user", None, "commit-workflow"),
            local_plugin_identity_key(
                "codex",
                "project",
                Some(r"C:\Users\ctf19\project-a"),
                "commit-workflow"
            )
        );
    }

    #[test]
    fn plugin_scan_roots_skips_skillhub_managed_claude_user_marketplace_root() {
        let app_dir = std::env::temp_dir().join(format!("skillhub-scan-roots-{}", new_id()));
        let conn = rusqlite::Connection::open_in_memory().expect("open sqlite");
        conn.execute_batch(
            r#"
            CREATE TABLE projects (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              path TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            "#,
        )
        .expect("create projects table");
        let state = AppState {
            conn: std::sync::Arc::new(std::sync::Mutex::new(
                rusqlite::Connection::open_in_memory().expect("open state sqlite"),
            )),
            app_dir: app_dir.clone(),
            local_macs: vec![],
        };

        let roots = plugin_scan_roots(&state, &conn).expect("scan roots");
        let skillhub_claude_user_root = canonical_display_path(
            &app_dir
                .join("plugin-marketplaces")
                .join("claude")
                .join("user"),
        );

        assert!(
            roots
                .iter()
                .all(|(_, _, _, root)| canonical_display_path(root) != skillhub_claude_user_root),
            "Skill Hub's own Claude user marketplace root should be represented by bindings, not scanned as an external local plugin"
        );
    }
}
