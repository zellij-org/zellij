fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-check-cfg=cfg(freedesktop)");
    let unix = std::env::var_os("CARGO_CFG_UNIX").is_some();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if unix && os != "macos" && os != "android" {
        println!("cargo:rustc-cfg=freedesktop");
    }
}
