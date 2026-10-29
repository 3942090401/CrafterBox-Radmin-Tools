use crate::config::RadminAccount;
use crate::system_registry;
use std::process::Command;
use winreg::enums::*;
use winreg::{RegKey, RegValue};

const REG_PATHS: [&str; 2] = [
    r"SOFTWARE\Famatech\RadminVPN\1.0",
    r"SOFTWARE\WOW6432Node\Famatech\RadminVPN\1.0",
];
const REG_HIVES: [(&str, winreg::HKEY); 2] =
    [("HKLM", HKEY_LOCAL_MACHINE), ("HKCU", HKEY_CURRENT_USER)];

fn run_ps(cmd: &str) -> String {
    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut c = Command::new("powershell");
    c.args(["-NoProfile", "-Command", cmd]);
    #[cfg(target_os = "windows")]
    c.creation_flags(CREATE_NO_WINDOW);
    match c.output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

fn run_ps_checked(cmd: &str) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-Command", cmd]);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|error| format!("无法启动 PowerShell：{error}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if error.is_empty() {
            format!("PowerShell 执行失败：{}", output.status)
        } else {
            format!("PowerShell 执行失败：{error}")
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn run_ps_pub(cmd: &str) -> String {
    run_ps(cmd)
}

fn run_cmd(cmd: &str) {
    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut c = Command::new("cmd");
    c.args(["/C", cmd]);
    #[cfg(target_os = "windows")]
    c.creation_flags(CREATE_NO_WINDOW);
    let _ = c.output();
}

pub fn get_radmin_ip() -> String {
    let ps = r#"
$OutputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$a = Get-NetAdapter | Where-Object { $_.Name -like '*Radmin*' -or $_.InterfaceDescription -like '*Radmin*' }
if ($a) {
    $ip = Get-NetIPAddress -InterfaceIndex $a.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue
    if ($ip) { $ip.IPAddress } else { '' }
} else { '' }
"#;
    let r = run_ps(ps);
    if r.is_empty() {
        "未找到".to_string()
    } else {
        r
    }
}

pub fn get_radmin_ipv6() -> String {
    let ps = r#"
$OutputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$a = Get-NetAdapter | Where-Object { $_.Name -like '*Radmin*' -or $_.InterfaceDescription -like '*Radmin*' }
if ($a) {
    $ip = Get-NetIPAddress -InterfaceIndex $a.ifIndex -AddressFamily IPv6 -ErrorAction SilentlyContinue
    if ($ip) { $ip.IPAddress -join ', ' } else { '' }
} else { '' }
"#;
    let r = run_ps(ps);
    if r.is_empty() {
        "未找到".to_string()
    } else {
        r
    }
}

pub fn get_radmin_mac() -> String {
    let ps = r#"
$OutputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$a = Get-NetAdapter | Where-Object { $_.Name -like '*Radmin*' -or $_.InterfaceDescription -like '*Radmin*' }
if ($a) { $a.MacAddress } else { '' }
"#;
    let r = run_ps(ps);
    if r.is_empty() {
        "未找到".to_string()
    } else {
        r
    }
}

pub fn get_radmin_rid() -> String {
    for (_, hive) in REG_HIVES {
        let root = RegKey::predef(hive);
        for path in REG_PATHS {
            if let Ok(key) = root.open_subkey(path) {
                if let Ok(value) = key.get_raw_value("RID") {
                    let rid = match value.vtype {
                        REG_DWORD if value.bytes.len() == 4 => Some(
                            u32::from_le_bytes(value.bytes[..4].try_into().unwrap()).to_string(),
                        ),
                        REG_QWORD if value.bytes.len() == 8 => Some(
                            u64::from_le_bytes(value.bytes[..8].try_into().unwrap()).to_string(),
                        ),
                        REG_SZ | REG_EXPAND_SZ => {
                            key.get_value::<String, _>("RID").ok().filter(|rid| {
                                !rid.is_empty() && rid.bytes().all(|byte| byte.is_ascii_digit())
                            })
                        }
                        _ => None,
                    };
                    if let Some(rid) = rid {
                        return rid;
                    }
                }
            }
        }
    }
    String::new()
}

pub fn get_radmin_alias() -> String {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for path in REG_PATHS.iter() {
        if let Ok(key) = hklm.open_subkey(path) {
            if let Ok(alias) = key.get_value::<String, _>("Alias") {
                return alias;
            }
        }
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for path in REG_PATHS.iter() {
        if let Ok(key) = hkcu.open_subkey(path) {
            if let Ok(alias) = key.get_value::<String, _>("Alias") {
                return alias;
            }
        }
    }
    String::new()
}

// 这里直接返回二进制 Vec<u8>
pub fn get_server_password() -> Result<Vec<u8>, String> {
    system_registry::read_server_password().map(|value| value.bytes)
}

pub fn get_radmin_details() -> String {
    let ps = r#"
$OutputEncoding = [System.Text.Encoding]::UTF8
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$a = Get-NetAdapter | Where-Object { $_.Name -like '*Radmin*' -or $_.InterfaceDescription -like '*Radmin*' }
if ($a) {
    $ipv4 = Get-NetIPAddress -InterfaceIndex $a.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue
    $ipv6 = Get-NetIPAddress -InterfaceIndex $a.ifIndex -AddressFamily IPv6 -ErrorAction SilentlyContinue
    Write-Output "Name: $($a.Name)"
    Write-Output "Description: $($a.InterfaceDescription)"
    Write-Output "Status: $($a.Status)"
    Write-Output "MAC: $($a.MacAddress)"
    Write-Output "LinkSpeed: $($a.LinkSpeed)"
    Write-Output "MTU: $($a.MtuSize)"
    if ($ipv4) { Write-Output "IPv4: $($ipv4.IPAddress)/$($ipv4.PrefixLength)" }
    if ($ipv6) {
        $v6list = ($ipv6 | ForEach-Object { $_.IPAddress }) -join ', '
        Write-Output "IPv6: $v6list"
    }
} else { Write-Output 'Radmin VPN adapter not found' }
"#;
    run_ps(ps)
}

fn stop_service() {
    run_cmd("taskkill /f /im RvControlSvc.exe 2>nul");
    std::thread::sleep(std::time::Duration::from_millis(800));
}

fn start_service() {
    run_cmd("net start RvControlSvc 2>nul");
    std::thread::sleep(std::time::Duration::from_millis(1500));
}

fn delete_reg_value(name: &str) {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for path in REG_PATHS.iter() {
        if let Ok(key) = hklm.open_subkey_with_flags(path, KEY_ALL_ACCESS) {
            let _ = key.delete_value(name);
        }
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for path in REG_PATHS.iter() {
        if let Ok(key) = hkcu.open_subkey_with_flags(path, KEY_ALL_ACCESS) {
            let _ = key.delete_value(name);
        }
    }
}

fn set_reg_u64(name: &str, val: u64) {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for path in REG_PATHS.iter() {
        if let Ok(key) = hklm.open_subkey_with_flags(path, KEY_ALL_ACCESS) {
            let _ = key.set_value(name, &val);
        }
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for path in REG_PATHS.iter() {
        if let Ok(key) = hkcu.open_subkey_with_flags(path, KEY_ALL_ACCESS) {
            let _ = key.set_value(name, &val);
        }
    }
}

fn set_reg_string(name: &str, val: &str) {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for path in REG_PATHS.iter() {
        if let Ok(key) = hklm.open_subkey_with_flags(path, KEY_ALL_ACCESS) {
            let _ = key.set_value(name, &val.to_string());
        }
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for path in REG_PATHS.iter() {
        if let Ok(key) = hkcu.open_subkey_with_flags(path, KEY_ALL_ACCESS) {
            let _ = key.set_value(name, &val.to_string());
        }
    }
}

pub fn reset_ip() -> Result<String, String> {
    stop_service();
    delete_reg_value("IPv4");
    delete_reg_value("IPv6");
    delete_reg_value("RID");
    start_service();
    Ok("IP 已随机".to_string())
}

pub fn reset_rid() -> Result<String, String> {
    stop_service();
    delete_reg_value("RID");
    start_service();
    Ok("RID 已随机".to_string())
}

pub fn reset_mac() -> Result<String, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let a = (seed & 0xFF) as u8;
    let b = ((seed >> 8) & 0xFF) as u8;
    let c = ((seed >> 16) & 0xFF) as u8;
    let d = ((seed >> 24) & 0xFF) as u8;
    let mac = format!(
        "02:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        a,
        b,
        c,
        d,
        (seed >> 32) as u8
    );
    let hex_mac = mac.replace(":", "");
    let ps_set = format!(
        r#"
try {{
$ErrorActionPreference = 'Stop'
$adapters = @(Get-NetAdapter -IncludeHidden -ErrorAction Stop | Where-Object {{
    $_.Name -like '*Radmin*' -or $_.InterfaceDescription -like '*Radmin*'
}})
if ($adapters.Count -ne 1) {{
    throw "预期找到 1 个 Radmin 网卡，实际找到 $($adapters.Count) 个"
}}
$adapter = $adapters[0]
$classRoot = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{{4D36E972-E325-11CE-BFC1-08002BE10318}}'
$adapterGuid = $adapter.InterfaceGuid.ToString().Trim('{{}}')
$classKeys = @(Get-ChildItem -LiteralPath $classRoot -ErrorAction Stop | ForEach-Object {{
    $key = Get-ItemProperty -LiteralPath $_.PSPath -Name NetCfgInstanceId -ErrorAction SilentlyContinue
    if ($key.NetCfgInstanceId -and $key.NetCfgInstanceId.Trim('{{}}') -eq $adapterGuid) {{
        $_
    }}
}})
if ($classKeys.Count -ne 1) {{
    throw "无法唯一定位 Radmin 网卡注册表项（找到 $($classKeys.Count) 个）"
}}
Set-ItemProperty -LiteralPath $classKeys[0].PSPath -Name NetworkAddress -Value '{}' -Force -ErrorAction Stop
Stop-Service -Name 'RvControlSvc' -Force -ErrorAction Stop
Start-Service -Name 'RvControlSvc' -ErrorAction Stop
Start-Sleep -Seconds 2
$actualAdapters = @(Get-NetAdapter -IncludeHidden -ErrorAction Stop | Where-Object {{
    $_.InterfaceGuid.ToString() -eq $adapter.InterfaceGuid.ToString()
}})
if ($actualAdapters.Count -ne 1) {{
    throw "重启服务后无法重新定位 Radmin 网卡"
}}
$actualMac = $actualAdapters[0].MacAddress -replace '[:-]', ''
if ($actualMac.ToUpperInvariant() -ne '{}') {{
    throw "已写入新 MAC，但网卡重启后仍报告 $actualMac"
}}
Write-Output $actualMac
}} catch {{
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}}
"#,
        hex_mac, hex_mac
    );
    let actual_mac = run_ps_checked(&ps_set)?;
    if !actual_mac.eq_ignore_ascii_case(&hex_mac) {
        return Err(format!(
            "MAC 修改未通过验证，网卡返回了无效结果：{actual_mac}"
        ));
    }
    let actual_mac = actual_mac
        .chars()
        .collect::<Vec<_>>()
        .chunks(2)
        .map(|pair| pair.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(":");
    Ok(format!("MAC 已修改并验证：{actual_mac}"))
}

pub fn apply_rid_alias(rid: &str, alias: &str) -> Result<String, String> {
    stop_service();
    if !rid.is_empty() {
        let rid_u64: u64 = rid.parse().map_err(|_| "RID 必须是数字".to_string())?;
        set_reg_u64("RID", rid_u64);
    }
    if !alias.is_empty() {
        set_reg_string("Alias", alias);
    }
    start_service();
    Ok("已应用".to_string())
}

pub fn apply_account(account: &RadminAccount) -> Result<String, String> {
    if account.alias.trim().is_empty() {
        return Err("账号 Alias 不能为空".to_string());
    }
    if account.rid.is_empty() || !account.rid.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("账号 RID 必须是十进制数字".to_string());
    }
    if account.server_password.is_empty() || account.server_password.len() > 4096 {
        return Err("账号 ServerPassword 无效".to_string());
    }
    let rid = account
        .rid
        .parse::<u64>()
        .map_err(|_| "账号 RID 超出范围".to_string())?;

    stop_service();
    let restore_result = (|| {
        write_account_registry_value(
            "Alias",
            RegValue {
                bytes: account
                    .alias
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .flat_map(u16::to_le_bytes)
                    .collect(),
                vtype: REG_SZ,
            },
        )?;
        write_account_registry_value(
            "RID",
            RegValue {
                bytes: rid.to_le_bytes().to_vec(),
                vtype: REG_QWORD,
            },
        )?;
        system_registry::write_server_password(crate::config::RegistryValueSnapshot {
            name: "ServerPassword".to_string(),
            value_type: REG_BINARY as u32,
            bytes: account.server_password.clone(),
        })
    })();
    start_service();
    restore_result?;
    Ok("已恢复该账号的 Alias、RID 和 ServerPassword".to_string())
}

fn write_account_registry_value(name: &str, value: RegValue) -> Result<(), String> {
    let hives = [
        ("HKLM", RegKey::predef(HKEY_LOCAL_MACHINE)),
        ("HKCU", RegKey::predef(HKEY_CURRENT_USER)),
    ];
    let mut written = false;
    for (hive_name, root) in hives {
        for path in REG_PATHS {
            match root.open_subkey_with_flags(path, KEY_SET_VALUE) {
                Ok(key) => {
                    key.set_raw_value(name, &value).map_err(|error| {
                        format!("写入注册表 {hive_name}\\{path}\\{name} 失败：{error}")
                    })?;
                    written = true;
                }
                Err(error) if matches!(error.raw_os_error(), Some(2) | Some(3)) => {}
                Err(error) => return Err(format!("无法打开注册表 {hive_name}\\{path}：{error}")),
            }
        }
    }
    if written {
        Ok(())
    } else {
        Err("未找到可写入的 Radmin 注册表键".to_string())
    }
}
