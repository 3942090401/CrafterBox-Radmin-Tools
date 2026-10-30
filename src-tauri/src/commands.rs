use crate::broadcast::{self, ScanResult, SharedScan, SharedState as SharedBroadcast};
use crate::config::{load_config, save_config, AppConfig, RadminAccount};
use crate::log::{self as logmod, SharedLog};
use crate::radmin;
use crate::win_api;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct RadminInfo {
    pub ip: String,
    pub mac: String,
    pub rid: String,
    pub alias: String,
    pub password: Vec<u8>, // 改为 Vec<u8>
}

#[tauri::command]
pub async fn get_radmin_info() -> Result<RadminInfo, String> {
    tokio::task::spawn_blocking(|| {
        Ok(RadminInfo {
            ip: radmin::get_radmin_ip(),
            mac: radmin::get_radmin_mac(),
            rid: radmin::get_radmin_rid(),
            alias: radmin::get_radmin_alias(),
            password: radmin::get_server_password()?,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn get_radmin_detail() -> String {
    tokio::task::spawn_blocking(radmin::get_radmin_details)
        .await
        .unwrap_or_else(|_| "读取失败".into())
}

#[tauri::command]
pub async fn get_radmin_snapshot() -> Result<crate::networks::RadminSnapshot, String> {
    tokio::task::spawn_blocking(crate::networks::read_snapshot)
        .await
        .map_err(|error| format!("读取 Radmin 网络状态任务失败：{error}"))?
}

/// 通过 GUI 管道发送 CJoinNetwork（msg id = 3），成功与否由服务端返回码决定。
#[tauri::command]
pub async fn join_radmin_network(
    name: String,
    password: String,
    flag: u32,
) -> Result<crate::networks::JoinReply, String> {
    tokio::task::spawn_blocking(move || crate::networks::join_network(&name, &password, flag))
        .await
        .map_err(|error| format!("加网任务失败：{error}"))?
}

/// 一键加入 `Minecraft [中文 1]` … `Minecraft [中文 14]`，返回每个网络的结果。
#[tauri::command]
pub async fn join_minecraft_networks(
    password: Option<String>,
    flag: Option<u32>,
) -> Result<Vec<crate::networks::BatchJoinResult>, String> {
    tokio::task::spawn_blocking(move || {
        crate::networks::join_minecraft_networks(
            password.as_deref().unwrap_or(""),
            flag.unwrap_or(0),
        )
    })
    .await
    .map_err(|error| format!("批量加网任务失败：{error}"))
}

/// 批量 ICMP 测延迟，返回 虚拟IP -> 毫秒。
#[tauri::command]
pub async fn ping_radmin_peers(
    hosts: Vec<String>,
    timeout_ms: Option<u32>,
) -> Result<std::collections::BTreeMap<String, u32>, String> {
    let timeout = timeout_ms.unwrap_or(1200).clamp(200, 5000);
    tokio::task::spawn_blocking(move || crate::networks::ping_hosts(&hosts, timeout))
        .await
        .map_err(|error| format!("延迟探测任务失败：{error}"))
}

#[tauri::command]
pub fn apply_acrylic(window: tauri::Window, opacity: u8, r: u8, g: u8, b: u8, effect: u8) {
    if let Ok(hwnd) = window.hwnd() {
        win_api::enable_acrylic(hwnd.0 as isize, opacity, (r, g, b), effect);
    }
}

#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

#[tauri::command]
pub async fn random_ip() -> Result<String, String> {
    tokio::task::spawn_blocking(radmin::reset_ip)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn random_mac() -> Result<String, String> {
    tokio::task::spawn_blocking(radmin::reset_mac)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn random_rid() -> Result<String, String> {
    tokio::task::spawn_blocking(radmin::reset_rid)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn apply_rid_alias(rid: String, alias: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || radmin::apply_rid_alias(&rid, &alias))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn random_alias() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
        .chars()
        .collect();
    let mut s = String::with_capacity(8);
    let mut n = seed;
    for _ in 0..8 {
        n = n
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let idx = ((n >> 33) as usize) % chars.len();
        s.push(chars[idx]);
    }
    s
}

#[tauri::command]
pub fn get_current_ipv6() -> String {
    radmin::get_radmin_ipv6()
}

#[tauri::command]
pub async fn apply_account(account: RadminAccount) -> Result<String, String> {
    tokio::task::spawn_blocking(move || radmin::apply_account(&account))
        .await
        .map_err(|e| e.to_string())?
}

// ===== 广播控制 =====
#[tauri::command]
pub fn start_broadcast_cmd(
    state: tauri::State<'_, SharedBroadcast>,
    count: usize,
    motd: String,
    prefix: String,
) -> Result<(), String> {
    {
        let mut s = state.lock().unwrap();
        if s.running {
            return Err("已在运行".to_string());
        }
        s.running = true;
        s.paused = false;
        s.packets_sent = 0;
        s.current_rate = 0;
    }
    broadcast::start_broadcast(state.inner().clone(), count, motd, prefix);
    Ok(())
}

#[tauri::command]
pub fn pause_broadcast_cmd(state: tauri::State<'_, SharedBroadcast>) -> Result<(), String> {
    broadcast::pause_broadcast(state.inner().clone());
    Ok(())
}

#[tauri::command]
pub fn resume_broadcast_cmd(state: tauri::State<'_, SharedBroadcast>) -> Result<(), String> {
    broadcast::resume_broadcast(state.inner().clone());
    Ok(())
}

#[tauri::command]
pub fn stop_broadcast_cmd(state: tauri::State<'_, SharedBroadcast>) -> Result<(), String> {
    broadcast::stop_broadcast(state.inner().clone());
    Ok(())
}

#[tauri::command]
pub fn get_broadcast_stats(state: tauri::State<'_, SharedBroadcast>) -> (u64, u64, bool, bool) {
    let s = state.lock().unwrap();
    (s.packets_sent, s.current_rate, s.running, s.paused)
}

// ===== MOTD 扫描 =====
#[tauri::command]
pub fn scan_start(state: tauri::State<'_, SharedScan>) {
    broadcast::start_scan(state.inner().clone());
}

#[tauri::command]
pub fn scan_stop(state: tauri::State<'_, SharedScan>) {
    broadcast::stop_scan(state.inner().clone());
}

#[tauri::command]
pub fn scan_get_results(state: tauri::State<'_, SharedScan>) -> Vec<ScanResult> {
    broadcast::get_scan_results(state.inner().clone())
}

#[tauri::command]
pub fn scan_clear(state: tauri::State<'_, SharedScan>) {
    broadcast::clear_scan_results(state.inner().clone());
}

#[tauri::command]
pub fn scan_set_max_per_ip(state: tauri::State<'_, SharedScan>, limit: u32) {
    broadcast::set_max_per_ip(state.inner().clone(), limit);
}

#[tauri::command]
pub fn scan_get_max_per_ip(state: tauri::State<'_, SharedScan>) -> u32 {
    broadcast::get_max_per_ip(state.inner().clone())
}

// ===== 公益广播 =====
#[tauri::command]
pub fn set_public_broadcast(
    state: tauri::State<'_, broadcast::SharedPublicBroadcast>,
    enabled: bool,
) {
    broadcast::set_public_broadcast(state.inner().clone(), enabled);
}

#[tauri::command]
pub fn get_public_broadcast(state: tauri::State<'_, broadcast::SharedPublicBroadcast>) -> bool {
    broadcast::is_public_broadcast_enabled(state.inner().clone())
}

// ===== 日志 =====
#[tauri::command]
pub fn get_logs(state: tauri::State<'_, SharedLog>) -> Vec<String> {
    logmod::get_logs(state.inner())
}

#[tauri::command]
pub fn clear_logs(state: tauri::State<'_, SharedLog>) {
    logmod::clear_logs(state.inner());
}

#[tauri::command]
pub fn add_log(state: tauri::State<'_, SharedLog>, msg: String) {
    logmod::add_log(state.inner(), &msg);
}

#[tauri::command]
pub fn set_anonymous(state: tauri::State<'_, SharedLog>, anon: bool) {
    logmod::set_anonymous(state.inner(), anon);
}

#[tauri::command]
pub fn get_anonymous(state: tauri::State<'_, SharedLog>) -> bool {
    logmod::is_anonymous(state.inner())
}

#[tauri::command]
pub async fn export_report(
    state: tauri::State<'_, SharedLog>,
    path: String,
) -> Result<String, String> {
    let log_ref = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let radmin_info = vec![
            ("IP 地址".to_string(), radmin::get_radmin_ip()),
            ("MAC 地址".to_string(), radmin::get_radmin_mac()),
            ("RID".to_string(), radmin::get_radmin_rid()),
            ("设备名".to_string(), radmin::get_radmin_alias()),
        ];
        logmod::export_report(&log_ref, &path, radmin_info)?;
        Ok(path)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_sysinfo() -> Vec<(String, String)> {
    tokio::task::spawn_blocking(logmod::get_system_info)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_desktop_path() -> String {
    use winreg::enums::*;
    use winreg::RegKey;
    let bs = char::from(92u8);
    let reg_path = [
        "Software",
        "Microsoft",
        "Windows",
        "CurrentVersion",
        "Explorer",
    ]
    .join(&bs.to_string());
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey(&reg_path) {
        if let Ok(desktop) = key.get_value::<String, _>("Desktop") {
            return desktop;
        }
    }
    std::env::var("USERPROFILE")
        .map(|p| format!("{}{}Desktop", p, bs))
        .unwrap_or_else(|_| "C:\\Desktop".to_string())
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "shell32")]
        extern "system" {
            fn ShellExecuteW(
                hwnd: isize,
                lp_operation: *const u16,
                lp_file: *const u16,
                lp_parameters: *const u16,
                lp_directory: *const u16,
                n_show_cmd: i32,
            ) -> isize;
        }
        let url_w: Vec<u16> = OsStr::new(&url)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let op_w: Vec<u16> = OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            let result = ShellExecuteW(
                0,
                op_w.as_ptr(),
                url_w.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            );
            if result <= 32 {
                return Err(format!("打开失败 code={}", result));
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_app_config() -> AppConfig {
    load_config()
}

#[tauri::command]
pub fn save_app_config(cfg: AppConfig) -> Result<(), String> {
    save_config(&cfg)
}

#[tauri::command]
pub fn fetch_qq_name(_qq: String) -> Result<String, String> {
    Ok("".to_string())
}

#[tauri::command]
pub fn is_admin() -> bool {
    crate::win_api::is_admin()
}

#[tauri::command]
pub fn run_as_admin() -> Result<(), String> {
    crate::win_api::run_as_admin()
}

#[tauri::command]
pub fn get_accounts() -> Result<Vec<RadminAccount>, String> {
    crate::config::load_accounts()
}

#[tauri::command]
pub fn save_accounts(accounts: Vec<RadminAccount>) -> Result<(), String> {
    crate::config::save_accounts(&accounts)
}

#[tauri::command]
pub fn import_account_profile(contents: String) -> Result<String, String> {
    crate::config::import_account_profile(&contents)
}

#[tauri::command]
pub fn save_account_profile(accounts: Vec<RadminAccount>) -> Result<String, String> {
    crate::config::save_account_profile(&accounts)
}

#[tauri::command]
pub fn list_account_profiles() -> Result<Vec<crate::config::AccountProfile>, String> {
    crate::config::list_account_profiles()
}

#[tauri::command]
pub fn read_account_profile(name: String) -> Result<String, String> {
    crate::config::read_account_profile(&name)
}

#[tauri::command]
pub fn activate_account_profile(name: String) -> Result<Vec<RadminAccount>, String> {
    crate::config::activate_account_profile(&name)
}

#[tauri::command]
pub fn delete_account_profile(name: String) -> Result<(), String> {
    crate::config::delete_account_profile(&name)
}

#[tauri::command]
pub fn rename_account_profile(file: String, name: String) -> Result<String, String> {
    crate::config::rename_account_profile(&file, &name)
}

#[tauri::command]
pub fn move_account_profile(file: String, direction: i32) -> Result<(), String> {
    crate::config::move_account_profile(&file, direction)
}

#[tauri::command]
pub fn open_account_config_folder() -> Result<(), String> {
    let accounts_path = crate::config::accounts_path();
    let path = accounts_path
        .parent()
        .ok_or_else(|| "无法确定配置文件夹路径".to_string())?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe")
            .arg(path)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|error| format!("无法打开配置文件夹：{error}"))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err(format!("打开配置文件夹仅支持 Windows：{}", path.display()))
    }
}
