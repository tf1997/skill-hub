use anyhow::{anyhow, Result};

use crate::models::PublishMeta;

pub(crate) fn is_publish_meta_ready_for_status(meta: &PublishMeta) -> bool {
    !meta.namespace.trim().is_empty()
        && !meta.skill_id.trim().is_empty()
        && !meta.name.trim().is_empty()
        && !meta.summary.trim().is_empty()
}

pub(crate) fn is_publish_meta_complete(meta: &PublishMeta) -> bool {
    is_publish_meta_ready_for_status(meta)
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

pub(crate) fn validate_publish_meta(meta: &PublishMeta) -> Result<()> {
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

pub(crate) fn validate_object_segment(name: &str, value: &str) -> Result<()> {
    if is_valid_object_segment_value(value) {
        Ok(())
    } else {
        Err(anyhow!("{name} 只能包含字母、数字、点、下划线和短横线"))
    }
}

pub(crate) fn is_valid_object_segment_value(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && !trimmed.contains("..")
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

pub(crate) fn normalize_relative_object_path(value: &str) -> Result<String> {
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

pub(crate) fn validate_plugin_target(target: &str) -> Result<()> {
    match target {
        "codex" | "claude" => Ok(()),
        _ => Err(anyhow!("PLUGIN_TARGET_UNSUPPORTED: {target}")),
    }
}

pub(crate) fn validate_plugin_scope(scope: &str) -> Result<()> {
    match scope {
        "user" | "project" | "local" => Ok(()),
        _ => Err(anyhow!("PLUGIN_SOURCE_INVALID: 不支持的 scope {scope}")),
    }
}

pub(crate) fn validate_plugin_target_scope(target: &str, scope: &str) -> Result<()> {
    if target == "codex" && scope == "project" {
        return Err(anyhow!(
            "PLUGIN_SCOPE_UNSUPPORTED: Codex plugin only supports user scope"
        ));
    }
    Ok(())
}

pub(crate) fn validate_target(target: &str) -> Result<()> {
    match target {
        "codex" | "claude" => Ok(()),
        _ => Err(anyhow!("不支持的目标平台: {target}")),
    }
}

pub(crate) fn validate_level(level: &str) -> Result<()> {
    match level {
        "personal" | "project" => Ok(()),
        _ => Err(anyhow!("不支持的作用域: {level}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_object_segments() {
        assert!(validate_object_segment("namespace", "team_alpha-1").is_ok());
        assert!(validate_object_segment("namespace", "../team").is_err());
        assert!(normalize_relative_object_path("cat/demo").is_ok());
        assert!(normalize_relative_object_path("cat/../demo").is_err());
    }

    #[test]
    fn plugin_target_scope_rejects_codex_project_plugins() {
        assert!(validate_plugin_target_scope("codex", "user").is_ok());
        assert!(validate_plugin_target_scope("claude", "project").is_ok());
        let err = validate_plugin_target_scope("codex", "project")
            .expect_err("Codex plugins do not support project-level activation");
        assert!(err.to_string().contains("PLUGIN_SCOPE_UNSUPPORTED"));
    }
}
