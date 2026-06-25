use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBootstrap {
    pub sources: Vec<Source>,
    pub categories: Vec<Category>,
    pub skills: Vec<MarketSkill>,
    pub plugins: Vec<MarketPlugin>,
    pub market_projects: Vec<MarketProject>,
    pub bindings: Vec<SkillBinding>,
    pub cached_packages: Vec<CachedSkillPackage>,
    pub plugin_packages: Vec<CachedPluginPackage>,
    pub plugin_bindings: Vec<PluginBinding>,
    pub local_plugins: Vec<LocalPlugin>,
    pub local_skills: Vec<LocalSkill>,
    pub projects: Vec<Project>,
    pub target_roots: Vec<TargetRoot>,
    pub updates: Vec<UpdateCandidate>,
    pub metadata_sync_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub bucket: String,
    pub region: Option<String>,
    pub enabled: bool,
    pub last_sync_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSourceRequest {
    pub id: Option<String>,
    pub name: String,
    pub endpoint: String,
    pub bucket: String,
    pub region: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUnlockRequest {
    pub admin_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminSession {
    pub enabled: bool,
    pub endpoint: String,
    pub bucket: String,
    pub region: Option<String>,
    pub role: String,
    pub projects: Vec<String>,
    pub mac_address: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAdminAuditLogsRequest {
    pub admin_key: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminAuditLog {
    pub object_path: String,
    pub action: String,
    pub actor: Option<String>,
    pub role: Option<String>,
    pub mac_address: Option<String>,
    pub ip_address: Option<String>,
    pub target: Option<String>,
    pub summary: String,
    pub created_at: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRoot {
    pub target: String,
    pub personal_path: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTargetRootRequest {
    pub target: String,
    pub personal_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
    pub order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDoc {
    pub schema: String,
    #[serde(alias = "generated_at")]
    pub generated_at: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    pub skills: Vec<MarketSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoriesDoc {
    pub schema: String,
    #[serde(alias = "generated_at")]
    pub generated_at: Option<String>,
    #[serde(default)]
    pub items: Vec<Category>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsDoc {
    pub schema: String,
    #[serde(default, alias = "generated_at")]
    pub generated_at: Option<String>,
    #[serde(default)]
    pub projects: Vec<MarketProject>,
    #[serde(default)]
    pub items: Vec<MarketProject>,
}

impl ProjectsDoc {
    pub fn into_projects(self) -> Vec<MarketProject> {
        if self.projects.is_empty() {
            self.items
        } else {
            self.projects
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketProject {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub order: i64,
    #[serde(default, alias = "created_at")]
    pub created_at: Option<String>,
    #[serde(default, alias = "updated_at")]
    pub updated_at: Option<String>,
    #[serde(default, alias = "updated_by")]
    pub updated_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketSkill {
    pub namespace: String,
    pub id: String,
    pub name: String,
    pub summary: String,
    #[serde(alias = "latest_version")]
    pub latest_version: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default)]
    pub levels: Vec<String>,
    #[serde(alias = "manifest_path")]
    pub manifest_path: String,
    #[serde(alias = "updated_at")]
    pub updated_at: Option<String>,
    #[serde(default, alias = "source_id")]
    pub source_id: Option<String>,
    #[serde(default)]
    pub installed_bindings: Vec<SkillBinding>,
    #[serde(default)]
    pub cached_versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCatalogDoc {
    pub schema: String,
    #[serde(default, alias = "generated_at")]
    pub generated_at: Option<String>,
    #[serde(default)]
    pub plugins: Vec<MarketPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketPlugin {
    pub namespace: String,
    pub id: String,
    pub name: String,
    pub summary: String,
    #[serde(alias = "latest_version")]
    pub latest_version: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default, alias = "risk_level")]
    pub risk_level: String,
    #[serde(alias = "manifest_path")]
    pub manifest_path: String,
    #[serde(default, alias = "updated_at")]
    pub updated_at: Option<String>,
    #[serde(default, alias = "source_id")]
    pub source_id: Option<String>,
    #[serde(default)]
    pub installed_bindings: Vec<PluginBinding>,
    #[serde(default)]
    pub cached_versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub schema: String,
    pub namespace: String,
    pub id: String,
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default, alias = "risk_level")]
    pub risk_level: String,
    #[serde(alias = "latest_version")]
    pub latest_version: String,
    #[serde(default)]
    pub versions: Vec<PluginVersion>,
    #[serde(default, alias = "updated_at")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginVersion {
    pub version: String,
    #[serde(alias = "plugin_path")]
    pub plugin_path: String,
    #[serde(default)]
    pub packages: PluginVersionPackages,
    #[serde(default, alias = "component_inventory_path")]
    pub component_inventory_path: Option<String>,
    #[serde(default, alias = "risk_report_path")]
    pub risk_report_path: Option<String>,
    #[serde(default, alias = "changelog_path")]
    pub changelog_path: Option<String>,
    #[serde(default, alias = "created_at")]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginVersionPackages {
    pub codex: Option<PluginPackageRef>,
    pub claude: Option<PluginPackageRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPackageRef {
    #[serde(alias = "package_path")]
    pub package_path: String,
    #[serde(alias = "sha256_path")]
    pub sha256_path: String,
    #[serde(default, alias = "signature_path")]
    pub signature_path: Option<String>,
    #[serde(default)]
    pub package: Option<PackageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedPluginPackage {
    pub source_id: Option<String>,
    pub namespace: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub version: String,
    pub target: String,
    pub package_path: String,
    pub cached_at: String,
    pub risk_level: String,
    pub component_inventory_json: String,
    pub binding_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginBinding {
    pub id: String,
    pub package_id: String,
    pub source_id: Option<String>,
    pub namespace: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub version: String,
    pub target: String,
    pub scope: String,
    pub project_path: Option<String>,
    pub marketplace_id: Option<String>,
    pub marketplace_name: String,
    pub platform_ref: String,
    pub enabled: bool,
    pub install_mode: String,
    pub update_policy: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPlugin {
    pub id: String,
    pub target: String,
    pub scope: String,
    pub project_path: Option<String>,
    pub path: String,
    pub marketplace_name: Option<String>,
    pub plugin_id: Option<String>,
    pub version: Option<String>,
    pub enabled: bool,
    pub status: String,
    pub component_inventory_json: String,
    pub managed_by_skillhub: bool,
    pub scanned_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillManifest {
    pub schema: String,
    pub namespace: String,
    pub id: String,
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default)]
    pub levels: Vec<String>,
    #[serde(alias = "latest_version")]
    pub latest_version: String,
    #[serde(default)]
    pub versions: Vec<SkillVersion>,
    #[serde(alias = "updated_at")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillVersion {
    pub version: String,
    #[serde(alias = "skill_path")]
    pub skill_path: String,
    #[serde(alias = "package_path")]
    pub package_path: String,
    #[serde(alias = "sha256_path")]
    pub sha256_path: String,
    #[serde(alias = "changelog_path")]
    pub changelog_path: Option<String>,
    #[serde(alias = "signature_path")]
    pub signature_path: Option<String>,
    #[serde(alias = "created_at")]
    pub created_at: Option<String>,
    pub package: Option<PackageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageInfo {
    pub file: String,
    pub sha256: String,
    pub size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSkillRequest {
    pub source_id: Option<String>,
    pub namespace: String,
    pub skill_id: String,
    pub version: Option<String>,
    pub target: String,
    pub level: String,
    pub project_path: Option<String>,
    pub install_mode: Option<String>,
    pub update_policy: Option<String>,
    pub enable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPluginRequest {
    pub source_id: Option<String>,
    pub namespace: String,
    pub plugin_id: String,
    pub version: Option<String>,
    pub target: String,
    pub scope: String,
    pub project_path: Option<String>,
    pub install_mode: Option<String>,
    pub update_policy: Option<String>,
    pub enable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportLocalSkillRequest {
    pub path: String,
    pub skill_id: Option<String>,
    pub version: Option<String>,
    pub overwrite: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallCachedSkillRequest {
    pub source_id: Option<String>,
    pub namespace: String,
    pub skill_id: String,
    pub version: String,
    pub target: String,
    pub level: String,
    pub project_path: Option<String>,
    pub install_mode: Option<String>,
    pub update_policy: Option<String>,
    pub enable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCachedSkillRequest {
    pub source_id: Option<String>,
    pub namespace: String,
    pub skill_id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteLocalSkillRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedSkillPackage {
    pub source_id: Option<String>,
    pub namespace: String,
    pub skill_id: String,
    pub skill_name: String,
    pub version: String,
    pub package_path: String,
    pub cached_at: String,
    pub binding_count: i64,
    #[serde(default)]
    pub origin: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillBinding {
    pub id: String,
    pub package_id: String,
    pub source_id: Option<String>,
    pub namespace: String,
    pub skill_id: String,
    pub skill_name: String,
    pub version: String,
    pub target: String,
    pub level: String,
    pub project_path: Option<String>,
    pub install_path: String,
    pub enabled: bool,
    pub install_mode: String,
    pub update_policy: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetBindingEnabledRequest {
    pub binding_id: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeBindingRequest {
    pub binding_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProjectRequest {
    pub id: Option<String>,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSkill {
    pub id: String,
    pub target: String,
    pub level: String,
    pub project_path: Option<String>,
    pub path: String,
    pub detected_manifest: Option<String>,
    pub managed_by_skillhub: bool,
    pub status: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub scanned_at: String,
    pub origin: String,
    pub skill_id: Option<String>,
    pub version: Option<String>,
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub matched_source_id: Option<String>,
    pub matched_namespace: Option<String>,
    pub matched_skill_id: Option<String>,
    pub matched_version: Option<String>,
    #[serde(default)]
    pub can_import_to_cache: bool,
    #[serde(default)]
    pub can_restore_binding: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLocalSkillEnabledRequest {
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPreviewRequest {
    pub source_id: Option<String>,
    pub namespace: Option<String>,
    pub skill_id: Option<String>,
    pub version: Option<String>,
    pub binding_id: Option<String>,
    pub path: Option<String>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPreviewRequest {
    pub source_id: Option<String>,
    pub namespace: Option<String>,
    pub plugin_id: Option<String>,
    pub version: Option<String>,
    pub target: Option<String>,
    pub binding_id: Option<String>,
    pub path: Option<String>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminDraftPreviewRequest {
    pub admin_key: String,
    pub gitlab_source_path: String,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPreview {
    pub title: String,
    pub root_path: String,
    pub origin: String,
    pub files: Vec<SkillPreviewFile>,
    #[serde(default)]
    pub file_list: Vec<SkillPreviewFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPreviewFile {
    pub path: String,
    pub language: String,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPreviewFileEntry {
    pub path: String,
    pub language: String,
    pub previewable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCandidate {
    #[serde(default = "default_update_kind")]
    pub kind: String,
    pub binding_id: String,
    pub namespace: String,
    pub skill_id: String,
    pub skill_name: String,
    pub target: String,
    pub level: String,
    pub project_path: Option<String>,
    pub current_version: String,
    pub latest_version: String,
    pub update_policy: String,
    pub blocked_reason: Option<String>,
}

fn default_update_kind() -> String {
    "skill".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminDraftSkill {
    pub gitlab_source_path: String,
    pub draft_slug: Option<String>,
    pub gitlab_category_code: Option<String>,
    #[serde(default)]
    pub gitlab_category_path: Vec<String>,
    #[serde(default)]
    pub source_available: bool,
    pub version: Option<String>,
    pub author: Option<String>,
    pub status: String,
    pub validation_status: Option<String>,
    pub publish_meta: Option<PublishMeta>,
    pub published_version: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminDraftPlugin {
    pub gitlab_source_path: String,
    pub draft_slug: Option<String>,
    #[serde(default)]
    pub gitlab_category_path: Vec<String>,
    #[serde(default)]
    pub source_available: bool,
    #[serde(default)]
    pub readme_metadata_complete: bool,
    pub namespace: Option<String>,
    pub plugin_id: Option<String>,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default)]
    pub risk_level: Option<String>,
    pub status: String,
    pub validation_status: Option<String>,
    pub publish_meta: Option<PublishMeta>,
    pub published_version: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PluginSourceMeta {
    pub schema: String,
    #[serde(default)]
    pub namespace: String,
    #[serde(default, alias = "plugin_id")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default, alias = "risk_level")]
    pub risk_level: Option<String>,
    #[serde(default, alias = "publish_scope")]
    pub publish_scope: Option<String>,
    #[serde(default, alias = "publish_project_slug")]
    pub publish_project_slug: Option<String>,
    #[serde(default)]
    pub platforms: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PublishMeta {
    pub namespace: String,
    #[serde(alias = "skill_id")]
    pub skill_id: String,
    #[serde(default)]
    pub version: Option<String>,
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub levels: Vec<String>,
    #[serde(default, alias = "publish_scope")]
    pub publish_scope: String,
    #[serde(default, alias = "publish_category_slug")]
    pub publish_category_slug: Option<String>,
    #[serde(default, alias = "publish_project_slug")]
    pub publish_project_slug: Option<String>,
    #[serde(default)]
    pub changelog: String,
    #[serde(default, alias = "updated_at")]
    pub updated_at: Option<String>,
    #[serde(default, alias = "updated_by")]
    pub updated_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePublishMetaRequest {
    pub admin_key: String,
    pub gitlab_source_path: String,
    pub meta: PublishMeta,
    #[serde(default, alias = "artifact_kind")]
    pub artifact_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMarketProjectRequest {
    pub admin_key: String,
    pub project: MarketProject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMarketProjectRequest {
    pub admin_key: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMarketCategoryRequest {
    pub admin_key: String,
    pub category: Category,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMarketCategoryRequest {
    pub admin_key: String,
    pub category_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMarketSkillRequest {
    pub admin_key: String,
    pub namespace: String,
    pub skill_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMarketPluginRequest {
    pub admin_key: String,
    pub namespace: String,
    pub plugin_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishDraftRequest {
    pub admin_key: String,
    pub gitlab_source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishPluginDraftRequest {
    pub admin_key: String,
    pub gitlab_source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPluginBindingEnabledRequest {
    pub binding_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallPluginRequest {
    pub binding_id: String,
    #[serde(default)]
    pub delete_cached_package: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradePluginBindingRequest {
    pub binding_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickRepublishRequest {
    pub admin_key: String,
    pub gitlab_source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl CommandError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_camel_case_minio_metadata() {
        let catalog: CatalogDoc = serde_json::from_str(
            r#"{
              "schema": "skillhub.catalog.v1",
              "generatedAt": "2026-06-10T00:00:00Z",
              "skills": [{
                "namespace": "validation",
                "id": "metadata-probe",
                "name": "Metadata Probe",
                "summary": "Validate metadata.",
                "latestVersion": "1.0.0",
                "manifestPath": "skills/validation/metadata-probe/manifest.json",
                "updatedAt": "2026-06-10T00:00:00Z"
              }]
            }"#,
        )
        .expect("catalog should parse");

        assert_eq!(catalog.skills[0].latest_version, "1.0.0");
        assert_eq!(
            catalog.skills[0].manifest_path,
            "skills/validation/metadata-probe/manifest.json"
        );

        let manifest: SkillManifest = serde_json::from_str(
            r#"{
              "schema": "skillhub.skill-manifest.v1",
              "namespace": "validation",
              "id": "metadata-probe",
              "name": "Metadata Probe",
              "summary": "Validate metadata.",
              "latestVersion": "1.0.0",
              "versions": [{
                "version": "1.0.0",
                "skillPath": "skills/validation/metadata-probe/versions/1.0.0/skill.json",
                "packagePath": "skills/validation/metadata-probe/versions/1.0.0/package.zip",
                "sha256Path": "skills/validation/metadata-probe/versions/1.0.0/package.sha256"
              }]
            }"#,
        )
        .expect("manifest should parse");

        assert_eq!(manifest.latest_version, "1.0.0");
        assert_eq!(
            manifest.versions[0].package_path,
            "skills/validation/metadata-probe/versions/1.0.0/package.zip"
        );
    }

    #[test]
    fn project_status_is_ignored_when_rewriting_projects_doc() {
        let doc: ProjectsDoc = serde_json::from_str(
            r#"{
              "schema": "skillhub.projects.v1",
              "projects": [{
                "slug": "archive-demo",
                "name": "Archive Demo",
                "description": "Legacy archived project",
                "status": "archived"
              }]
            }"#,
        )
        .expect("legacy project status should not break parsing");

        let projects = doc.into_projects();
        assert_eq!(projects[0].slug, "archive-demo");

        let rewritten = serde_json::to_value(ProjectsDoc {
            schema: "skillhub.projects.v1".to_string(),
            generated_at: None,
            projects,
            items: Vec::new(),
        })
        .expect("projects doc should serialize");
        assert!(rewritten["projects"][0].get("status").is_none());
    }

    #[test]
    fn parses_legacy_snake_case_minio_metadata() {
        let catalog: CatalogDoc = serde_json::from_str(
            r#"{
              "schema": "skillhub.catalog.v1",
              "generated_at": "2026-06-10T00:00:00Z",
              "categories": ["frontend"],
              "skills": [{
                "namespace": "validation",
                "id": "metadata-probe",
                "name": "Metadata Probe",
                "summary": "Validate metadata.",
                "latest_version": "1.0.0",
                "manifest_path": "skills/validation/metadata-probe/manifest.json",
                "updated_at": "2026-06-10T00:00:00Z"
              }]
            }"#,
        )
        .expect("legacy catalog should parse");

        assert_eq!(catalog.skills[0].latest_version, "1.0.0");

        let manifest: SkillManifest = serde_json::from_str(
            r#"{
              "schema": "skillhub.skill-manifest.v1",
              "namespace": "validation",
              "id": "metadata-probe",
              "name": "Metadata Probe",
              "summary": "Validate metadata.",
              "latest_version": "1.0.0",
              "versions": [{
                "version": "1.0.0",
                "skill_path": "skills/validation/metadata-probe/versions/1.0.0/skill.json",
                "package_path": "skills/validation/metadata-probe/versions/1.0.0/package.zip",
                "sha256_path": "skills/validation/metadata-probe/versions/1.0.0/package.sha256",
                "created_at": "2026-06-10T00:00:00Z"
              }],
              "updated_at": "2026-06-10T00:00:00Z"
            }"#,
        )
        .expect("legacy manifest should parse");

        assert_eq!(
            manifest.versions[0].sha256_path,
            "skills/validation/metadata-probe/versions/1.0.0/package.sha256"
        );
    }
}
