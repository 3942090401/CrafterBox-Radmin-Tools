fn main() {
    if !cfg!(debug_assertions) && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let windows =
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("windows.manifest"));
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
            .expect("failed to run Tauri build script");
    } else {
        tauri_build::build();
    }
}
