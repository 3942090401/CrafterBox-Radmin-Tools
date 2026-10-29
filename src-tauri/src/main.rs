#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    if std::env::args_os().any(|argument| argument == "--crafterbox-registry-service") {
        if let Err(error) = crafterbox_lib::run_system_registry_service() {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if std::env::args_os().any(|argument| argument == "--uninstall-crafterbox-registry-service") {
        match crafterbox_lib::uninstall_system_registry_service() {
            Ok(()) => show_result("SYSTEM 注册表服务已卸载", false),
            Err(error) => {
                show_result(&format!("卸载 SYSTEM 注册表服务失败：{error}"), true);
                std::process::exit(1);
            }
        }
        return;
    }
    crafterbox_lib::run()
}

#[cfg(windows)]
fn show_result(message: &str, is_error: bool) {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: isize, text: *const u16, caption: *const u16, flags: u32) -> i32;
    }

    let text: Vec<u16> = std::ffi::OsStr::new(message)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let caption: Vec<u16> = "CrafterBox"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        MessageBoxW(
            0,
            text.as_ptr(),
            caption.as_ptr(),
            if is_error { 0x10 } else { 0x40 },
        );
    }
}

#[cfg(not(windows))]
fn show_result(message: &str, _is_error: bool) {
    println!("{message}");
}
