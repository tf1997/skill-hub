use std::{fs, io::Cursor, path::Path};

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

use crate::{
    db::{canonical_display_path, AppState},
    models::PluginSourceMeta,
};

use super::validation;

pub(crate) fn verify_sha256(bytes: &[u8], expected: &str) -> Result<()> {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let actual = format!("{:x}", hasher.finalize());
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err(anyhow!("SHA-256 校验失败"))
    }
}

pub(crate) fn extract_zip_safely(bytes: &[u8], destination: &Path) -> Result<()> {
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

pub(crate) fn extract_zip_preserving_json_safely(bytes: &[u8], destination: &Path) -> Result<()> {
    let reader = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(reader).context("PLUGIN_PACKAGE_BUILD_FAILED: open zip failed")?;

    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let Some(enclosed_name) = file.enclosed_name().map(|path| path.to_owned()) else {
            return Err(anyhow!(
                "PLUGIN_PACKAGE_BUILD_FAILED: zip contains unsafe path"
            ));
        };

        let out_path = destination.join(enclosed_name);
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

pub(crate) fn remove_json_files_recursive(root: &Path) -> Result<()> {
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

pub(crate) fn build_package_zip(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
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

pub(crate) fn build_plugin_package_zip(
    files: &[(String, Vec<u8>)],
    meta: &PluginSourceMeta,
    target: &str,
) -> Result<Vec<u8>> {
    validation::validate_plugin_target(target)?;
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let manifest_path = plugin_native_manifest_path(target)?;
    let manifest = serde_json::to_vec_pretty(&build_native_plugin_manifest(meta, target, files)?)?;
    writer.start_file(manifest_path, options)?;
    std::io::Write::write_all(&mut writer, &manifest)?;
    let mut written = 1_usize;

    for (relative, bytes) in files {
        let Some(relative) = normalize_zip_relative_path(relative) else {
            return Err(anyhow!(
                "PLUGIN_PACKAGE_BUILD_FAILED: 包含不安全路径 {relative}"
            ));
        };
        if !should_include_common_plugin_file(&relative, target) {
            continue;
        }
        writer.start_file(relative, options)?;
        std::io::Write::write_all(&mut writer, bytes)?;
        written += 1;
    }

    if written == 0 {
        return Err(anyhow!(
            "PLUGIN_PACKAGE_BUILD_FAILED: plugin 草稿根目录下没有可打包文件"
        ));
    }

    Ok(writer.finish()?.into_inner())
}

fn plugin_native_manifest_path(target: &str) -> Result<&'static str> {
    match target {
        "codex" => Ok(".codex-plugin/plugin.json"),
        "claude" => Ok(".claude-plugin/plugin.json"),
        _ => Err(anyhow!("PLUGIN_TARGET_UNSUPPORTED: {target}")),
    }
}

fn build_native_plugin_manifest(
    meta: &PluginSourceMeta,
    target: &str,
    files: &[(String, Vec<u8>)],
) -> Result<serde_json::Value> {
    validation::validate_plugin_target(target)?;
    let paths = common_plugin_paths(files);
    let mut manifest = serde_json::Map::new();
    manifest.insert(
        "name".to_string(),
        serde_json::Value::String(meta.id.clone()),
    );
    manifest.insert(
        "version".to_string(),
        serde_json::Value::String(meta.version.clone()),
    );
    manifest.insert(
        "description".to_string(),
        serde_json::Value::String(meta.summary.clone()),
    );
    manifest.insert(
        "displayName".to_string(),
        serde_json::Value::String(meta.name.clone()),
    );
    manifest.insert(
        "skillHub".to_string(),
        serde_json::json!({
            "schema": "skillhub.generated-platform-plugin.v1",
            "namespace": meta.namespace,
            "id": meta.id,
            "target": target
        }),
    );
    if !direct_child_dirs(&paths, "skills/").is_empty() {
        manifest.insert(
            "skills".to_string(),
            serde_json::Value::String("./skills".to_string()),
        );
    }
    if paths.iter().any(|path| path == ".mcp.json") {
        manifest.insert(
            "mcpServers".to_string(),
            serde_json::Value::String("./.mcp.json".to_string()),
        );
    }
    if target == "codex" && paths.iter().any(|path| path == ".app.json") {
        manifest.insert(
            "apps".to_string(),
            serde_json::Value::String("./.app.json".to_string()),
        );
    }
    Ok(serde_json::Value::Object(manifest))
}

fn should_include_common_plugin_file(path: &str, target: &str) -> bool {
    if path == "pluginhub.json" {
        return false;
    }
    if matches!(
        path,
        "README.md" | "CHANGELOG.md" | "LICENSE" | "LICENSE.md" | ".mcp.json"
    ) {
        return true;
    }
    if path.starts_with("skills/") || path.starts_with("hooks/") || path.starts_with("assets/") {
        return true;
    }
    match target {
        "codex" => path == ".app.json",
        "claude" => {
            path == ".lsp.json"
                || path == "settings.json"
                || path.starts_with("agents/")
                || path.starts_with("monitors/")
                || path.starts_with("bin/")
        }
        _ => false,
    }
}

pub(crate) fn normalize_zip_relative_path(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let trimmed = normalized.trim().trim_matches('/');
    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed.contains('\0')
        || trimmed
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn common_plugin_paths(files: &[(String, Vec<u8>)]) -> Vec<String> {
    let mut paths = files
        .iter()
        .filter_map(|(path, _)| normalize_zip_relative_path(path))
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

pub(crate) fn direct_child_dirs(paths: &[String], prefix: &str) -> Vec<String> {
    let mut items = paths
        .iter()
        .filter_map(|path| path.strip_prefix(prefix))
        .filter_map(|rest| rest.split('/').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    items.sort();
    items.dedup();
    items
}

pub(crate) fn ensure_safe_package_cache_path(state: &AppState, package_path: &Path) -> Result<()> {
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

pub(crate) fn ensure_safe_plugin_package_cache_path(
    state: &AppState,
    package_path: &Path,
) -> Result<()> {
    let package_root = state
        .app_dir
        .join("plugin-packages")
        .canonicalize()
        .context("PLUGIN_PACKAGE_BUILD_FAILED: read plugin package cache root failed")?;
    let target = package_path
        .canonicalize()
        .context("PLUGIN_PACKAGE_BUILD_FAILED: read plugin package cache path failed")?;

    if target.starts_with(&package_root) && target != package_root {
        Ok(())
    } else {
        Err(anyhow!(
            "PLUGIN_PACKAGE_BUILD_FAILED: refused unsafe plugin cache path"
        ))
    }
}

pub(crate) fn copy_package_to_install(package_path: &Path, install_path: &Path) -> Result<()> {
    if install_path.exists() {
        fs::remove_dir_all(install_path).context("清理旧安装目录失败")?;
    }
    fs::create_dir_all(install_path)?;
    copy_dir_recursive(package_path, install_path)
}

pub(crate) fn copy_package_to_install_including_json(
    package_path: &Path,
    install_path: &Path,
) -> Result<()> {
    if install_path.exists() {
        fs::remove_dir_all(install_path).context("清理旧安装目录失败")?;
    }
    fs::create_dir_all(install_path)?;
    copy_dir_recursive_including_json(package_path, install_path)
}

pub(crate) fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
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

pub(crate) fn copy_dir_recursive_including_json(source: &Path, destination: &Path) -> Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if source_path.is_dir() {
            fs::create_dir_all(&destination_path)?;
            copy_dir_recursive_including_json(&source_path, &destination_path)?;
        } else {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &destination_path)?;
        }
    }

    Ok(())
}

pub(crate) fn is_json_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Cursor,
        time::{SystemTime, UNIX_EPOCH},
    };

    use zip::ZipArchive;

    use super::*;
    use crate::models::PluginSourceMeta;

    fn sample_plugin_meta() -> PluginSourceMeta {
        PluginSourceMeta {
            schema: "skillhub.plugin-source.v1".to_string(),
            namespace: "team".to_string(),
            id: "demo-plugin".to_string(),
            name: "Demo Plugin".to_string(),
            version: "1.0.0".to_string(),
            summary: "Demo plugin".to_string(),
            author: Some("Skill Hub".to_string()),
            categories: vec!["general".to_string()],
            tags: vec![],
            targets: vec!["codex".to_string()],
            scopes: vec!["user".to_string()],
            components: vec!["skills".to_string()],
            risk_level: Some("low".to_string()),
            publish_scope: Some("public".to_string()),
            publish_project_slug: None,
            platforms: serde_json::json!({}),
        }
    }

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}"))
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
    fn plugin_package_rejects_path_traversal() {
        let files = vec![
            ("skills/review/SKILL.md".to_string(), b"# Review\n".to_vec()),
            ("../escape.txt".to_string(), b"bad".to_vec()),
        ];
        let err = build_plugin_package_zip(&files, &sample_plugin_meta(), "codex")
            .expect_err("unsafe path should fail");
        assert!(err.to_string().contains("PLUGIN_PACKAGE_BUILD_FAILED"));
    }

    #[test]
    fn plugin_zip_extraction_preserves_native_json_manifest() {
        let files = vec![
            ("skills/demo/SKILL.md".to_string(), b"# Demo\n".to_vec()),
            (".mcp.json".to_string(), br#"{"servers":{}}"#.to_vec()),
        ];
        let bytes = build_plugin_package_zip(&files, &sample_plugin_meta(), "codex")
            .expect("plugin zip should build");
        let root = unique_temp_dir("skillhub-plugin-test");
        fs::create_dir_all(&root).expect("create temp dir");
        extract_zip_preserving_json_safely(&bytes, &root).expect("extract plugin zip");
        assert!(root.join(".codex-plugin").join("plugin.json").is_file());
        assert!(root.join(".mcp.json").is_file());
        fs::remove_dir_all(root).expect("remove temp dir");
    }
}
