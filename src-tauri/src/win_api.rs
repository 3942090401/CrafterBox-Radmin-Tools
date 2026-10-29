use std::ffi::c_void;

type SetWindowCompositionAttributeFn =
    unsafe extern "system" fn(isize, *mut WindowCompositionAttributeData) -> i32;

#[repr(C)]
struct AccentPolicy {
    accent_state: i32,
    accent_flags: i32,
    gradient_color: u32,
    animation_id: i32,
}

#[repr(C)]
struct WindowCompositionAttributeData {
    attribute: i32,
    data: *mut c_void,
    size_of_data: usize,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
    fn GetProcAddress(module: isize, name: *const u8) -> *const c_void;
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: isize,
        dw_attribute: u32,
        pv_attribute: *const c_void,
        cb_attribute: u32,
    ) -> i32;
}

const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;

fn get_set_wca_fn() -> Option<SetWindowCompositionAttributeFn> {
    unsafe {
        let name: Vec<u16> = "user32.dll\0".encode_utf16().collect();
        let hmod = GetModuleHandleW(name.as_ptr());
        if hmod == 0 {
            return None;
        }
        let proc_name = b"SetWindowCompositionAttribute\0";
        let addr = GetProcAddress(hmod, proc_name.as_ptr());
        if addr.is_null() {
            return None;
        }
        Some(std::mem::transmute(addr))
    }
}

/// Windows 11 Mica/云母效果（通过 DWM API）
fn set_dwm_backdrop(hwnd: isize, backdrop_type: i32) {
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop_type as *const _ as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );
    }
}

/// 主函数：应用窗口效果
/// - `opacity`: 0~100，用户滑块值
/// - `color`: (r, g, b) 背景色调
/// - `effect`: 0=无, 1=渐变, 2=透明渐变, 3=模糊, 4=亚克力, 5=云母
pub fn enable_acrylic(hwnd: isize, opacity: u8, color: (u8, u8, u8), effect: u8) {
    let (r, g, b) = color;
    let user = opacity.min(100) as u32;
    let alpha: u32 = (user * 204) / 100;

    // 云母用 DWM API 单独处理
    if effect == 5 {
        // 4 = Tabbed, 3 = Acrylic, 2 = Mica
        // Win11 22H2+ 支持 DWMWA_SYSTEMBACKDROP_TYPE
        set_dwm_backdrop(hwnd, 2); // 2 = Mica
                                   // 同时也调用 SetWindowCompositionAttribute 让老系统也有点效果
                                   // 但用 AccentState = 4
    } else {
        // 非云母，清除 DWM 效果
        set_dwm_backdrop(hwnd, 1); // 1 = None
    }

    let Some(func) = get_set_wca_fn() else {
        return;
    };

    let accent_state: i32 = match effect {
        0 => 0,
        2 => 2, // 透明渐变
        4 => 4, // 亚克力
        5 => 4, // 云母（用亚克力兜底，配合 DWM）
        _ => 4,
    };

    let accent_state = if accent_state == 0 || alpha < 5 {
        0
    } else {
        accent_state
    };

    let gradient = (alpha << 24) | ((b as u32) << 16) | ((g as u32) << 8) | (r as u32);

    let mut accent = AccentPolicy {
        accent_state,
        accent_flags: 0,
        gradient_color: gradient,
        animation_id: 0,
    };

    let mut data = WindowCompositionAttributeData {
        attribute: 19,
        data: &mut accent as *mut _ as *mut c_void,
        size_of_data: std::mem::size_of::<AccentPolicy>(),
    };

    unsafe {
        func(hwnd, &mut data);
    }
}

// ============================================
//  管理员权限检测 + 提权
// ============================================

#[cfg(target_os = "windows")]
#[link(name = "shell32")]
extern "system" {
    fn IsUserAnAdmin() -> i32;
    fn ShellExecuteW(
        hwnd: isize,
        lp_operation: *const u16,
        lp_file: *const u16,
        lp_parameters: *const u16,
        lp_directory: *const u16,
        n_show_cmd: i32,
    ) -> isize;
}

#[cfg(all(target_os = "windows", not(debug_assertions)))]
#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(hwnd: isize, text: *const u16, caption: *const u16, flags: u32) -> i32;
}

#[cfg(target_os = "windows")]
pub fn is_admin() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

#[cfg(not(target_os = "windows"))]
pub fn is_admin() -> bool {
    false
}

#[cfg(target_os = "windows")]
pub fn run_as_admin() -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_wide: Vec<u16> = OsStr::new(&exe)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let op_wide: Vec<u16> = OsStr::new("runas")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let result = ShellExecuteW(
            0,
            op_wide.as_ptr(),
            exe_wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        );
        if result <= 32 {
            return Err(format!("提权失败 (code {})", result));
        }
    }
    Ok(())
}

#[cfg(all(target_os = "windows", not(debug_assertions)))]
pub fn show_admin_error(message: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let text: Vec<u16> = OsStr::new(message)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let caption: Vec<u16> = OsStr::new("CrafterBox")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        MessageBoxW(0, text.as_ptr(), caption.as_ptr(), 0x10);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn run_as_admin() -> Result<(), String> {
    Err("仅支持 Windows".to_string())
}
