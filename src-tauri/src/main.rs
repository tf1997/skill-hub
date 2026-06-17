#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod admin_config;
mod commands;
mod crash_report;
mod db;
mod minio_config;
mod models;
mod process_util;
mod updater;
mod webview_prereq;

use tauri::api::{dialog, shell};
use tauri::{CustomMenuItem, Manager, Menu, Submenu};

const MENU_CHECK_UPDATE: &str = "check_update";
const MENU_ABOUT: &str = "about";
const MENU_ONLINE_DOCS: &str = "online_docs";
const ONLINE_DOCS_URL: &str = "https://github.com";
const DEVELOPMENT_TEAM: &str = "Skill Hub Team";
const FEEDBACK_EMAIL: &str = "support@skill-hub.dev";

fn app_menu() -> Menu {
    Menu::os_default("Skill Hub").add_submenu(Submenu::new(
        "帮助",
        Menu::new()
            .add_item(CustomMenuItem::new(MENU_ONLINE_DOCS, "在线文档"))
            .add_item(CustomMenuItem::new(MENU_CHECK_UPDATE, "检查更新"))
            .add_item(CustomMenuItem::new(MENU_ABOUT, "关于")),
    ))
}

#[derive(Clone, serde::Serialize)]
struct AboutPayload {
    name: &'static str,
    description: &'static str,
    authors: &'static str,
    version: &'static str,
    docs_url: &'static str,
    team: &'static str,
    feedback_email: &'static str,
}

fn show_about_window(window: &tauri::Window) {
    let payload = AboutPayload {
        name: env!("CARGO_PKG_NAME"),
        description: "Claude / Codex skills 的桌面市场、安装、更新与管理工具。",
        authors: env!("CARGO_PKG_AUTHORS"),
        version: env!("CARGO_PKG_VERSION"),
        docs_url: ONLINE_DOCS_URL,
        team: DEVELOPMENT_TEAM,
        feedback_email: FEEDBACK_EMAIL,
    };

    let _ = window.emit("show-about", payload);
}

fn main() {
    crash_report::install();
    updater::relaunch_latest_portable_if_needed();
    webview_prereq::ensure_webview2_runtime_or_exit();

    if let Err(error) = tauri::Builder::default()
        .menu(app_menu())
        .on_menu_event(|event| match event.menu_item_id() {
            MENU_CHECK_UPDATE => {
                let _ = event.window().emit("open-app-update", ());
            }
            MENU_ABOUT => show_about_window(event.window()),
            MENU_ONLINE_DOCS => {
                if let Err(error) = shell::open(
                    &event.window().shell_scope(),
                    ONLINE_DOCS_URL.to_string(),
                    None,
                ) {
                    dialog::message(
                        Some(event.window()),
                        "打开在线文档失败",
                        format!("无法打开在线文档：{error}"),
                    );
                }
            }
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
            commands::delete_local_skill,
            commands::set_local_skill_enabled,
            commands::import_local_skill_to_cache,
            commands::install_cached_skill,
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
