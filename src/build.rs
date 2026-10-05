fn main() {
    // The clap-derived `augment_subcommands` for `CliAction` (~70 variants)
    // produces a >1 MB stack frame in debug mode, overflowing the Windows
    // default 1 MB main-thread stack.  Increase it to 8 MB to match Linux.
    // Release builds optimize the frame down, so this is only needed for non-release profiles.
    if cfg!(target_os = "windows") && std::env::var("PROFILE").unwrap_or_default() != "release" {
        println!("cargo:rustc-link-arg=/STACK:8388608");
    }

    // Embed the application icon and manifest into the Windows executables.
    #[cfg(target_os = "windows")]
    {
        // These files are outside the package's `include` list, so Cargo would not rerun this
        // script when they change unless told to watch them.
        for resource in [
            "assets/zellij.rc",
            "assets/zellij.manifest",
            "assets/logo128.ico",
        ] {
            println!("cargo:rerun-if-changed={}", resource);
        }
        let _ = embed_resource::compile("assets/zellij.rc", embed_resource::NONE);
    }
}
