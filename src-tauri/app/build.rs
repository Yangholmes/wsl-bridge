fn main() {
    if std::env::var_os("CARGO_FEATURE_TAURI").is_some() {
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=windows/app.standard.manifest");
        println!("cargo:rerun-if-changed=windows/app.su.manifest");
        println!("cargo:rerun-if-env-changed=WSL_BRIDGE_BUILD_FLAVOR");

        let flavor =
            std::env::var("WSL_BRIDGE_BUILD_FLAVOR").unwrap_or_else(|_| "standard".to_owned());
        println!("cargo:rustc-env=WSL_BRIDGE_BUILD_FLAVOR={flavor}");

        let manifest = match flavor.as_str() {
            "su" => include_str!("windows/app.su.manifest"),
            _ => include_str!("windows/app.standard.manifest"),
        };

        let windows = tauri_build::WindowsAttributes::new().app_manifest(manifest);
        let attrs = tauri_build::Attributes::new().windows_attributes(windows);

        tauri_build::try_build(attrs).expect("failed to run tauri build script");
    } else {
        println!("cargo:rerun-if-changed=build.rs");
    }
}
