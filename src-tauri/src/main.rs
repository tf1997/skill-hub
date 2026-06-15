#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod crash_report;
mod admin_config;
mod db;
mod minio_config;
mod models;
mod process_util;
mod updater;

use tauri::api::dialog;
use tauri::{CustomMenuItem, Manager, Menu, Submenu};

const MENU_CHECK_UPDATE: &str = "check_update";
const MENU_ABOUT: &str = "about";

fn app_menu() -> Menu {
    Menu::os_default("Skill Hub").add_submenu(Submenu::new(
        "帮助",
        Menu::new()
            .add_item(CustomMenuItem::new(MENU_CHECK_UPDATE, "检查更新"))
            .add_item(CustomMenuItem::new(MENU_ABOUT, "关于")),
    ))
}

fn show_about_dialog(window: &tauri::Window) {
    let body = format!(
        "{name}\n\n{description}\n\n开发维护：{authors}\n当前版本：{version}",
        name = env!("CARGO_PKG_NAME"),
        description = "Claude / Codex skills 的桌面市场、安装、更新与管理工具。",
        authors = env!("CARGO_PKG_AUTHORS"),
        version = env!("CARGO_PKG_VERSION")
    );

    dialog::message(Some(window), "关于", body);
}

fn main() {
    crash_report::install();
    updater::relaunch_latest_portable_if_needed();

    if let Err(error) = tauri::Builder::default()
        .menu(app_menu())
        .on_menu_event(|event| match event.menu_item_id() {
            MENU_CHECK_UPDATE => {
                updater::spawn_manual_update_check(event.window().app_handle().clone());
            }
            MENU_ABOUT => show_about_dialog(event.window()),
            _ => {}
        })
        .setup(|app| {
            let state = db::init_state(&app.handle())?;
            app.manage(state);
            updater::spawn_background_update_check(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::list_market_skills,
            commands::list_sources,
            commands::save_source,
            commands::unlock_admin_mode,
            commands::list_admin_drafts,
            commands::list_admin_audit_logs,
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
            commands::upgrade_skill_binding,
            commands::uninstall_binding,
            commands::list_projects,
            commands::save_project,
            commands::unbind_project,
            commands::scan_local_skills,
            commands::preview_skill,
            commands::list_update_candidates,
            updater::check_for_updates_command,
            updater::download_update_command,
            updater::restart_after_update_command
        ])
        .run(tauri::generate_context!())
    {
        crash_report::report_fatal_error("tauri run", &error.to_string());
        eprintln!("error while running Skill Hub: {error}");
        std::process::exit(1);
    }
}
