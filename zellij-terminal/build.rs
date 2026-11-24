const BUNDLED_EMOJI: &str = "assets/fonts/NotoColorEmoji.ttf";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/fonts");
    println!("cargo:rustc-check-cfg=cfg(freedesktop)");
    println!("cargo:rustc-check-cfg=cfg(bundled_emoji)");
    let unix = std::env::var_os("CARGO_CFG_UNIX").is_some();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if unix && os != "macos" && os != "android" {
        println!("cargo:rustc-cfg=freedesktop");
    }
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default();
    if std::path::Path::new(&manifest_dir)
        .join(BUNDLED_EMOJI)
        .is_file()
    {
        println!("cargo:rustc-cfg=bundled_emoji");
    }
}
