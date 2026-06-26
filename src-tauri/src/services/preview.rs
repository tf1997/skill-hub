use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
use rusqlite::{params, OptionalExtension};

use crate::{
    commands,
    db::{
        canonical_display_path, list_market_plugins_inner, list_market_skills_inner,
        list_sources_inner, AppState,
    },
    models::{
        MarketPlugin, MarketSkill, PluginPreviewRequest, SkillPreview, SkillPreviewFile,
        SkillPreviewFileEntry, SkillPreviewRequest,
    },
    services::{local, validation},
};

pub(crate) const PREVIEW_MAX_FILES: usize = 8;
pub(crate) const PREVIEW_MAX_FILE_LIST: usize = 500;
pub(crate) const PREVIEW_MAX_BYTES: usize = 24 * 1024;

pub(crate) fn preview_candidate_paths(
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
        "readme.md",
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

pub(crate) async fn preview_skill_inner(
    request: SkillPreviewRequest,
    state: &AppState,
) -> Result<SkillPreview> {
    let should_refresh_market_metadata =
        request.binding_id.is_none() && request.path.is_none() && request.version.is_none();
    if should_refresh_market_metadata {
        let _metadata_sync_error = commands::refresh_catalog_best_effort(state).await;
    }
    let selected_path = normalize_preview_file_path(request.file_path.as_deref())?;

    let (title, origin, root_path) = if let Some(binding_id) = request.binding_id.as_deref() {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let binding = commands::find_binding(&conn, binding_id)?;
        (
            binding.skill_name,
            format!("{} / {}", binding.target, binding.level),
            PathBuf::from(binding.install_path),
        )
    } else if let Some(path) = request.path.as_deref() {
        let root_path = PathBuf::from(path);
        let title = local::detect_local_skill_label(&root_path)
            .unwrap_or_else(|| local::display_skill_name_from_path(&root_path));
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
                Some(source) => Some(
                    commands::fetch_manifest_version(source, &skill.manifest_path, &version)
                        .await?,
                ),
                _ => None,
            };
            let package_path = commands::prepare_package(
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

pub(crate) async fn preview_plugin_inner(
    request: PluginPreviewRequest,
    state: &AppState,
) -> Result<SkillPreview> {
    let should_refresh_market_metadata =
        request.binding_id.is_none() && request.path.is_none() && request.version.is_none();
    if should_refresh_market_metadata {
        let _metadata_sync_error = commands::refresh_catalog_best_effort(state).await;
    }
    let selected_path = normalize_preview_file_path(request.file_path.as_deref())?;

    let (title, origin, root_path) = if let Some(binding_id) = request.binding_id.as_deref() {
        let conn = state.conn.lock().expect("db mutex poisoned");
        let binding = commands::find_plugin_binding(&conn, binding_id)?;
        let package_path: String = conn
            .query_row(
                "SELECT package_path FROM plugin_packages WHERE id = ?1",
                params![binding.package_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| {
                anyhow!("PLUGIN_MARKETPLACE_WRITE_FAILED: plugin cache package not found")
            })?;
        (
            binding.plugin_name,
            format!("{} / {}", binding.target, binding.scope),
            PathBuf::from(package_path),
        )
    } else if let Some(path) = request.path.as_deref() {
        let root_path = PathBuf::from(path);
        (
            local::display_skill_name_from_path(&root_path),
            "local plugin".to_string(),
            root_path,
        )
    } else {
        let namespace = request
            .namespace
            .as_deref()
            .ok_or_else(|| anyhow!("PLUGIN_SOURCE_INVALID: missing namespace"))?;
        let plugin_id = request
            .plugin_id
            .as_deref()
            .ok_or_else(|| anyhow!("PLUGIN_SOURCE_INVALID: missing plugin id"))?;
        let target = request.target.as_deref().unwrap_or("codex");
        validation::validate_plugin_target(target)?;
        let requested_source_id = request.source_id.clone();
        let requested_version = request.version.clone();

        let (source_id, plugin, source) = {
            let conn = state.conn.lock().expect("db mutex poisoned");
            let source_id = requested_source_id.or_else(|| {
                default_source_for_plugin(&conn, namespace, plugin_id)
                    .ok()
                    .flatten()
            });
            let plugin = find_market_plugin(&conn, source_id.as_deref(), namespace, plugin_id)?;
            if !plugin.targets.iter().any(|item| item == target) {
                return Err(anyhow!("PLUGIN_TARGET_UNSUPPORTED: {target}"));
            }
            let source = source_id.as_deref().and_then(|id| {
                list_sources_inner(&conn)
                    .ok()?
                    .into_iter()
                    .find(|item| item.id == id)
            });
            (source_id, plugin, source)
        };
        let version = requested_version.unwrap_or_else(|| plugin.latest_version.clone());
        let version_info = match source.as_ref() {
            Some(source) => Some(
                commands::fetch_plugin_manifest_version(source, &plugin.manifest_path, &version)
                    .await?,
            ),
            _ => None,
        };
        let package_path = commands::prepare_plugin_package(
            state,
            source.as_ref(),
            &plugin,
            &version,
            target,
            version_info.as_ref(),
        )
        .await?;
        (
            plugin.name,
            format!(
                "{} / {}",
                source_id.unwrap_or_else(|| "local cache".to_string()),
                target
            ),
            package_path,
        )
    };

    if !root_path.exists() || !root_path.is_dir() {
        return Err(anyhow!(
            "PLUGIN_PACKAGE_BUILD_FAILED: preview directory missing"
        ));
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

pub(crate) fn collect_preview_file_list(root: &Path) -> Result<Vec<SkillPreviewFileEntry>> {
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
        let allow_hidden_plugin_manifest_dir =
            path.is_dir() && (name == ".codex-plugin" || name == ".claude-plugin");
        if (name.starts_with('.') && !allow_hidden_plugin_manifest_dir)
            || name == "node_modules"
            || name == "target"
        {
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

pub(crate) fn preview_file_entry(relative: &str) -> SkillPreviewFileEntry {
    SkillPreviewFileEntry {
        path: relative.to_string(),
        language: language_for_relative_path(relative),
        previewable: is_previewable_relative_path(relative),
    }
}

pub(crate) fn normalize_preview_file_path(value: Option<&str>) -> Result<Option<String>> {
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

pub(crate) fn preview_file_from_bytes(
    relative: &str,
    bytes: &[u8],
    max_bytes: usize,
) -> Option<SkillPreviewFile> {
    let truncated = bytes.len() > max_bytes;
    let slice = if truncated {
        &bytes[..max_bytes]
    } else {
        bytes
    };
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

pub(crate) fn default_source_for_skill(
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

pub(crate) fn find_market_skill(
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

pub(crate) fn default_source_for_plugin(
    conn: &rusqlite::Connection,
    namespace: &str,
    plugin_id: &str,
) -> Result<Option<String>> {
    conn.query_row(
        "SELECT source_id FROM plugin_catalog_cache WHERE namespace = ?1 AND plugin_id = ?2 LIMIT 1",
        params![namespace, plugin_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub(crate) fn find_market_plugin(
    conn: &rusqlite::Connection,
    source_id: Option<&str>,
    namespace: &str,
    plugin_id: &str,
) -> Result<MarketPlugin> {
    let plugins = list_market_plugins_inner(conn)?;
    plugins
        .into_iter()
        .find(|plugin| {
            plugin.namespace == namespace
                && plugin.id == plugin_id
                && source_id
                    .map(|id| plugin.source_id.as_deref() == Some(id))
                    .unwrap_or(true)
        })
        .ok_or_else(|| anyhow!("PLUGIN_SOURCE_INVALID: plugin not found {namespace}/{plugin_id}"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::new_id;
    use std::fs;

    #[test]
    fn plugin_preview_file_list_includes_native_manifest_directory() {
        let root = std::env::temp_dir().join(format!("skillhub-plugin-preview-{}", new_id()));
        fs::create_dir_all(root.join(".codex-plugin")).expect("create manifest dir");
        fs::write(
            root.join(".codex-plugin").join("plugin.json"),
            br#"{"name":"demo"}"#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("skills").join("demo")).expect("create skill dir");
        fs::write(
            root.join("skills").join("demo").join("SKILL.md"),
            b"# Demo\n",
        )
        .expect("write skill");

        let entries = collect_preview_file_list(&root).expect("collect preview files");

        assert!(entries
            .iter()
            .any(|entry| entry.path == ".codex-plugin/plugin.json"));
        assert!(entries
            .iter()
            .any(|entry| entry.path == "skills/demo/SKILL.md"));
        fs::remove_dir_all(root).expect("remove temp dir");
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
}
