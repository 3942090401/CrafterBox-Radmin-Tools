mod broadcast;
mod commands;
mod config;
mod log;
mod networks;
mod radmin;
mod system_registry;
mod win_api;

use broadcast::{BroadcastState, PublicBroadcastState, ScanState};
use log::LogState;
use std::sync::{Arc, Mutex};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(all(target_os = "windows", not(debug_assertions)))]
    if !win_api::is_admin() {
        match win_api::run_as_admin() {
            Ok(()) => std::process::exit(0),
            Err(error) => {
                win_api::show_admin_error(&format!(
                    "应用没有管理员权限，无法继续启动。\n请在 UAC 提示中选择“是”，或使用管理员账户运行。\n\n{}",
                    error
                ));
                std::process::exit(1);
            }
        }
    }

    // ===== 初始化状态 =====
    let broadcast_state = Arc::new(Mutex::new(BroadcastState::default()));
    let scan_state = Arc::new(Mutex::new(ScanState::default()));
    let log_state = Arc::new(Mutex::new(LogState::default()));
    let public_broadcast_state = Arc::new(Mutex::new(PublicBroadcastState::default()));

    // ===== 启动公益广播 =====
    {
        let st = public_broadcast_state.clone();
        let enabled = st.lock().unwrap().enabled;
        if enabled {
            broadcast::start_public_broadcast(st);
        }
    }

    // ===== 构建 Tauri 窗口 =====
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(broadcast_state)
        .manage(scan_state)
        .manage(log_state)
        .manage(public_broadcast_state)
        .invoke_handler(tauri::generate_handler![
            commands::greet,
            commands::get_radmin_info,
            commands::get_radmin_detail,
            commands::get_radmin_snapshot,
            commands::join_radmin_network,
            commands::ping_radmin_peers,
            commands::apply_acrylic,
            commands::random_ip,
            commands::random_mac,
            commands::random_rid,
            commands::random_alias,
            commands::apply_rid_alias,
            commands::start_broadcast_cmd,
            commands::pause_broadcast_cmd,
            commands::resume_broadcast_cmd,
            commands::stop_broadcast_cmd,
            commands::get_broadcast_stats,
            commands::scan_start,
            commands::scan_stop,
            commands::scan_get_results,
            commands::scan_clear,
            commands::scan_set_max_per_ip,
            commands::scan_get_max_per_ip,
            commands::set_public_broadcast,
            commands::get_public_broadcast,
            commands::get_logs,
            commands::clear_logs,
            commands::add_log,
            commands::set_anonymous,
            commands::get_anonymous,
            commands::export_report,
            commands::get_sysinfo,
            commands::get_desktop_path,
            commands::open_url,
            commands::get_app_config,
            commands::save_app_config,
            commands::fetch_qq_name,
            commands::is_admin,
            commands::run_as_admin,
            commands::apply_account,
            commands::get_current_ipv6,
            commands::get_accounts,
            commands::save_accounts,
            commands::import_account_profile,
            commands::save_account_profile,
            commands::list_account_profiles,
            commands::read_account_profile,
            commands::activate_account_profile,
            commands::delete_account_profile,
            commands::rename_account_profile,
            commands::move_account_profile,
            commands::open_account_config_folder,
            commands::join_minecraft_networks,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn run_system_registry_service() -> Result<(), String> {
    system_registry::run_as_service()
}

pub fn uninstall_system_registry_service() -> Result<(), String> {
    system_registry::uninstall_service()
}
