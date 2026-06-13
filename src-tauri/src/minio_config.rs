pub const COMPILED_SOURCE_ID: &str = "compiled-source";
pub const COMPILED_SOURCE_NAME: &str = "本地 MinIO";
pub const COMPILED_SOURCE_ENDPOINT: &str = match option_env!("SKILL_HUB_MINIO_ENDPOINT") {
    Some(value) => value,
    None => "http://192.168.1.4:9000",
};
pub const COMPILED_SOURCE_BUCKET: &str = match option_env!("SKILL_HUB_MINIO_BUCKET") {
    Some(value) => value,
    None => "skill-market",
};
pub const COMPILED_SOURCE_REGION: Option<&str> = option_env!("SKILL_HUB_MINIO_REGION");

pub const APP_UPDATE_MANIFEST_OBJECT: &str = "skill-hub/updates/stable/latest.json";

pub fn object_url(object_path: &str) -> String {
    format!(
        "{}/{}/{}",
        COMPILED_SOURCE_ENDPOINT.trim_end_matches('/'),
        COMPILED_SOURCE_BUCKET.trim_matches('/'),
        object_path.trim_start_matches('/')
    )
}

pub fn app_update_manifest_url() -> String {
    option_env!("SKILL_HUB_BUILT_IN_UPDATE_MANIFEST_URL")
        .map(ToString::to_string)
        .unwrap_or_else(|| object_url(APP_UPDATE_MANIFEST_OBJECT))
}
