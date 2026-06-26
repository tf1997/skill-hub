use std::{fs, io::Cursor, path::PathBuf};

use anyhow::Result;
use rusqlite::params;
use tauri::State;
use zip::ZipArchive;

use crate::{
    admin_config,
    db::{app_bootstrap, canonical_display_path, new_id, now, AppState},
    models::{
        AdminAuditLog, AdminDraftPlugin, AdminDraftPreviewRequest, AdminDraftSkill, AdminSession,
        AdminUnlockRequest, AppBootstrap, ArchiveMarketPluginRequest, ArchiveMarketSkillRequest,
        CachedSkillPackage, CatalogDoc, CategoriesDoc, Category, CommandError,
        DeleteCachedPluginRequest, DeleteCachedSkillRequest, DeleteLocalSkillRequest,
        DeleteMarketCategoryRequest, DeleteMarketProjectRequest, ImportLocalSkillRequest,
        InstallCachedSkillRequest, InstallPluginRequest, InstallSkillRequest,
        ListAdminAuditLogsRequest, LocalPlugin, LocalSkill, MarketPlugin, MarketProject,
        MarketSkill, PluginBinding, PluginPreviewRequest, PluginSourceMeta, Project,
        PublishDraftRequest, PublishMeta, PublishPluginDraftRequest, QuickRepublishRequest,
        SaveMarketCategoryRequest, SaveMarketProjectRequest, SaveProjectRequest,
        SavePublishMetaRequest, SaveSourceRequest, SaveTargetRootRequest, SetBindingEnabledRequest,
        SetLocalSkillEnabledRequest, SetPluginBindingEnabledRequest, SkillBinding, SkillManifest,
        SkillPreview, SkillPreviewRequest, SkillVersion, Source, TargetRoot,
        UninstallPluginRequest, UpdateCandidate, UpgradeBindingRequest,
        UpgradePluginBindingRequest,
    },
    services::{admin, install, local, market, preview},
};

type CommandResult<T> = std::result::Result<T, CommandError>;

#[tauri::command]
pub async fn bootstrap(state: State<'_, AppState>) -> CommandResult<AppBootstrap> {
    let metadata_sync_error = market::refresh_catalog_best_effort(&state).await;
    map_result(app_bootstrap(&state, metadata_sync_error))
}

#[tauri::command]
pub async fn list_market_skills(state: State<'_, AppState>) -> CommandResult<Vec<MarketSkill>> {
    map_result(market::list_market_skills(&state).await)
}

#[tauri::command]
pub async fn list_market_plugins(state: State<'_, AppState>) -> CommandResult<Vec<MarketPlugin>> {
    map_result(market::list_market_plugins(&state).await)
}

#[tauri::command]
pub async fn list_sources(state: State<'_, AppState>) -> CommandResult<Vec<Source>> {
    map_result(market::list_sources(&state))
}

#[tauri::command]
pub async fn save_source(
    request: SaveSourceRequest,
    state: State<'_, AppState>,
) -> CommandResult<Source> {
    map_result(market::save_source(request, &state))
}

#[tauri::command]
pub async fn unlock_admin_mode(
    request: AdminUnlockRequest,
    state: State<'_, AppState>,
) -> CommandResult<AdminSession> {
    map_result(admin::unlock_admin_mode_inner(request, &state.local_macs).await)
}
#[tauri::command]
pub async fn list_admin_drafts(
    admin_key: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<AdminDraftSkill>> {
    map_result(admin::list_admin_drafts_inner(&admin_key, &state.local_macs).await)
}

#[tauri::command]
pub async fn list_admin_plugin_drafts(
    admin_key: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<AdminDraftPlugin>> {
    map_result(admin::list_admin_plugin_drafts_inner(&admin_key, &state.local_macs).await)
}

#[tauri::command]
pub async fn list_admin_audit_logs(
    request: ListAdminAuditLogsRequest,
    state: State<'_, AppState>,
) -> CommandResult<Vec<AdminAuditLog>> {
    map_result(admin::list_admin_audit_logs_inner(request, &state.local_macs).await)
}

#[tauri::command]
pub async fn preview_admin_draft(
    request: AdminDraftPreviewRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillPreview> {
    map_result(admin::preview_admin_draft_inner(request, &state.local_macs).await)
}

#[tauri::command]
pub async fn preview_admin_plugin_draft(
    request: AdminDraftPreviewRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillPreview> {
    map_result(admin::preview_admin_plugin_draft_inner(request, &state.local_macs).await)
}

#[tauri::command]
pub async fn save_publish_meta(
    request: SavePublishMetaRequest,
    state: State<'_, AppState>,
) -> CommandResult<PublishMeta> {
    map_result(admin::save_publish_meta_inner(request, &state.local_macs).await)
}

#[tauri::command]
pub async fn save_market_project_remote(
    request: SaveMarketProjectRequest,
    state: State<'_, AppState>,
) -> CommandResult<Vec<MarketProject>> {
    map_result(admin::save_market_project_remote_inner(request, &state, &state.local_macs).await)
}

#[tauri::command]
pub async fn delete_market_project_remote(
    request: DeleteMarketProjectRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        admin::delete_market_project_remote_inner(request, &state, &state.local_macs).await?;
        market::refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn save_market_category_remote(
    request: SaveMarketCategoryRequest,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Category>> {
    map_result(admin::save_market_category_remote_inner(request, &state.local_macs).await)
}

#[tauri::command]
pub async fn delete_market_category_remote(
    request: DeleteMarketCategoryRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        admin::delete_market_category_remote_inner(request, &state.local_macs).await?;
        market::refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn archive_market_skill(
    request: ArchiveMarketSkillRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        admin::archive_market_skill_inner(request, &state.local_macs).await?;
        market::refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn archive_market_plugin(
    request: ArchiveMarketPluginRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        admin::archive_market_plugin_inner(request, &state.local_macs).await?;
        market::refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn publish_draft(
    request: PublishDraftRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        admin::publish_draft_inner(request, &state.local_macs).await?;
        market::refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn publish_plugin_draft(
    request: PublishPluginDraftRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        admin::publish_plugin_draft_inner(request, &state.local_macs).await?;
        market::refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn quick_republish_archived_skill(
    request: QuickRepublishRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        admin::quick_republish_archived_skill_inner(request, &state.local_macs).await?;
        market::refresh_catalog_inner(&state).await?;
        app_bootstrap(&state, None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn list_target_roots(state: State<'_, AppState>) -> CommandResult<Vec<TargetRoot>> {
    map_result(market::list_target_roots(&state))
}

#[tauri::command]
pub async fn save_target_root(
    request: SaveTargetRootRequest,
    state: State<'_, AppState>,
) -> CommandResult<TargetRoot> {
    map_result(market::save_target_root(request, &state))
}

#[tauri::command]
pub async fn refresh_catalog(state: State<'_, AppState>) -> CommandResult<AppBootstrap> {
    map_result(market::refresh_app_bootstrap(&state).await)
}

#[tauri::command]
pub async fn install_skill(
    request: InstallSkillRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillBinding> {
    map_result(install::install_skill_inner(request, &state).await)
}

#[tauri::command]
pub async fn install_plugin(
    request: InstallPluginRequest,
    state: State<'_, AppState>,
) -> CommandResult<PluginBinding> {
    map_result(install::install_plugin_inner(request, &state).await)
}

#[tauri::command]
pub async fn delete_cached_skill(
    request: DeleteCachedSkillRequest,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    map_result(install::delete_cached_skill_inner(request, &state))
}

#[tauri::command]
pub async fn delete_cached_plugin(
    request: DeleteCachedPluginRequest,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    map_result(install::delete_cached_plugin_inner(request, &state))
}

#[tauri::command]
pub async fn delete_local_skill(
    request: DeleteLocalSkillRequest,
    state: State<'_, AppState>,
) -> CommandResult<Vec<LocalSkill>> {
    map_result(install::delete_local_skill_inner(request, &state))
}

#[tauri::command]
pub async fn set_local_skill_enabled(
    request: SetLocalSkillEnabledRequest,
    state: State<'_, AppState>,
) -> CommandResult<Vec<LocalSkill>> {
    map_result(install::set_local_skill_enabled_inner(request, &state))
}

#[tauri::command]
pub async fn import_local_skill_to_cache(
    request: ImportLocalSkillRequest,
    state: State<'_, AppState>,
) -> CommandResult<CachedSkillPackage> {
    map_result(install::import_local_skill_to_cache_inner(request, &state))
}

#[tauri::command]
pub async fn install_cached_skill(
    request: InstallCachedSkillRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillBinding> {
    map_result(install::install_cached_skill_inner(request, &state))
}

#[tauri::command]
pub async fn set_binding_enabled(
    request: SetBindingEnabledRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillBinding> {
    map_result(install::set_binding_enabled_inner(request, &state))
}

#[tauri::command]
pub async fn upgrade_skill_binding(
    request: UpgradeBindingRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        install::upgrade_skill_binding_inner(request, state.inner()).await?;
        app_bootstrap(state.inner(), None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn upgrade_plugin_binding(
    request: UpgradePluginBindingRequest,
    state: State<'_, AppState>,
) -> CommandResult<AppBootstrap> {
    let result = async {
        install::upgrade_plugin_binding_inner(request, state.inner()).await?;
        market::refresh_catalog_inner(state.inner()).await?;
        app_bootstrap(state.inner(), None)
    }
    .await;
    map_result(result)
}

#[tauri::command]
pub async fn uninstall_binding(
    binding_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<SkillBinding>> {
    map_result(install::uninstall_binding_inner(binding_id, &state))
}

#[tauri::command]
pub async fn set_plugin_binding_enabled(
    request: SetPluginBindingEnabledRequest,
    state: State<'_, AppState>,
) -> CommandResult<PluginBinding> {
    map_result(install::set_plugin_binding_enabled_inner(request, &state))
}

#[tauri::command]
pub async fn uninstall_plugin(
    request: UninstallPluginRequest,
    state: State<'_, AppState>,
) -> CommandResult<Vec<PluginBinding>> {
    map_result(install::uninstall_plugin_inner(request, &state))
}

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> CommandResult<Vec<Project>> {
    map_result(market::list_projects(&state))
}

#[tauri::command]
pub async fn save_project(
    request: SaveProjectRequest,
    state: State<'_, AppState>,
) -> CommandResult<Project> {
    map_result(market::save_project(request, &state))
}

#[tauri::command]
pub async fn unbind_project(
    project_id: String,
    state: State<'_, AppState>,
) -> CommandResult<Vec<Project>> {
    map_result(market::unbind_project(project_id, &state))
}

#[tauri::command]
pub async fn scan_local_skills(state: State<'_, AppState>) -> CommandResult<Vec<LocalSkill>> {
    map_result(local::scan_local_skills(&state))
}

#[tauri::command]
pub async fn scan_local_plugins(state: State<'_, AppState>) -> CommandResult<Vec<LocalPlugin>> {
    map_result(local::scan_local_plugins(&state))
}

#[tauri::command]
pub async fn preview_skill(
    request: SkillPreviewRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillPreview> {
    map_result(preview::preview_skill_inner(request, &state).await)
}

#[tauri::command]
pub async fn preview_plugin(
    request: PluginPreviewRequest,
    state: State<'_, AppState>,
) -> CommandResult<SkillPreview> {
    map_result(preview::preview_plugin_inner(request, &state).await)
}

#[tauri::command]
pub async fn list_update_candidates(
    state: State<'_, AppState>,
) -> CommandResult<Vec<UpdateCandidate>> {
    map_result(market::list_update_candidates(&state).await)
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;

fn map_result<T>(result: Result<T>) -> CommandResult<T> {
    result.map_err(|error| CommandError::new("COMMAND_FAILED", error.to_string()))
}
