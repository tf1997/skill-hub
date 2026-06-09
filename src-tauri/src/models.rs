use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBootstrap {
    pub sources: Vec<Source>,
    pub categories: Vec<Category>,
    pub skills: Vec<MarketSkill>,
    pub bindings: Vec<SkillBinding>,
    pub cached_packages: Vec<CachedSkillPackage>,
    pub projects: Vec<Project>,
    pub target_roots: Vec<TargetRoot>,
    pub updates: Vec<UpdateCandidate>,
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
    pub generated_at: Option<String>,
    pub categories: Vec<String>,
    pub skills: Vec<MarketSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoriesDoc {
    pub schema: String,
    pub generated_at: Option<String>,
    pub items: Vec<Category>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketSkill {
    pub namespace: String,
    pub id: String,
    pub name: String,
    pub summary: String,
    pub latest_version: String,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default)]
    pub levels: Vec<String>,
    pub manifest_path: String,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub installed_bindings: Vec<SkillBinding>,
    #[serde(default)]
    pub cached_versions: Vec<String>,
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
    pub latest_version: String,
    #[serde(default)]
    pub versions: Vec<SkillVersion>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillVersion {
    pub version: String,
    pub skill_path: String,
    pub package_path: String,
    pub sha256_path: String,
    pub changelog_path: Option<String>,
    pub signature_path: Option<String>,
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
pub struct DeleteCachedSkillRequest {
    pub source_id: Option<String>,
    pub namespace: String,
    pub skill_id: String,
    pub version: String,
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
    pub scanned_at: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPreview {
    pub title: String,
    pub root_path: String,
    pub origin: String,
    pub files: Vec<SkillPreviewFile>,
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
pub struct UpdateCandidate {
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
