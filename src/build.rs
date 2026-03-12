fn main() {
    // The clap-derived `augment_subcommands` for `CliAction` (~70 variants)
    // produces a >1 MB stack frame in debug mode, overflowing the Windows
    // default 1 MB main-thread stack.  Increase it to 8 MB to match Linux.
    // Release builds optimize the frame down, so this is only needed for non-release profiles.
    if cfg!(target_os = "windows") && std::env::var("PROFILE").unwrap_or_default() != "release" {
        println!("cargo:rustc-link-arg=/STACK:8388608");
    }
}
