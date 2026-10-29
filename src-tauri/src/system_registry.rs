use crate::config::RegistryValueSnapshot;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

const SERVICE_NAME: &str = "CrafterBoxRegistryBridge";
const SERVICE_DISPLAY_NAME: &str = "CrafterBox Registry Bridge";
const SERVICE_ARGUMENT: &str = "--crafterbox-registry-service";
const PIPE_NAME: &str = r"\\.\pipe\CrafterBoxRegistryBridge-v1";
const REGISTRATION_PATH: &str = r"SOFTWARE\WOW6432Node\Famatech\RadminVPN\1.0\Registration";
const MAX_MESSAGE_SIZE: usize = 64 * 1024;
const RESPONSE_ACK: u8 = 0xA5;

#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Request {
    ReadServerPassword,
    WriteServerPassword { value: RegistryValueSnapshot },
}

#[derive(Serialize, Deserialize)]
struct Response {
    value: Option<RegistryValueSnapshot>,
    error: Option<String>,
}

#[cfg(windows)]
pub fn read_server_password() -> Result<RegistryValueSnapshot, String> {
    match send_request(Request::ReadServerPassword)? {
        Some(value) => Ok(value),
        None => Err("SYSTEM 服务没有返回 ServerPassword".to_string()),
    }
}

#[cfg(not(windows))]
pub fn read_server_password() -> Result<RegistryValueSnapshot, String> {
    Err("SYSTEM 注册表服务仅支持 Windows".to_string())
}

#[cfg(windows)]
pub fn write_server_password(value: RegistryValueSnapshot) -> Result<(), String> {
    send_request(Request::WriteServerPassword { value }).map(|_| ())
}

#[cfg(not(windows))]
pub fn write_server_password(_value: RegistryValueSnapshot) -> Result<(), String> {
    Err("SYSTEM 注册表服务仅支持 Windows".to_string())
}

#[cfg(windows)]
fn send_request(request: Request) -> Result<Option<RegistryValueSnapshot>, String> {
    ensure_service()?;
    let mut last_error = None;
    let mut pipe = None;
    for _ in 0..50 {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(PIPE_NAME)
        {
            Ok(opened) => {
                pipe = Some(opened);
                break;
            }
            Err(error) => {
                last_error = Some(error);
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
    let mut pipe = pipe.ok_or_else(|| {
        format!(
            "SYSTEM 服务未能在 5 秒内打开本机管道：{}",
            last_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "未知错误".to_string())
        )
    })?;
    let payload = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
    if payload.len() > MAX_MESSAGE_SIZE {
        return Err("注册表服务请求超出大小限制".to_string());
    }
    pipe.write_all(&(payload.len() as u32).to_le_bytes())
        .and_then(|_| pipe.write_all(&payload))
        .map_err(|error| format!("发送 SYSTEM 注册表请求失败：{error}"))?;

    let mut length = [0u8; 4];
    pipe.read_exact(&mut length)
        .map_err(|error| format!("读取 SYSTEM 注册表服务响应失败：{error}"))?;
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_MESSAGE_SIZE {
        return Err("SYSTEM 注册表服务响应长度无效".to_string());
    }
    let mut response = vec![0u8; length];
    pipe.read_exact(&mut response)
        .map_err(|error| format!("读取 SYSTEM 注册表服务响应失败：{error}"))?;
    pipe.write_all(&[RESPONSE_ACK])
        .map_err(|error| format!("确认 SYSTEM 注册表服务响应失败：{error}"))?;
    let response: Response =
        serde_json::from_slice(&response).map_err(|error| error.to_string())?;
    if let Some(error) = response.error {
        return Err(error);
    }
    Ok(response.value)
}

#[cfg(windows)]
fn ensure_service() -> Result<(), String> {
    use std::ffi::OsStr;
    use windows_service::service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState,
        ServiceType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .map_err(|error| format!("无法连接 Windows 服务管理器：{error}"))?;
    let access = ServiceAccess::QUERY_STATUS
        | ServiceAccess::START
        | ServiceAccess::STOP
        | ServiceAccess::CHANGE_CONFIG;
    let (executable_path, needs_copy) = service_executable_path()?;
    let info = ServiceInfo {
        name: SERVICE_NAME.into(),
        display_name: SERVICE_DISPLAY_NAME.into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::OnDemand,
        error_control: ServiceErrorControl::Normal,
        executable_path,
        launch_arguments: vec![SERVICE_ARGUMENT.into()],
        dependencies: Vec::new(),
        account_name: None,
        account_password: None,
    };
    let service = match manager.open_service(OsStr::new(SERVICE_NAME), access) {
        Ok(service) => {
            if needs_copy
                && service
                    .query_status()
                    .map_err(|error| error.to_string())?
                    .current_state
                    == ServiceState::Running
            {
                service
                    .stop()
                    .map_err(|error| format!("停止旧版 SYSTEM 注册表服务失败：{error}"))?;
                let mut stopped = false;
                for _ in 0..100 {
                    if service
                        .query_status()
                        .map_err(|error| error.to_string())?
                        .current_state
                        == ServiceState::Stopped
                    {
                        stopped = true;
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                if !stopped {
                    return Err("等待旧版 SYSTEM 注册表服务停止超时".to_string());
                }
            }
            if needs_copy {
                install_service_executable(&info.executable_path)?;
            }
            secure_file(&info.executable_path)?;
            service
                .change_config(&info)
                .map_err(|error| format!("更新 SYSTEM 注册表服务配置失败：{error}"))?;
            service
        }
        Err(_) => {
            install_service_executable(&info.executable_path)?;
            manager
                .create_service(&info, access)
                .map_err(|error| format!("安装 SYSTEM 注册表服务失败：{error}"))?
        }
    };

    if service
        .query_status()
        .map_err(|error| error.to_string())?
        .current_state
        != ServiceState::Running
    {
        service
            .start::<&OsStr>(&[])
            .map_err(|error| format!("启动 SYSTEM 注册表服务失败：{error}"))?;
    }
    Ok(())
}

#[cfg(windows)]
fn service_executable_path() -> Result<(std::path::PathBuf, bool), String> {
    let service_dir = program_files_directory()?.join(SERVICE_DISPLAY_NAME);
    secure_directory(&service_dir)?;

    let destination = service_dir.join("RadminRegistryService.exe");
    let current = std::env::current_exe().map_err(|error| error.to_string())?;
    let needs_copy = match (std::fs::read(current), std::fs::read(&destination)) {
        (Ok(source), Ok(installed)) => source != installed,
        (Ok(_), Err(_)) => true,
        (Err(error), _) => return Err(format!("无法读取应用程序 EXE：{error}")),
    };
    Ok((destination, needs_copy))
}

#[cfg(windows)]
fn install_service_executable(destination: &std::path::Path) -> Result<(), String> {
    let current = std::env::current_exe().map_err(|error| error.to_string())?;
    std::fs::copy(current, destination)
        .map_err(|error| format!("无法安装受保护的 SYSTEM 服务 EXE：{error}"))?;
    secure_file(destination)
}

#[cfg(windows)]
fn program_files_directory() -> Result<std::path::PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;

    let folder_id = Guid {
        data1: 0x905e63b6,
        data2: 0xc1bf,
        data3: 0x494e,
        data4: [0xb2, 0x9c, 0x65, 0xb7, 0x32, 0xd3, 0xd2, 0x1a],
    };
    let mut path = std::ptr::null_mut();
    let result = unsafe { SHGetKnownFolderPath(&folder_id, 0, std::ptr::null_mut(), &mut path) };
    if result < 0 {
        return Err(format!(
            "无法确定 Program Files 目录：HRESULT 0x{:08x}",
            result as u32
        ));
    }
    if path.is_null() {
        return Err("Windows 未返回 Program Files 路径".to_string());
    }
    let mut length = 0;
    unsafe {
        while *path.add(length) != 0 {
            length += 1;
        }
    }
    let directory =
        unsafe { std::ffi::OsString::from_wide(std::slice::from_raw_parts(path, length)) };
    unsafe {
        CoTaskMemFree(path.cast());
    }
    Ok(directory.into())
}

#[cfg(windows)]
fn secure_directory(path: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let sddl: Vec<u16> = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let descriptor = security_descriptor(&sddl)?;
    let attributes = SecurityAttributes {
        length: std::mem::size_of::<SecurityAttributes>() as u32,
        security_descriptor: descriptor,
        inherit_handle: 0,
    };
    let created = unsafe { CreateDirectoryW(path_wide.as_ptr(), &attributes) };
    let create_error = if created == 0 {
        Some(std::io::Error::last_os_error())
    } else {
        None
    };
    let secured = unsafe { SetFileSecurityW(path_wide.as_ptr(), 0x80000004, descriptor) };
    unsafe {
        LocalFree(descriptor);
    }
    if secured == 0 {
        return Err(format!(
            "无法限制 SYSTEM 服务目录的写入权限：{}",
            std::io::Error::last_os_error()
        ));
    }
    if let Some(error) = create_error {
        if error.raw_os_error() != Some(183) {
            return Err(format!("无法创建 SYSTEM 服务目录：{error}"));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn secure_file(path: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let sddl: Vec<u16> = "D:P(A;;FA;;;SY)(A;;FA;;;BA)"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let descriptor = security_descriptor(&sddl)?;
    let secured = unsafe { SetFileSecurityW(path_wide.as_ptr(), 0x80000004, descriptor) };
    unsafe {
        LocalFree(descriptor);
    }
    if secured == 0 {
        return Err(format!(
            "无法限制 SYSTEM 服务程序的写入权限：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn security_descriptor(sddl: &[u16]) -> Result<*mut std::ffi::c_void, String> {
    let mut descriptor = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(format!(
            "无法创建 SYSTEM 服务安全描述符：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(descriptor)
}

#[cfg(windows)]
pub fn run_as_service() -> Result<(), String> {
    windows_service::service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .map_err(|error| format!("启动 SYSTEM 服务入口失败：{error}"))
}

#[cfg(windows)]
pub fn uninstall_service() -> Result<(), String> {
    use std::ffi::OsStr;
    use windows_service::service::{ServiceAccess, ServiceState};
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|error| format!("无法连接 Windows 服务管理器：{error}"))?;
    {
        let service = manager
            .open_service(
                OsStr::new(SERVICE_NAME),
                ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
            )
            .map_err(|error| format!("无法打开 SYSTEM 注册表服务：{error}"))?;
        let mut stop_requested = false;
        let mut stopped = false;
        for _ in 0..100 {
            let state = service
                .query_status()
                .map_err(|error| format!("无法查询 SYSTEM 注册表服务：{error}"))?
                .current_state;
            if state == ServiceState::Stopped {
                stopped = true;
                break;
            }
            if state == ServiceState::Running && !stop_requested {
                service
                    .stop()
                    .map_err(|error| format!("无法停止 SYSTEM 注册表服务：{error}"))?;
                stop_requested = true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if !stopped {
            return Err("等待 SYSTEM 注册表服务停止超时".to_string());
        }
        service
            .delete()
            .map_err(|error| format!("无法删除 SYSTEM 注册表服务：{error}"))?;
    }

    let service_dir = program_files_directory()?.join(SERVICE_DISPLAY_NAME);
    let service_exe = service_dir.join("RadminRegistryService.exe");
    match std::fs::remove_file(&service_exe) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("无法删除 SYSTEM 服务程序：{error}")),
    }
    match std::fs::remove_dir(&service_dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("无法删除 SYSTEM 服务目录：{error}")),
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn run_as_service() -> Result<(), String> {
    Err("SYSTEM 注册表服务仅支持 Windows".to_string())
}

#[cfg(not(windows))]
pub fn uninstall_service() -> Result<(), String> {
    Err("SYSTEM 注册表服务仅支持 Windows".to_string())
}

#[cfg(windows)]
windows_service::define_windows_service!(ffi_service_main, service_main);

#[cfg(windows)]
fn service_main(_arguments: Vec<std::ffi::OsString>) {
    if let Err(error) = run_service() {
        eprintln!("SYSTEM registry service failed: {error}");
    }
}

#[cfg(windows)]
fn run_service() -> Result<(), String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

    let stopping = Arc::new(AtomicBool::new(false));
    let stop_flag = stopping.clone();
    let status_handle =
        service_control_handler::register(SERVICE_NAME, move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                stop_flag.store(true, Ordering::SeqCst);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })
        .map_err(|error| error.to_string())?;

    status_handle
        .set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .map_err(|error| error.to_string())?;

    while !stopping.load(Ordering::SeqCst) {
        match serve_one_client(&stopping) {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => {
                eprintln!("SYSTEM registry pipe error: {error}");
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    }

    status_handle
        .set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(windows)]
fn serve_one_client(stopping: &std::sync::atomic::AtomicBool) -> Result<bool, String> {
    use std::os::windows::io::FromRawHandle;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    const PIPE_ACCESS_DUPLEX: u32 = 0x00000003;
    const PIPE_TYPE_BYTE: u32 = 0x00000000;
    const PIPE_READMODE_BYTE: u32 = 0x00000000;
    const PIPE_WAIT: u32 = 0x00000000;
    const PIPE_NOWAIT: u32 = 0x00000001;
    const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x00000008;
    const ERROR_PIPE_CONNECTED: u32 = 535;
    const ERROR_PIPE_LISTENING: u32 = 536;
    const INVALID_HANDLE_VALUE: isize = -1;

    let pipe_name: Vec<u16> = PIPE_NAME.encode_utf16().chain(Some(0)).collect();
    let sddl: Vec<u16> = "D:P(A;;GA;;;SY)(A;;GA;;;BA)"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut descriptor = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(format!(
            "无法设置本机管道 ACL：{}",
            std::io::Error::last_os_error()
        ));
    }
    let attributes = SecurityAttributes {
        length: std::mem::size_of::<SecurityAttributes>() as u32,
        security_descriptor: descriptor,
        inherit_handle: 0,
    };
    let raw_pipe = unsafe {
        CreateNamedPipeW(
            pipe_name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            MAX_MESSAGE_SIZE as u32 + 4,
            MAX_MESSAGE_SIZE as u32 + 4,
            0,
            &attributes,
        )
    };
    unsafe {
        LocalFree(descriptor);
    }
    if raw_pipe as isize == INVALID_HANDLE_VALUE {
        return Err(format!(
            "无法创建 SYSTEM 服务本机管道：{}",
            std::io::Error::last_os_error()
        ));
    }

    let mut connected = false;
    let idle_deadline = Instant::now() + Duration::from_secs(120);
    while !stopping.load(Ordering::SeqCst) {
        if unsafe { ConnectNamedPipe(raw_pipe, std::ptr::null_mut()) } != 0 {
            connected = true;
            break;
        }
        match unsafe { GetLastError() } {
            ERROR_PIPE_CONNECTED => {
                connected = true;
                break;
            }
            ERROR_PIPE_LISTENING => {
                if Instant::now() >= idle_deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            error => {
                unsafe {
                    DisconnectNamedPipe(raw_pipe);
                    CloseHandle(raw_pipe);
                }
                return Err(format!("等待本机管道连接失败，错误码 {error}"));
            }
        }
    }

    if connected {
        let mut pipe_mode = PIPE_READMODE_BYTE | PIPE_WAIT;
        if unsafe {
            SetNamedPipeHandleState(
                raw_pipe,
                &mut pipe_mode,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            let error = std::io::Error::last_os_error();
            unsafe {
                DisconnectNamedPipe(raw_pipe);
                CloseHandle(raw_pipe);
            }
            return Err(format!("切换 SYSTEM 服务管道为阻塞模式失败：{error}"));
        }
        let pipe = unsafe { std::fs::File::from_raw_handle(raw_pipe) };
        let mut pipe = pipe;
        let result = handle_client(&mut pipe);
        let _ = unsafe { DisconnectNamedPipe(raw_pipe) };
        result?;
        Ok(true)
    } else {
        unsafe {
            DisconnectNamedPipe(raw_pipe);
            CloseHandle(raw_pipe);
        }
        Ok(false)
    }
}

#[cfg(windows)]
fn handle_client(pipe: &mut std::fs::File) -> Result<(), String> {
    let mut length = [0u8; 4];
    pipe.read_exact(&mut length)
        .map_err(|error| format!("读取服务请求长度失败：{error}"))?;
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_MESSAGE_SIZE {
        return Err("SYSTEM 注册表请求长度无效".to_string());
    }
    let mut payload = vec![0u8; length];
    pipe.read_exact(&mut payload)
        .map_err(|error| format!("读取服务请求失败：{error}"))?;
    let request: Request = serde_json::from_slice(&payload).map_err(|error| error.to_string())?;
    let response = match process_request(request) {
        Ok(value) => Response { value, error: None },
        Err(error) => Response {
            value: None,
            error: Some(error),
        },
    };
    let response = serde_json::to_vec(&response).map_err(|error| error.to_string())?;
    pipe.write_all(&(response.len() as u32).to_le_bytes())
        .and_then(|_| pipe.write_all(&response))
        .map_err(|error| format!("发送 SYSTEM 注册表响应失败：{error}"))?;
    let mut acknowledgement = [0u8; 1];
    pipe.read_exact(&mut acknowledgement)
        .map_err(|error| format!("等待客户端确认服务响应失败：{error}"))?;
    if acknowledgement[0] != RESPONSE_ACK {
        return Err("客户端未确认 SYSTEM 注册表服务响应".to_string());
    }
    Ok(())
}

#[cfg(windows)]
fn process_request(request: Request) -> Result<Option<RegistryValueSnapshot>, String> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_BINARY};
    use winreg::{RegKey, RegValue};

    let root = RegKey::predef(HKEY_LOCAL_MACHINE);
    match request {
        Request::ReadServerPassword => {
            let key = root
                .open_subkey_with_flags(REGISTRATION_PATH, KEY_QUERY_VALUE)
                .map_err(|error| format!("SYSTEM 无法打开 Registration 键：{error}"))?;
            let value = key
                .get_raw_value("ServerPassword")
                .map_err(|error| format!("SYSTEM 无法读取 ServerPassword：{error}"))?;
            if value.vtype != REG_BINARY || value.bytes.is_empty() {
                return Err("ServerPassword 不是有效的非空 REG_BINARY 值".to_string());
            }
            Ok(Some(RegistryValueSnapshot {
                name: "ServerPassword".to_string(),
                value_type: value.vtype as u32,
                bytes: value.bytes,
            }))
        }
        Request::WriteServerPassword { value } => {
            if !value.name.eq_ignore_ascii_case("ServerPassword")
                || value.value_type != REG_BINARY as u32
                || value.bytes.is_empty()
                || value.bytes.len() > 4096
            {
                return Err("只允许恢复有效的 ServerPassword REG_BINARY 值".to_string());
            }
            let key = root
                .open_subkey_with_flags(REGISTRATION_PATH, KEY_SET_VALUE)
                .map_err(|error| format!("SYSTEM 无法打开 Registration 键：{error}"))?;
            key.set_raw_value(
                "ServerPassword",
                &RegValue {
                    bytes: value.bytes,
                    vtype: REG_BINARY,
                },
            )
            .map_err(|error| format!("SYSTEM 无法恢复 ServerPassword：{error}"))?;
            Ok(None)
        }
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn CreateDirectoryW(path: *const u16, security_attributes: *const SecurityAttributes) -> i32;
    fn CreateNamedPipeW(
        name: *const u16,
        open_mode: u32,
        pipe_mode: u32,
        max_instances: u32,
        out_buffer_size: u32,
        in_buffer_size: u32,
        default_timeout: u32,
        security_attributes: *const SecurityAttributes,
    ) -> *mut std::ffi::c_void;
    fn ConnectNamedPipe(pipe: *mut std::ffi::c_void, overlapped: *mut std::ffi::c_void) -> i32;
    fn GetLastError() -> u32;
    fn DisconnectNamedPipe(pipe: *mut std::ffi::c_void) -> i32;
    fn SetNamedPipeHandleState(
        pipe: *mut std::ffi::c_void,
        mode: *mut u32,
        max_collection_count: *mut u32,
        collect_data_timeout: *mut u32,
    ) -> i32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    fn LocalFree(memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}

#[cfg(windows)]
#[link(name = "advapi32")]
extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        string_security_descriptor: *const u16,
        revision: u32,
        security_descriptor: *mut *mut std::ffi::c_void,
        security_descriptor_size: *mut u32,
    ) -> i32;
    fn SetFileSecurityW(
        file_name: *const u16,
        security_information: u32,
        security_descriptor: *mut std::ffi::c_void,
    ) -> i32;
}

#[cfg(windows)]
#[repr(C)]
struct SecurityAttributes {
    length: u32,
    security_descriptor: *mut std::ffi::c_void,
    inherit_handle: i32,
}

#[cfg(windows)]
#[repr(C)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

#[cfg(windows)]
#[link(name = "shell32")]
extern "system" {
    fn SHGetKnownFolderPath(
        folder_id: *const Guid,
        flags: u32,
        token: *mut std::ffi::c_void,
        path: *mut *mut u16,
    ) -> i32;
}

#[cfg(windows)]
#[link(name = "ole32")]
extern "system" {
    fn CoTaskMemFree(memory: *mut std::ffi::c_void);
}
