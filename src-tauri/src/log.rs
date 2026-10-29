use crate::radmin::run_ps_pub;
use chrono::Local;
use std::fs;
use std::io::Write;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct LogState {
    pub lines: Vec<String>,
    pub anonymous: bool,
}

pub type SharedLog = Arc<Mutex<LogState>>;

pub fn add_log(state: &SharedLog, msg: &str) {
    if let Ok(mut s) = state.lock() {
        let ts = Local::now().format("%H:%M:%S").to_string();
        s.lines.push(format!("[{}] {}", ts, msg));
        if s.lines.len() > 1000 {
            let drain = s.lines.len() - 1000;
            s.lines.drain(0..drain);
        }
    }
}

pub fn get_logs(state: &SharedLog) -> Vec<String> {
    state.lock().map(|s| s.lines.clone()).unwrap_or_default()
}

pub fn clear_logs(state: &SharedLog) {
    if let Ok(mut s) = state.lock() {
        s.lines.clear();
    }
}

pub fn set_anonymous(state: &SharedLog, anon: bool) {
    if let Ok(mut s) = state.lock() {
        s.anonymous = anon;
    }
}

pub fn is_anonymous(state: &SharedLog) -> bool {
    state.lock().map(|s| s.anonymous).unwrap_or(false)
}

pub fn get_system_info() -> Vec<(String, String)> {
    let mut info = Vec::new();

    let os_name = run_ps_pub("(Get-CimInstance Win32_OperatingSystem).Caption");
    let os_ver = run_ps_pub("(Get-CimInstance Win32_OperatingSystem).Version");
    let os_arch = run_ps_pub("(Get-CimInstance Win32_OperatingSystem).OSArchitecture");
    info.push((
        "操作系统".into(),
        format!("{} | 版本 {} | {}", os_name, os_ver, os_arch),
    ));

    let cpu_name = run_ps_pub("(Get-CimInstance Win32_Processor).Name");
    let cpu_manu = run_ps_pub("(Get-CimInstance Win32_Processor).Manufacturer");
    let cores = run_ps_pub("(Get-CimInstance Win32_Processor).NumberOfCores");
    let logical = run_ps_pub("(Get-CimInstance Win32_Processor).NumberOfLogicalProcessors");
    info.push((
        "CPU".into(),
        format!(
            "厂商: {} | 型号: {} | 核心: {} 核 {} 线程",
            cpu_manu, cpu_name, cores, logical
        ),
    ));

    let mem_manu =
        run_ps_pub("(Get-CimInstance Win32_PhysicalMemory | Select-Object -First 1).Manufacturer");
    let mem_part =
        run_ps_pub("(Get-CimInstance Win32_PhysicalMemory | Select-Object -First 1).PartNumber");
    let total_mem = run_ps_pub("(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory");
    let total_gb = total_mem
        .trim()
        .parse::<u64>()
        .map(|v| v / (1024 * 1024 * 1024))
        .unwrap_or(0);
    info.push((
        "内存".into(),
        format!(
            "厂商: {} | 型号: {} | 总容量: {} GB",
            mem_manu, mem_part, total_gb
        ),
    ));

    let gpu_name =
        run_ps_pub("(Get-CimInstance Win32_VideoController | Select-Object -First 1).Name");
    let gpu_manu = run_ps_pub(
        "(Get-CimInstance Win32_VideoController | Select-Object -First 1).AdapterCompatibility",
    );
    info.push((
        "显卡".into(),
        format!("厂商: {} | 型号: {}", gpu_manu, gpu_name),
    ));

    let board_manu = run_ps_pub("(Get-CimInstance Win32_BaseBoard).Manufacturer");
    let board_model = run_ps_pub("(Get-CimInstance Win32_BaseBoard).Product");
    info.push((
        "主板".into(),
        format!("厂商: {} | 型号: {}", board_manu, board_model),
    ));

    info
}

pub fn export_report(
    state: &SharedLog,
    path: &str,
    radmin_info: Vec<(String, String)>,
) -> Result<(), String> {
    let logs = get_logs(state);
    let anonymous = is_anonymous(state);
    let username = if anonymous {
        "匿名用户".to_string()
    } else {
        std::env::var("USERNAME").unwrap_or_else(|_| "未知".to_string())
    };

    let mut report = String::new();
    report.push_str("============================================================\n");
    report.push_str("  Radmin Tools 日志报告\n");
    report.push_str("============================================================\n\n");
    report.push_str("【系统信息】\n");
    report.push_str(&format!(
        "  导出时间: {}\n",
        Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    report.push_str(&format!("  系统用户: {}\n", username));
    for (k, v) in get_system_info() {
        report.push_str(&format!("  {}: {}\n", k, v));
    }
    report.push_str("\n【Radmin VPN 网卡信息】\n");
    for (k, v) in radmin_info {
        report.push_str(&format!("  {}: {}\n", k, v));
    }
    report.push_str("\n============================================================\n");
    report.push_str("【运行日志】\n");
    report.push_str("============================================================\n\n");
    for line in logs {
        report.push_str(&line);
        report.push('\n');
    }

    let file = fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("report.txt", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(report.as_bytes())
        .map_err(|e| e.to_string())?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}
