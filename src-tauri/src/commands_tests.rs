
use crate::services::admin::*;
use crate::services::install::*;
use crate::services::{object_store, validation};

use super::*;

#[test]
fn admin_mac_rejection_does_not_expose_allowlist_macs() {
    let allowlist = admin_config::parse_mac_allowlist(
        r#"{
              "entries": [
                { "mac": "C1:7F:54:5C:60:D8", "role": "system" }
              ]
            }"#,
    )
    .expect("allowlist should parse");

    let err = authorize_admin_from_allowlist(
        admin_config::ADMIN_KEY,
        &[String::from("C8:7F:54:5C:60:D8")],
        &allowlist,
    )
    .expect_err("non-allowlisted mac should be rejected");
    let message = err.to_string();

    assert!(message.contains("C8:7F:54:5C:60:D8"));
    assert!(!message.contains("C1:7F:54:5C:60:D8"));
    assert!(!message.contains("白名单:"));
}

#[test]
fn admin_audit_envelope_includes_authorized_mac() {
    let authorization = admin_config::AdminAuthorization {
        role: "project".to_string(),
        projects: vec![],
        mac_address: "C8:7F:54:5C:60:D8".to_string(),
        name: Some("系统管理员".to_string()),
    };

    let envelope = admin_audit_envelope(
        "saveMarketProject",
        &authorization,
        serde_json::json!({ "slug": "live-project" }),
    );

    assert_eq!(envelope["action"], "saveMarketProject");
    assert_eq!(envelope["actor"], "系统管理员");
    assert_eq!(envelope["role"], "project");
    assert_eq!(envelope["macAddress"], "C8:7F:54:5C:60:D8");
    assert_eq!(envelope["payload"]["slug"], "live-project");
}

#[test]
fn admin_audit_log_normalizes_wrapped_payload_identity() {
    let record = admin_audit_log_from_value(
        "admin/audit/2026/06/14/saveMarketProject-abc.json",
        serde_json::json!({
            "schema": "skillhub.admin-audit.v1",
            "action": "saveMarketProject",
            "actor": "系统管理员",
            "role": "project",
            "macAddress": "C8:7F:54:5C:60:D8",
            "payload": {
                "slug": "live-project"
            },
            "createdAt": "2026-06-14T10:20:30Z"
        }),
    )
    .expect("audit record should normalize");

    assert_eq!(record.action, "saveMarketProject");
    assert_eq!(record.actor.as_deref(), Some("系统管理员"));
    assert_eq!(record.role.as_deref(), Some("project"));
    assert_eq!(record.mac_address.as_deref(), Some("C8:7F:54:5C:60:D8"));
    assert_eq!(record.target.as_deref(), Some("live-project"));
    assert!(record.summary.contains("live-project"));
    assert_eq!(record.created_at, "2026-06-14T10:20:30Z");
}

#[test]
fn project_admin_cannot_view_admin_audit_logs() {
    let authorization = admin_config::AdminAuthorization {
        role: "project".to_string(),
        projects: vec![],
        mac_address: "C8:7F:54:5C:60:D8".to_string(),
        name: Some("项目管理员".to_string()),
    };

    let err = ensure_can_view_admin_audit(&authorization)
        .expect_err("project admin should not view audit logs");

    assert!(err.to_string().contains("系统管理员"));
}

#[test]
fn local_skill_enable_disable_paths_round_trip() {
    let active = PathBuf::from(r"C:\Users\ctf19\.codex\skills\daily-note-helper");
    let disabled = disabled_local_skill_path(&active).expect("disabled path should resolve");
    assert_eq!(
        disabled,
        PathBuf::from(r"C:\Users\ctf19\.codex\skills\.skill-hub-disabled\daily-note-helper")
    );
    assert_eq!(
        enabled_local_skill_path(&disabled).expect("enabled path should resolve"),
        active
    );
}

#[test]
fn local_skill_enable_path_requires_disabled_root() {
    let active = PathBuf::from(r"C:\Users\ctf19\.codex\skills\daily-note-helper");
    assert!(enabled_local_skill_path(&active).is_err());
}

#[test]
fn parses_skill_markdown_admin_fields() {
    let content = r#"---
version: 1.2.3
author: "Skill Hub"
---

# Demo
"#;

    assert_eq!(
        parse_skill_markdown_field(content, "version").as_deref(),
        Some("1.2.3")
    );
    assert_eq!(
        parse_skill_markdown_field(content, "author").as_deref(),
        Some("Skill Hub")
    );
}

#[test]
fn validates_publish_meta_completeness() {
    let meta = PublishMeta {
        namespace: "community".to_string(),
        skill_id: "demo".to_string(),
        version: None,
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

    assert!(validation::validate_publish_meta(&meta).is_ok());
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
        version: None,
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
fn parses_gitlab_draft_category_paths_from_skill_location() {
    let single = parse_gitlab_source_path("product/prd-shaper");
    assert_eq!(single.category_path, vec!["product".to_string()]);
    assert_eq!(single.category_code().as_deref(), Some("product"));
    assert_eq!(single.draft_slug.as_deref(), Some("prd-shaper"));

    let nested = parse_gitlab_source_path("general/product/prd-shaper");
    assert_eq!(
        nested.category_path,
        vec!["general".to_string(), "product".to_string()]
    );
    assert_eq!(nested.category_code().as_deref(), Some("general/product"));
    assert_eq!(nested.draft_slug.as_deref(), Some("prd-shaper"));

    assert_ne!(single.category_code(), nested.category_code());
}

#[test]
fn parses_plugin_draft_multi_level_category_path() {
    let nested = parse_gitlab_source_path("backend/java/commit-workflow");
    assert_eq!(
        nested.category_path,
        vec!["backend".to_string(), "java".to_string()]
    );
    assert_eq!(nested.category_code().as_deref(), Some("backend/java"));
    assert_eq!(nested.draft_slug.as_deref(), Some("commit-workflow"));
}

#[test]
fn plugin_publish_generates_target_packages_from_common_source() {
    let files = sample_plugin_files(vec![
        (
            "README.md",
            br#"---
name: Commit Workflow
description: Team commit and PR workflow plugin.
version: 1.0.0
author: skill-hub
---
# Commit Workflow
"#
            .to_vec(),
        ),
        ("CHANGELOG.md", b"## 1.0.0\n".to_vec()),
        ("skills/review/SKILL.md", b"# Review\n".to_vec()),
        ("agents/reviewer.md", b"agent".to_vec()),
    ]);

    let prepared =
        prepare_plugin_publish(&files, None).expect("common plugin source should publish");
    let codex_package = prepared.packages.get("codex").expect("codex package");
    let claude_package = prepared.packages.get("claude").expect("claude package");

    let mut codex_archive =
        ZipArchive::new(Cursor::new(codex_package.bytes.clone())).expect("codex zip opens");
    assert!(codex_archive.by_name(".codex-plugin/plugin.json").is_ok());
    assert!(codex_archive.by_name("skills/review/SKILL.md").is_ok());
    assert!(codex_archive
        .by_name("codex/.codex-plugin/plugin.json")
        .is_err());
    assert!(codex_archive
        .by_name("claude/.claude-plugin/plugin.json")
        .is_err());

    let mut claude_archive =
        ZipArchive::new(Cursor::new(claude_package.bytes.clone())).expect("claude zip opens");
    assert!(claude_archive.by_name(".claude-plugin/plugin.json").is_ok());
    assert!(claude_archive.by_name("skills/review/SKILL.md").is_ok());
    assert!(claude_archive.by_name("agents/reviewer.md").is_ok());
    assert!(claude_archive
        .by_name("codex/.codex-plugin/plugin.json")
        .is_err());
    assert!(claude_archive
        .by_name("claude/.claude-plugin/plugin.json")
        .is_err());
    assert_eq!(prepared.risk_level, "medium");
}

#[test]
fn plugin_publish_rejects_readme_without_required_frontmatter() {
    let files = vec![
        ("README.md".to_string(), b"# Commit Workflow\n".to_vec()),
        ("skills/review/SKILL.md".to_string(), b"# Review\n".to_vec()),
    ];
    let saved = PublishMeta {
        namespace: "internal".to_string(),
        skill_id: "commit-workflow".to_string(),
        version: Some("1.0.0".to_string()),
        name: "Commit Workflow".to_string(),
        summary: "Team commit and PR workflow plugin.".to_string(),
        tags: vec!["git".to_string()],
        targets: vec!["codex".to_string(), "claude".to_string()],
        levels: vec!["user".to_string(), "project".to_string()],
        publish_scope: "public".to_string(),
        publish_category_slug: Some("backend".to_string()),
        publish_project_slug: None,
        changelog: String::new(),
        updated_at: None,
        updated_by: None,
    };

    let err = prepare_plugin_publish(&files, Some(saved))
        .expect_err("README front matter should be required");

    assert!(err.to_string().contains("README.md 缺少 name"));
}

#[test]
fn plugin_publish_can_use_readme_frontmatter_without_pluginhub() {
    let files = vec![
        (
            "README.md".to_string(),
            br#"---
name: webapp-testing
description: Toolkit for interacting with and testing local web applications using Playwright.
metadata:
  version: 1.0.0
  author: skill-hub
  tags: [web, testing]
---
# Webapp Testing
"#
            .to_vec(),
        ),
        ("skills/review/SKILL.md".to_string(), b"# Review\n".to_vec()),
    ];
    let saved = PublishMeta {
        namespace: "internal".to_string(),
        skill_id: "webapp-testing".to_string(),
        version: Some("1.0.0".to_string()),
        name: String::new(),
        summary: String::new(),
        tags: Vec::new(),
        targets: vec!["codex".to_string()],
        levels: vec!["user".to_string(), "project".to_string()],
        publish_scope: "public".to_string(),
        publish_category_slug: Some("testing".to_string()),
        publish_project_slug: None,
        changelog: String::new(),
        updated_at: None,
        updated_by: None,
    };

    let prepared = prepare_plugin_publish(&files, Some(saved))
        .expect("README front matter should fill plugin source metadata");

    assert_eq!(prepared.meta.id, "webapp-testing");
    assert_eq!(prepared.meta.name, "webapp-testing");
    assert_eq!(
        prepared.meta.summary,
        "Toolkit for interacting with and testing local web applications using Playwright."
    );
    assert_eq!(prepared.meta.version, "1.0.0");
    assert_eq!(prepared.meta.tags, vec!["web", "testing"]);
    assert!(prepared.packages.contains_key("codex"));
}

#[test]
fn plugin_publish_meta_overrides_source_publish_target() {
    let source = PluginSourceMeta {
        schema: "skillhub.plugin-source.v1".to_string(),
        namespace: "internal".to_string(),
        id: "commit-workflow".to_string(),
        name: "Commit Workflow".to_string(),
        version: "1.0.0".to_string(),
        summary: "From pluginhub".to_string(),
        author: None,
        categories: vec!["backend".to_string()],
        tags: vec!["git".to_string()],
        targets: vec!["codex".to_string(), "claude".to_string()],
        scopes: vec!["user".to_string(), "project".to_string()],
        components: vec!["skills".to_string()],
        risk_level: Some("low".to_string()),
        publish_scope: Some("public".to_string()),
        publish_project_slug: None,
        platforms: serde_json::json!({}),
    };
    let saved = PublishMeta {
        namespace: "ignored".to_string(),
        skill_id: "ignored".to_string(),
        version: Some("1.1.0".to_string()),
        name: "Managed Commit Workflow".to_string(),
        summary: "Managed summary".to_string(),
        tags: vec!["managed".to_string()],
        targets: vec!["claude".to_string()],
        levels: vec!["project".to_string()],
        publish_scope: "project".to_string(),
        publish_category_slug: None,
        publish_project_slug: Some("alpha".to_string()),
        changelog: "Managed changelog".to_string(),
        updated_at: None,
        updated_by: None,
    };

    let merged = apply_plugin_publish_meta(source, Some(saved));

    assert_eq!(merged.name, "Managed Commit Workflow");
    assert_eq!(merged.summary, "Managed summary");
    assert_eq!(merged.tags, vec!["managed"]);
    assert_eq!(merged.targets, vec!["claude"]);
    assert_eq!(merged.scopes, vec!["project"]);
    assert_eq!(merged.publish_scope.as_deref(), Some("project"));
    assert_eq!(merged.publish_project_slug.as_deref(), Some("alpha"));
    assert_eq!(plugin_publish_categories(&merged), vec!["project:alpha"]);
}

#[test]
fn plugin_publish_rejects_platform_generated_directories_in_source() {
    let files = sample_plugin_files(vec![(
        "codex/.codex-plugin/plugin.json",
        br#"{"name":"Commit Workflow"}"#.to_vec(),
    )]);
    let err = prepare_plugin_publish(&files, None).expect_err("platform directories should fail");
    assert!(err.to_string().contains("PLUGIN_SOURCE_INVALID"));
}

#[test]
fn plugin_package_filters_target_specific_common_files() {
    let files = sample_plugin_files(vec![
        ("skills/review/SKILL.md", b"# Review\n".to_vec()),
        ("agents/reviewer.md", b"agent".to_vec()),
        (".app.json", br#"{"apps":[]}"#.to_vec()),
    ]);
    let prepared = prepare_plugin_publish(&files, None).expect("plugin publish should prepare");
    let codex_package = prepared.packages.get("codex").expect("codex package");
    let mut archive = ZipArchive::new(Cursor::new(codex_package.bytes.clone())).expect("zip opens");
    assert!(archive.by_name(".codex-plugin/plugin.json").is_ok());
    assert!(archive.by_name("skills/review/SKILL.md").is_ok());
    assert!(archive.by_name(".app.json").is_ok());
    assert!(archive.by_name("agents/reviewer.md").is_err());
    assert_eq!(prepared.risk_level, "medium");
}

#[test]
fn plugin_package_rejects_path_traversal() {
    let files = sample_plugin_files(vec![
        ("skills/review/SKILL.md", b"# Review\n".to_vec()),
        ("../escape.txt", b"bad".to_vec()),
    ]);
    let err = prepare_plugin_publish(&files, None).expect_err("unsafe path should fail");
    assert!(err.to_string().contains("PLUGIN_PACKAGE_BUILD_FAILED"));
}

#[test]
fn plugin_marketplace_files_point_to_materialized_plugin_dir() {
    let root = std::env::temp_dir().join(format!("skillhub-plugin-market-{}", new_id()));
    let plugin = MarketPlugin {
        namespace: "internal".to_string(),
        id: "commit-workflow".to_string(),
        name: "Commit Workflow".to_string(),
        summary: "Team workflow plugin.".to_string(),
        latest_version: "1.0.0".to_string(),
        categories: vec!["backend".to_string()],
        tags: vec![],
        targets: vec!["codex".to_string()],
        scopes: vec!["user".to_string()],
        components: vec!["skills".to_string()],
        risk_level: "low".to_string(),
        manifest_path: "plugins/internal/commit-workflow/manifest.json".to_string(),
        updated_at: None,
        source_id: Some("compiled-source".to_string()),
        installed_bindings: vec![],
        cached_versions: vec![],
    };
    let codex_path = write_plugin_marketplace_file(
        "codex",
        &root,
        "internal.commit-workflow",
        &plugin,
        "1.0.0",
        "skillhub",
    )
    .expect("write codex marketplace");
    assert_eq!(
        canonical_display_path(&codex_path),
        canonical_display_path(
            &root
                .join(".agents")
                .join("plugins")
                .join("marketplace.json")
        )
    );
    let codex_doc: serde_json::Value =
        serde_json::from_slice(&fs::read(codex_path).expect("read marketplace")).expect("json");
    assert_eq!(
        codex_doc["plugins"][0]["source"]["path"],
        "./plugins/internal.commit-workflow"
    );
    assert_eq!(codex_doc["plugins"][0]["source"]["source"], "local");
    assert_eq!(
        codex_doc["plugins"][0]["policy"]["installation"],
        "AVAILABLE"
    );

    let claude_path = write_plugin_marketplace_file(
        "claude",
        &root,
        "internal.commit-workflow",
        &plugin,
        "1.0.0",
        "skillhub",
    )
    .expect("write claude marketplace");
    let claude_doc: serde_json::Value =
        serde_json::from_slice(&fs::read(claude_path).expect("read marketplace")).expect("json");
    assert_eq!(
        claude_doc["plugins"][0]["source"],
        "./plugins/internal.commit-workflow"
    );
    assert_eq!(claude_doc["owner"]["name"], "Skill Hub");
    fs::remove_dir_all(root).expect("remove temp dir");
}

#[test]
fn plugin_marketplace_upsert_preserves_other_entries() {
    let existing = serde_json::json!({
        "name": "old",
        "plugins": [
            { "name": "alpha", "source": { "path": "./plugins/alpha" } },
            { "name": "commit-workflow", "source": { "path": "./plugins/old" } }
        ]
    });
    let next = upsert_plugin_marketplace_entry(
        existing,
        "skillhub",
        Some(serde_json::json!(1)),
        serde_json::json!({
            "name": "commit-workflow",
            "source": { "path": "./plugins/internal.commit-workflow" }
        }),
    );
    let plugins = next["plugins"].as_array().expect("plugins array");
    assert_eq!(plugins.len(), 2);
    assert_eq!(plugins[0]["name"], "alpha");
    assert_eq!(
        plugins[1]["source"]["path"],
        "./plugins/internal.commit-workflow"
    );
}

#[test]
fn claude_marketplace_schema_backfills_missing_owner() {
    let next = ensure_claude_marketplace_schema(serde_json::json!({
        "name": "skillhub",
        "plugins": []
    }));

    assert!(next["owner"].is_object());
    assert_eq!(next["owner"]["name"], "Skill Hub");
}

#[test]
fn plugin_marketplace_remove_preserves_other_entries() {
    let existing = serde_json::json!({
        "name": "skillhub",
        "plugins": [
            { "name": "alpha", "source": { "path": "./plugins/alpha" } },
            { "name": "commit-workflow", "source": { "path": "./plugins/internal.commit-workflow" } }
        ]
    });
    let next = remove_plugin_marketplace_entry(existing, "commit-workflow");
    let plugins = next["plugins"].as_array().expect("plugins array");
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0]["name"], "alpha");
}

#[test]
fn delete_cached_plugin_rejects_any_binding_relation() {
    let (state, package_id, package_dir) = plugin_cache_test_state(Some("disabled"));
    let request = DeleteCachedPluginRequest {
        source_id: Some("compiled-source".to_string()),
        namespace: "internal".to_string(),
        plugin_id: "commit-workflow".to_string(),
        version: "1.0.0".to_string(),
        target: "codex".to_string(),
    };

    let err = delete_cached_plugin_inner(request, &state)
        .expect_err("any binding relation should block cache deletion");

    assert!(err.to_string().contains("bindings"));
    assert!(package_dir.exists());

    let conn = state.conn.lock().expect("db mutex poisoned");
    let package_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM plugin_packages WHERE id = ?1",
            params![package_id],
            |row| row.get(0),
        )
        .expect("count packages");
    assert_eq!(package_count, 1);
}

#[test]
fn delete_cached_plugin_removes_unbound_cache_package() {
    let (state, package_id, package_dir) = plugin_cache_test_state(None);
    let request = DeleteCachedPluginRequest {
        source_id: Some("compiled-source".to_string()),
        namespace: "internal".to_string(),
        plugin_id: "commit-workflow".to_string(),
        version: "1.0.0".to_string(),
        target: "codex".to_string(),
    };

    delete_cached_plugin_inner(request, &state).expect("unbound cache should delete");

    assert!(!package_dir.exists());

    let conn = state.conn.lock().expect("db mutex poisoned");
    let package_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM plugin_packages WHERE id = ?1",
            params![package_id],
            |row| row.get(0),
        )
        .expect("count packages");
    let metadata_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM local_package_metadata WHERE package_id = ?1",
            params![package_id],
            |row| row.get(0),
        )
        .expect("count metadata");
    assert_eq!(package_count, 0);
    assert_eq!(metadata_count, 0);
}

#[test]
fn plugin_draft_content_prefix_prefers_flat_plugin_root() {
    let root = "draft/gitlab/plugins/backend/java/commit-workflow/";
    let objects = vec![
        "draft/gitlab/plugins/backend/java/commit-workflow/pluginhub.json".to_string(),
        "draft/gitlab/plugins/backend/java/commit-workflow/skills/review/SKILL.md".to_string(),
        "draft/gitlab/plugins/backend/java/commit-workflow/source/pluginhub.json".to_string(),
    ];
    let resolved =
        resolve_plugin_draft_content_prefix(root, &objects).expect("flat root should resolve");
    assert_eq!(resolved.prefix, root);
    assert_eq!(
        resolved.pluginhub_path,
        "draft/gitlab/plugins/backend/java/commit-workflow/pluginhub.json"
    );

    let files = collect_plugin_draft_preview_file_list(&resolved.prefix, &objects);
    let paths = files.into_iter().map(|file| file.path).collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            "pluginhub.json".to_string(),
            "skills/review/SKILL.md".to_string()
        ]
    );
}

#[test]
fn plugin_draft_content_prefix_falls_back_to_legacy_source_root() {
    let root = "draft/gitlab/plugins/backend/java/commit-workflow/";
    let objects = vec![
        "draft/gitlab/plugins/backend/java/commit-workflow/source/pluginhub.json".to_string(),
        "draft/gitlab/plugins/backend/java/commit-workflow/source/skills/review/SKILL.md"
            .to_string(),
    ];
    let resolved = resolve_plugin_draft_content_prefix(root, &objects)
        .expect("legacy source root should resolve");
    assert_eq!(
        resolved.prefix,
        "draft/gitlab/plugins/backend/java/commit-workflow/source/"
    );
    assert_eq!(
        resolved.pluginhub_path,
        "draft/gitlab/plugins/backend/java/commit-workflow/source/pluginhub.json"
    );
}

#[test]
fn plugin_draft_content_prefix_uses_gitlab_source_when_flat_root_is_generated_artifact() {
    let root = "draft/gitlab/plugins/backend/java/commit-workflow/";
    let objects = vec![
        "draft/gitlab/plugins/backend/java/commit-workflow/pluginhub.json".to_string(),
        "draft/gitlab/plugins/backend/java/commit-workflow/.codex-plugin/plugin.json".to_string(),
        "draft/gitlab/plugins/backend/java/commit-workflow/codex/.codex-plugin/plugin.json"
            .to_string(),
        "draft/gitlab/plugins/backend/java/commit-workflow/source/pluginhub.json".to_string(),
        "draft/gitlab/plugins/backend/java/commit-workflow/source/README.md".to_string(),
        "draft/gitlab/plugins/backend/java/commit-workflow/source/skills/review/SKILL.md"
            .to_string(),
    ];

    let resolved = resolve_plugin_draft_content_prefix(root, &objects)
        .expect("GitLab source root should resolve");

    assert_eq!(
        resolved.prefix,
        "draft/gitlab/plugins/backend/java/commit-workflow/source/"
    );
    assert_eq!(
        resolved.pluginhub_path,
        "draft/gitlab/plugins/backend/java/commit-workflow/source/pluginhub.json"
    );

    let files = collect_plugin_draft_preview_file_list(&resolved.prefix, &objects);
    let paths = files.into_iter().map(|file| file.path).collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            "README.md".to_string(),
            "pluginhub.json".to_string(),
            "skills/review/SKILL.md".to_string()
        ]
    );
}

#[test]
fn plugin_draft_preview_file_list_uses_plugin_content_prefix() {
    let prefix = "draft/gitlab/plugins/backend/java/commit-workflow/";
    let files = collect_plugin_draft_preview_file_list(
        prefix,
        &[
            "draft/gitlab/plugins/backend/java/commit-workflow/pluginhub.json".to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/skills/review/SKILL.md".to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/agents/reviewer.md".to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/validation.json".to_string(),
        ],
    );
    let paths = files.into_iter().map(|file| file.path).collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            "agents/reviewer.md".to_string(),
            "pluginhub.json".to_string(),
            "skills/review/SKILL.md".to_string(),
            "validation.json".to_string()
        ]
    );
}

#[test]
fn plugin_draft_preview_file_list_excludes_publish_meta() {
    let prefix = "draft/gitlab/plugins/backend/java/commit-workflow/";
    let files = collect_plugin_draft_preview_file_list(
        prefix,
        &[
            "draft/gitlab/plugins/backend/java/commit-workflow/pluginhub.json".to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/README.md".to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/publish-meta.v1.json".to_string(),
            "draft/gitlab/plugins/backend/java/commit-workflow/skills/review/SKILL.md".to_string(),
        ],
    );
    let paths = files.into_iter().map(|file| file.path).collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            "README.md".to_string(),
            "pluginhub.json".to_string(),
            "skills/review/SKILL.md".to_string()
        ]
    );
}

#[test]
fn plugin_draft_source_paths_are_discovered_from_pluginhub_without_draft_json() {
    let objects = vec![
        "draft/gitlab/plugins/backend/java/commit-workflow/pluginhub.json".to_string(),
        "draft/gitlab/plugins/backend/java/commit-workflow/README.md".to_string(),
        "draft/gitlab/plugins/productivity/release-helper/pluginhub.json".to_string(),
    ];
    let paths = collect_plugin_draft_source_paths(&objects);
    assert_eq!(
        paths,
        vec![
            "backend/java/commit-workflow".to_string(),
            "productivity/release-helper".to_string()
        ]
    );
}

#[test]
fn plugin_draft_source_paths_include_directories_without_pluginhub() {
    let objects = vec![
        "draft/gitlab/plugins/productivity/automation/release-notes-helper/README.md".to_string(),
        "draft/gitlab/plugins/productivity/automation/release-notes-helper/CHANGELOG.md"
            .to_string(),
        "draft/gitlab/plugins/productivity/automation/release-notes-helper/skills/write/SKILL.md"
            .to_string(),
        "draft/gitlab/plugins/productivity/automation/release-notes-helper/validation.json"
            .to_string(),
    ];
    let paths = collect_plugin_draft_source_paths(&objects);
    assert_eq!(
        paths,
        vec!["productivity/automation/release-notes-helper".to_string()]
    );
}

#[test]
fn plugin_publish_can_synthesize_source_meta_from_readme_and_saved_publish_meta() {
    let files = vec![
        (
            "README.md".to_string(),
            br#"---
name: Release Notes Helper
description: Generate release notes from commits and PRs.
metadata:
  version: 0.2.0
  author: skill-hub
  tags: [release, automation]
---
# Release Notes Helper
"#
            .to_vec(),
        ),
        (
            "skills/write/SKILL.md".to_string(),
            b"# Write Release Notes\n".to_vec(),
        ),
        ("agents/reviewer.md".to_string(), b"agent".to_vec()),
    ];
    let saved = PublishMeta {
        namespace: "productivity".to_string(),
        skill_id: "release-notes-helper".to_string(),
        version: Some("0.2.0".to_string()),
        name: "Release Notes Helper".to_string(),
        summary: "Generate release notes from commits and PRs.".to_string(),
        tags: vec!["release".to_string(), "automation".to_string()],
        targets: vec!["codex".to_string(), "claude".to_string()],
        levels: vec!["user".to_string(), "project".to_string()],
        publish_scope: "public".to_string(),
        publish_category_slug: Some("productivity".to_string()),
        publish_project_slug: None,
        changelog: "Initial plugin publish.".to_string(),
        updated_at: None,
        updated_by: None,
    };

    let prepared =
        prepare_plugin_publish(&files, Some(saved)).expect("saved meta should publish plugin");

    assert_eq!(prepared.meta.namespace, "productivity");
    assert_eq!(prepared.meta.id, "release-notes-helper");
    assert_eq!(prepared.meta.version, "0.2.0");
    assert_eq!(prepared.meta.targets, vec!["codex", "claude"]);
    assert_eq!(prepared.meta.scopes, vec!["user", "project"]);
    assert_eq!(prepared.meta.components, vec!["skills", "agents"]);
    assert!(prepared.packages.contains_key("codex"));
    assert!(prepared.packages.contains_key("claude"));
}

#[test]
fn default_plugin_publish_meta_does_not_take_market_scope_from_pluginhub() {
    let source = PluginSourceMeta {
        schema: "skillhub.plugin-source.v1".to_string(),
        namespace: "internal".to_string(),
        id: "commit-workflow".to_string(),
        name: "Commit Workflow".to_string(),
        version: "1.0.0".to_string(),
        summary: "Workflow helper".to_string(),
        author: None,
        categories: vec!["backend".to_string()],
        tags: vec!["git".to_string()],
        targets: vec!["codex".to_string()],
        scopes: vec!["user".to_string(), "project".to_string()],
        components: vec!["skills".to_string()],
        risk_level: Some("low".to_string()),
        publish_scope: Some("project".to_string()),
        publish_project_slug: Some("alpha".to_string()),
        platforms: serde_json::Value::Null,
    };
    let meta = default_plugin_publish_meta(&source);
    assert_eq!(meta.publish_scope, "public");
    assert_eq!(meta.publish_category_slug, None);
    assert_eq!(meta.publish_project_slug, None);
    assert_eq!(meta.targets, vec!["codex".to_string()]);
}

#[test]
fn claude_plugin_marketplace_path_resolves_from_marketplace_dir() {
    let root = PathBuf::from("/tmp/project");
    let resolved =
        local::resolve_plugin_source_path("claude", &root, "./plugins/internal.commit-workflow");
    assert_eq!(
        canonical_display_path(&resolved),
        "/tmp/project/./plugins/internal.commit-workflow"
    );
}

#[test]
fn claude_project_marketplace_root_lives_under_project_claude_dir() {
    let state = AppState {
        conn: std::sync::Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().expect("open sqlite"),
        )),
        app_dir: PathBuf::from("/tmp/skillhub-app"),
        local_macs: vec![],
    };

    let claude_root = plugin_marketplace_root(&state, "claude", "project", Some("/tmp/project-a"))
        .expect("resolve claude project marketplace root");
    assert_eq!(
        canonical_display_path(&claude_root),
        "/tmp/project-a/.claude/skillhub-plugin-marketplace"
    );

    let codex_root = plugin_marketplace_root(&state, "codex", "project", Some("/tmp/project-a"))
        .expect("resolve codex project marketplace root");
    assert_eq!(canonical_display_path(&codex_root), "/tmp/project-a");
}

#[test]
fn claude_project_marketplace_migration_moves_legacy_skillhub_root_under_claude_dir() {
    let project = std::env::temp_dir().join(format!("skillhub-claude-project-{}", new_id()));
    let legacy_marketplace_dir = project.join(".claude-plugin");
    let legacy_plugin_dir = project.join("plugins").join("internal.commit-workflow");
    fs::create_dir_all(&legacy_marketplace_dir).expect("create legacy marketplace dir");
    fs::create_dir_all(&legacy_plugin_dir).expect("create legacy plugin dir");
    fs::write(
        legacy_marketplace_dir.join("marketplace.json"),
        br#"{
              "name": "skillhub",
              "owner": { "name": "Skill Hub" },
              "plugins": [
                {
                  "name": "commit-workflow",
                  "version": "1.0.0",
                  "source": "./plugins/internal.commit-workflow"
                }
              ]
            }"#,
    )
    .expect("write legacy marketplace");
    fs::write(legacy_plugin_dir.join("README.md"), "# Commit Workflow\n")
        .expect("write legacy plugin file");

    let new_root = project.join(".claude").join("skillhub-plugin-marketplace");
    migrate_claude_project_marketplace_root(
        "claude",
        "project",
        Some(project.to_string_lossy().as_ref()),
        &new_root,
    )
    .expect("migrate legacy marketplace");

    assert!(new_root
        .join(".claude-plugin")
        .join("marketplace.json")
        .is_file());
    assert!(new_root
        .join("plugins")
        .join("internal.commit-workflow")
        .join("README.md")
        .is_file());
    assert!(!project.join(".claude-plugin").exists());
    assert!(!project
        .join("plugins")
        .join("internal.commit-workflow")
        .exists());

    fs::remove_dir_all(project).expect("remove temp project");
}

#[test]
fn claude_plugin_sync_commands_use_scope_and_marketplace_path() {
    let marketplace_root = PathBuf::from("/tmp/project/.claude/skillhub-plugin-marketplace");
    assert_eq!(
        build_claude_plugin_install_commands(
            "claude",
            "commit-workflow",
            "skillhub",
            "project",
            &marketplace_root
        ),
        Some(vec![
            vec![
                "plugin".to_string(),
                "marketplace".to_string(),
                "add".to_string(),
                "/tmp/project/.claude/skillhub-plugin-marketplace/.claude-plugin/marketplace.json"
                    .to_string(),
                "--scope".to_string(),
                "project".to_string(),
            ],
            vec![
                "plugin".to_string(),
                "install".to_string(),
                "commit-workflow@skillhub".to_string(),
                "--scope".to_string(),
                "project".to_string(),
            ],
            vec![
                "plugin".to_string(),
                "enable".to_string(),
                "commit-workflow@skillhub".to_string(),
                "--scope".to_string(),
                "project".to_string(),
            ],
        ])
    );
    assert_eq!(
        build_claude_plugin_install_commands(
            "codex",
            "commit-workflow",
            "skillhub",
            "project",
            &marketplace_root
        ),
        None
    );
}

#[test]
fn claude_project_marketplace_refresh_command_removes_existing_registration() {
    assert_eq!(
        build_claude_marketplace_remove_command("claude", "skillhub", "project"),
        Some(vec![
            "plugin".to_string(),
            "marketplace".to_string(),
            "remove".to_string(),
            "skillhub".to_string(),
            "--scope".to_string(),
            "project".to_string(),
        ])
    );
    assert_eq!(
        build_claude_marketplace_remove_command("claude", "skillhub", "user"),
        None
    );
    assert_eq!(
        build_claude_marketplace_remove_command("codex", "skillhub", "project"),
        None
    );
}

#[test]
fn claude_plugin_project_scope_uses_project_root_as_cli_working_dir() {
    let project = r"C:\Users\ctf19\project-a";
    assert_eq!(
        claude_plugin_command_working_dir("project", Some(project)),
        Some(PathBuf::from(project))
    );
    assert_eq!(
        claude_plugin_command_working_dir("local", Some(project)),
        Some(PathBuf::from(project))
    );
    assert_eq!(
        claude_plugin_command_working_dir("user", Some(project)),
        None
    );
    assert_eq!(claude_plugin_command_working_dir("project", None), None);
}

#[test]
fn codex_project_scope_does_not_sync_global_cli_plugin_config() {
    assert!(should_sync_codex_plugin_cli("codex", "user"));
    assert!(!should_sync_codex_plugin_cli("codex", "project"));
    assert!(!should_sync_codex_plugin_cli("claude", "project"));
}

#[test]
fn claude_plugin_remove_command_uses_disable_or_uninstall() {
    assert_eq!(
        build_claude_plugin_remove_command(
            "claude",
            "commit-workflow",
            "skillhub",
            "user",
            ClaudePluginRemoveAction::Disable
        ),
        Some(vec![
            "plugin".to_string(),
            "disable".to_string(),
            "commit-workflow@skillhub".to_string(),
            "--scope".to_string(),
            "user".to_string(),
        ])
    );
    assert_eq!(
        build_claude_plugin_remove_command(
            "claude",
            "commit-workflow",
            "skillhub",
            "user",
            ClaudePluginRemoveAction::Uninstall
        ),
        Some(vec![
            "plugin".to_string(),
            "uninstall".to_string(),
            "commit-workflow@skillhub".to_string(),
            "--scope".to_string(),
            "user".to_string(),
        ])
    );
}

#[test]
fn claude_plugin_command_candidates_use_windows_cmd_shim() {
    let candidates = claude_plugin_command_candidates();
    if cfg!(windows) {
        assert_eq!(candidates[0].0, "cmd");
        assert_eq!(candidates[0].1, vec!["/C", "claude.cmd"]);
    }
    assert!(candidates
        .iter()
        .any(|(program, prefix)| *program == "claude" && prefix.is_empty()));
}

#[cfg(windows)]
#[test]
fn plugin_command_finds_cmd_shim_from_external_command_path() {
    let temp = std::env::temp_dir().join(format!("skillhub-cli-shim-{}", new_id()));
    fs::create_dir_all(&temp).expect("create shim dir");
    fs::write(temp.join("claude.cmd"), "@echo off\r\nexit /b 0\r\n").expect("write shim");
    let original_path = std::env::var_os("PATH");
    std::env::set_var("PATH", temp.as_os_str());

    let result = run_plugin_command(
        &[("cmd", vec!["/C", "claude.cmd"])],
        &["--version"],
        "TEST_PLUGIN_COMMAND_FAILED",
        None,
        |_| false,
    );

    match original_path {
        Some(path) => std::env::set_var("PATH", path),
        None => std::env::remove_var("PATH"),
    }
    let _ = fs::remove_dir_all(&temp);
    result.expect("cmd shim should run from external command PATH");
}

#[cfg(windows)]
#[test]
fn plugin_command_runs_from_requested_working_dir() {
    let temp = std::env::temp_dir().join(format!("skillhub-cli-cwd-{}", new_id()));
    fs::create_dir_all(&temp).expect("create cwd dir");
    fs::write(temp.join("marker.txt"), "ok").expect("write marker");

    let result = run_plugin_command(
        &[(
            "cmd",
            vec!["/C", "if exist marker.txt (exit /b 0) else (exit /b 1)"],
        )],
        &[],
        "TEST_PLUGIN_COMMAND_FAILED",
        Some(&temp),
        |_| false,
    );

    let _ = fs::remove_dir_all(&temp);
    result.expect("command should run from requested working directory");
}

#[cfg(windows)]
#[test]
fn plugin_command_preserves_first_executed_candidate_failure() {
    let err = run_plugin_command(
        &[
            (
                "cmd",
                vec!["/C", "definitely-missing-skillhub-cli-shim.cmd"],
            ),
            ("definitely-missing-skillhub-cli-shim", Vec::new()),
        ],
        &["--version"],
        "TEST_PLUGIN_COMMAND_FAILED",
        None,
        |_| false,
    )
    .expect_err("missing cmd shim should fail");

    let message = err.to_string();
    assert!(message.contains("cmd exited"));
    assert!(!message.contains("definitely-missing-skillhub-cli-shim:"));
}

#[test]
fn claude_cli_missing_guidance_points_to_install_commands() {
    let message = plugin_cli_missing_guidance("claude");
    assert!(message.contains("Claude Code CLI"));
    assert!(message.contains("irm https://claude.ai/install.ps1 | iex"));
    assert!(message.contains("winget install Anthropic.ClaudeCode"));
    assert!(message.contains("npm install -g @anthropic-ai/claude-code"));
}

#[test]
fn plugin_scope_conflict_blocks_personal_when_project_enabled() {
    let bindings = vec![plugin_binding_fixture(
        "project-binding",
        "project",
        Some("/tmp/project-a"),
        true,
    )];
    let err = ensure_plugin_scope_can_enable_from_bindings(
        &bindings,
        None,
        "internal",
        "commit-workflow",
        "codex",
        "user",
    )
    .expect_err("personal install should conflict with project binding");
    assert!(err.to_string().contains("PLUGIN_SCOPE_CONFLICT"));
}

#[test]
fn plugin_scope_conflict_blocks_project_when_personal_enabled() {
    let bindings = vec![plugin_binding_fixture("user-binding", "user", None, true)];
    let err = ensure_plugin_scope_can_enable_from_bindings(
        &bindings,
        None,
        "internal",
        "commit-workflow",
        "codex",
        "project",
    )
    .expect_err("project install should conflict with user binding");
    assert!(err.to_string().contains("PLUGIN_SCOPE_CONFLICT"));
}

#[test]
fn plugin_scope_conflict_ignores_other_targets_disabled_and_excluded_binding() {
    let mut other_target = plugin_binding_fixture("other-target", "user", None, true);
    other_target.target = "claude".to_string();
    let disabled_project =
        plugin_binding_fixture("disabled-project", "project", Some("/tmp/project-a"), false);
    let current = plugin_binding_fixture("current", "user", None, true);
    let bindings = vec![other_target, disabled_project, current];

    ensure_plugin_scope_can_enable_from_bindings(
        &bindings,
        Some("current"),
        "internal",
        "commit-workflow",
        "codex",
        "project",
    )
    .expect("excluded current binding and unrelated bindings should not conflict");
}

#[test]
fn codex_plugin_marketplace_path_uses_agents_plugins_catalog() {
    let root = PathBuf::from("/tmp/project");
    assert_eq!(
        canonical_display_path(&local::plugin_marketplace_path("codex", &root)),
        "/tmp/project/.agents/plugins/marketplace.json"
    );
}

#[test]
fn codex_user_marketplace_uses_personal_name() {
    assert_eq!(plugin_marketplace_name("codex", "user", None), "personal");
    assert_eq!(
        plugin_marketplace_name("codex", "project", Some("/tmp/project")),
        format!("skillhub-{}", local::path_hash("/tmp/project"))
    );
    assert_eq!(plugin_marketplace_name("claude", "user", None), "skillhub");
}

#[test]
fn codex_cli_idempotent_error_detection_is_narrow() {
    assert!(is_plugin_missing_message(
        "Error: plugin `demo` is not installed"
    ));
    assert!(is_plugin_already_configured_message(
        "Error: marketplace `skillhub` is already configured"
    ));
    assert!(!is_plugin_already_configured_message(
        "Error: plugin `demo` is already installed"
    ));
}

#[test]
fn claude_cli_idempotent_error_detection_handles_already_enabled() {
    assert!(is_plugin_already_enabled_message(
        "Plugin \"commit-workflow@skillhub\" is already enabled at user scope"
    ));
}

fn plugin_binding_fixture(
    id: &str,
    scope: &str,
    project_path: Option<&str>,
    enabled: bool,
) -> PluginBinding {
    PluginBinding {
        id: id.to_string(),
        package_id: "pkg".to_string(),
        source_id: Some("source".to_string()),
        namespace: "internal".to_string(),
        plugin_id: "commit-workflow".to_string(),
        plugin_name: "Commit Workflow".to_string(),
        version: "1.0.0".to_string(),
        target: "codex".to_string(),
        scope: scope.to_string(),
        project_path: project_path.map(ToString::to_string),
        marketplace_id: Some("market".to_string()),
        marketplace_name: "personal".to_string(),
        platform_ref: "commit-workflow@personal".to_string(),
        enabled,
        install_mode: "marketplace".to_string(),
        update_policy: "follow_latest".to_string(),
        status: "installed".to_string(),
        created_at: now(),
        updated_at: now(),
    }
}

fn sample_plugin_files(extra: Vec<(&str, Vec<u8>)>) -> Vec<(String, Vec<u8>)> {
    let mut files = vec![
        (
            "pluginhub.json".to_string(),
            br#"{
              "schema": "skillhub.plugin-source.v1",
              "namespace": "internal",
              "id": "commit-workflow",
              "name": "Commit Workflow",
              "version": "1.0.0",
              "summary": "Team commit and PR workflow plugin.",
              "categories": ["backend"],
              "tags": ["git"],
              "targets": ["codex", "claude"],
              "scopes": ["user", "project"],
              "components": ["skills", "agents"],
              "riskLevel": "medium",
              "publishScope": "public"
            }"#
            .to_vec(),
        ),
        (
            "README.md".to_string(),
            br#"---
name: Commit Workflow
description: Team commit and PR workflow plugin.
version: 1.0.0
author: skill-hub
---
# Commit Workflow
"#
            .to_vec(),
        ),
    ];
    for (path, bytes) in extra {
        if let Some((_, existing_bytes)) = files
            .iter_mut()
            .find(|(existing_path, _)| existing_path == path)
        {
            *existing_bytes = bytes;
        } else {
            files.push((path.to_string(), bytes));
        }
    }
    files
}

fn plugin_cache_test_state(binding_status: Option<&str>) -> (AppState, String, PathBuf) {
    let app_dir = std::env::temp_dir().join(format!("skillhub-plugin-cache-{}", new_id()));
    let package_root = app_dir.join("plugin-packages");
    let package_dir = package_root
        .join("internal")
        .join("commit-workflow")
        .join("1.0.0")
        .join("codex");
    fs::create_dir_all(&package_dir).expect("create package dir");

    let conn = rusqlite::Connection::open_in_memory().expect("open sqlite");
    conn.execute_batch(
        r#"
            CREATE TABLE plugin_packages (
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
              cached_at TEXT NOT NULL
            );
            CREATE TABLE plugin_bindings (
              id TEXT PRIMARY KEY,
              package_id TEXT NOT NULL,
              status TEXT NOT NULL
            );
            CREATE TABLE local_package_metadata (
              package_id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              summary TEXT NOT NULL,
              tags_json TEXT NOT NULL,
              author TEXT,
              source_path TEXT NOT NULL,
              imported_at TEXT NOT NULL
            );
            CREATE TABLE audit_logs (
              id TEXT PRIMARY KEY,
              action TEXT NOT NULL,
              skill_ref TEXT,
              result TEXT NOT NULL,
              detail TEXT,
              created_at TEXT NOT NULL
            );
            "#,
    )
    .expect("create test schema");

    let package_id = new_id();
    let package_path = canonical_display_path(&package_dir);
    conn.execute(
        "INSERT INTO plugin_packages
             (id, source_id, namespace, plugin_id, plugin_name, version, target, package_path,
              sha256, component_inventory_json, risk_level, cached_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            package_id,
            Some("compiled-source".to_string()),
            "internal",
            "commit-workflow",
            "Commit Workflow",
            "1.0.0",
            "codex",
            package_path,
            None::<String>,
            "{}",
            "low",
            now()
        ],
    )
    .expect("insert package");
    conn.execute(
        "INSERT INTO local_package_metadata
             (package_id, name, summary, tags_json, author, source_path, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            package_id,
            "Commit Workflow",
            "Team commit and PR workflow plugin.",
            "[]",
            Option::<String>::None,
            package_path,
            now()
        ],
    )
    .expect("insert metadata");
    if let Some(status) = binding_status {
        conn.execute(
            "INSERT INTO plugin_bindings (id, package_id, status) VALUES (?1, ?2, ?3)",
            params![new_id(), package_id, status],
        )
        .expect("insert binding");
    }

    (
        AppState {
            conn: std::sync::Arc::new(std::sync::Mutex::new(conn)),
            app_dir,
            local_macs: vec![],
        },
        package_id,
        package_dir,
    )
}

#[test]
fn normalize_categories_cleans_names_and_duplicate_order() {
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
        doc.items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["backend", "public", "yy"]
    );
    assert_eq!(doc.items[1].name, "Public");
    assert_eq!(
        doc.items.iter().map(|item| item.order).collect::<Vec<_>>(),
        vec![10, 20, 30]
    );
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
fn draft_status_treats_missing_publish_target_as_pending() {
    let meta = PublishMeta {
        namespace: "community".to_string(),
        skill_id: "demo".to_string(),
        version: None,
        name: "Demo".to_string(),
        summary: "Demo skill".to_string(),
        tags: vec![],
        targets: vec![],
        levels: vec!["personal".to_string()],
        publish_scope: "project".to_string(),
        publish_category_slug: None,
        publish_project_slug: None,
        changelog: String::new(),
        updated_at: None,
        updated_by: None,
    };

    assert!(validation::is_publish_meta_ready_for_status(&meta));
    assert!(!validation::is_publish_meta_complete(&meta));
    assert_eq!(
        draft_status(Some("1.0.0"), None, None, Some(&meta), None),
        "待发布"
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
fn metadata_tags_take_priority_over_top_level_tags() {
    let content = r#"---
name: Metadata Tags Test
description: Metadata tags should win
tags: legacy, draft
metadata:
  version: 2.0.0
  author: Test Author
  tags:
    - curated
    - verified
---"#;

    let metadata = parse_skill_frontmatter(content);
    assert_eq!(metadata.tags, vec!["curated", "verified"]);
    assert_eq!(metadata.version.as_deref(), Some("2.0.0"));
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
        let local_macs = admin_config::local_mac_addresses();
        admin::unlock_admin_mode_inner(
            AdminUnlockRequest {
                admin_key: admin_key.clone(),
            },
            &local_macs,
        )
        .await
        .expect("admin mode should unlock against live MinIO");

        let project = MarketProject {
            slug: "live-project".to_string(),
            name: "Live Project".to_string(),
            description: "Created by live MinIO integration test".to_string(),
            order: 10,
            created_at: None,
            updated_at: None,
            updated_by: None,
        };
        let client = object_store::AdminObjectClient::new();
        let mut projects = load_remote_projects(&client).await.expect("load projects");
        projects.retain(|item| item.slug != project.slug);
        projects.push(project);
        save_remote_projects(&client, &projects)
            .await
            .expect("save projects");

        let source_path = "product/minio-live-draft".to_string();
        let drafts = admin::list_admin_drafts_inner(&admin_key, &local_macs)
            .await
            .expect("drafts should list");
        assert!(
            drafts
                .iter()
                .any(|draft| draft.gitlab_source_path == source_path
                    && draft.version.as_deref() == Some("0.1.0")),
            "live draft should be visible"
        );

        let meta = PublishMeta {
            namespace: "live".to_string(),
            skill_id: "minio-live-draft".to_string(),
            version: Some("0.1.0".to_string()),
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

        admin::save_publish_meta_inner(
            SavePublishMetaRequest {
                admin_key: admin_key.clone(),
                gitlab_source_path: source_path.clone(),
                meta,
                artifact_kind: None,
            },
            &local_macs,
        )
        .await
        .expect("save publish metadata");

        let preview = admin::preview_admin_draft_inner(
            AdminDraftPreviewRequest {
                admin_key: admin_key.clone(),
                gitlab_source_path: source_path.clone(),
                file_path: None,
            },
            &local_macs,
        )
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

        admin::publish_draft_inner(
            PublishDraftRequest {
                admin_key,
                gitlab_source_path: source_path,
            },
            &local_macs,
        )
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
        let local_macs = admin_config::local_mac_addresses();
        admin::unlock_admin_mode_inner(
            AdminUnlockRequest {
                admin_key: admin_key.clone(),
            },
            &local_macs,
        )
        .await
        .expect("admin mode should unlock against live MinIO");

        let client = object_store::AdminObjectClient::new();
        let project = MarketProject {
            slug: "live-project".to_string(),
            name: "Live Project".to_string(),
            description: "Created by live MinIO integration test".to_string(),
            order: 10,
            created_at: None,
            updated_at: None,
            updated_by: None,
        };
        let mut projects = load_remote_projects(&client).await.expect("load projects");
        projects.retain(|item| item.slug != project.slug);
        projects.push(project);
        save_remote_projects(&client, &projects)
            .await
            .expect("save projects");

        let source_path = "product/minio-live-draft".to_string();
        let meta = PublishMeta {
            namespace: "live".to_string(),
            skill_id: "minio-live-draft".to_string(),
            version: Some("0.1.0".to_string()),
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
        admin::save_publish_meta_inner(
            SavePublishMetaRequest {
                admin_key: admin_key.clone(),
                gitlab_source_path: source_path.clone(),
                meta,
                artifact_kind: None,
            },
            &local_macs,
        )
        .await
        .expect("save publish metadata");

        let catalog = load_remote_catalog(&client).await.expect("load catalog");
        let currently_listed = catalog
            .skills
            .iter()
            .any(|skill| skill.namespace == "live" && skill.id == "minio-live-draft");
        if !currently_listed {
            admin::publish_draft_inner(
                PublishDraftRequest {
                    admin_key: admin_key.clone(),
                    gitlab_source_path: source_path.clone(),
                },
                &local_macs,
            )
            .await
            .expect("publish existing draft before archive");
        }

        admin::archive_market_skill_inner(
            ArchiveMarketSkillRequest {
                admin_key: admin_key.clone(),
                namespace: "live".to_string(),
                skill_id: "minio-live-draft".to_string(),
                reason: Some("live integration archive test".to_string()),
            },
            &local_macs,
        )
        .await
        .expect("archive market skill");

        let archived_catalog = load_remote_catalog(&client)
            .await
            .expect("load catalog after archive");
        assert!(
            !archived_catalog
                .skills
                .iter()
                .any(|skill| skill.namespace == "live" && skill.id == "minio-live-draft"),
            "skill should disappear from market catalog after archive"
        );

        let drafts = admin::list_admin_drafts_inner(&admin_key, &local_macs)
            .await
            .expect("list drafts");
        assert!(
            drafts.iter().any(|draft| {
                draft.gitlab_source_path == source_path
                    && draft.status == "已下架"
                    && draft.source_available
            }),
            "archived GitLab draft should be visible in draft list"
        );

        admin::publish_draft_inner(
            PublishDraftRequest {
                admin_key,
                gitlab_source_path: source_path,
            },
            &local_macs,
        )
        .await
        .expect("republish archived existing version");

        let republished_catalog = load_remote_catalog(&client)
            .await
            .expect("load catalog after republish");
        assert!(
            republished_catalog
                .skills
                .iter()
                .any(|skill| skill.namespace == "live" && skill.id == "minio-live-draft"),
            "skill should return to market catalog after republish"
        );
    });
}
