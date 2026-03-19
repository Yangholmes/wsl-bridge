use wsl_bridge_shared::{AppRuntimeStatus, BuildFlavor};

pub fn current_runtime_status() -> AppRuntimeStatus {
    let is_admin = detect_admin();
    AppRuntimeStatus {
        build_flavor: build_flavor(),
        is_admin,
        admin_features_available: is_admin,
    }
}

fn build_flavor() -> BuildFlavor {
    match option_env!("WSL_BRIDGE_BUILD_FLAVOR") {
        Some("su") => BuildFlavor::Su,
        _ => BuildFlavor::Standard,
    }
}

#[cfg(target_os = "windows")]
fn detect_admin() -> bool {
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

#[cfg(not(target_os = "windows"))]
fn detect_admin() -> bool {
    true
}
