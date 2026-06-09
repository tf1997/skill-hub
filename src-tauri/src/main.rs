#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
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
            commands::list_target_roots,
            commands::save_target_root,
            commands::refresh_catalog,
            commands::install_skill,
            commands::delete_cached_skill,
            commands::set_binding_enabled,
            commands::uninstall_binding,
            commands::list_projects,
            commands::save_project,
            commands::scan_local_skills,
            commands::preview_skill,
            commands::list_update_candidates
        ])
        .run(tauri::generate_context!())
        .expect("error while running Skill Hub");
}
