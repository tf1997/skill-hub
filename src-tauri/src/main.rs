#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod admin_config;
mod db;
mod models;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let state = db::init_state(&app.handle())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::list_market_skills,
            commands::list_sources,
            commands::save_source,
            commands::unlock_admin_mode,
            commands::list_admin_drafts,
            commands::preview_admin_draft,
            commands::save_publish_meta,
            commands::save_market_project_remote,
            commands::delete_market_project_remote,
            commands::save_market_category_remote,
            commands::delete_market_category_remote,
            commands::archive_market_skill,
            commands::publish_draft,
            commands::quick_republish_archived_skill,
            commands::list_target_roots,
            commands::save_target_root,
            commands::refresh_catalog,
            commands::install_skill,
            commands::delete_cached_skill,
            commands::set_binding_enabled,
            commands::uninstall_binding,
            commands::list_projects,
            commands::save_project,
            commands::unbind_project,
            commands::scan_local_skills,
            commands::preview_skill,
            commands::list_update_candidates
        ])
        .run(tauri::generate_context!())
        .expect("error while running Skill Hub");
}
