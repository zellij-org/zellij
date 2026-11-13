use anyhow::{bail, Result};

pub const LOADS_SYSTEM_LIBRARIES: bool = !cfg!(all(
    target_os = "linux",
    target_env = "musl",
    target_feature = "crt-static"
));

pub const OPENS_WINDOWS: bool = LOADS_SYSTEM_LIBRARIES;

pub fn gnu_build_name() -> String {
    let variant = if cfg!(feature = "web_server_capability") {
        ""
    } else {
        "no-web-"
    };
    format!(
        "zellij-{}{}-unknown-linux-gnu.tar.gz",
        variant,
        std::env::consts::ARCH
    )
}

pub fn ensure_windows() -> Result<()> {
    if OPENS_WINDOWS {
        return Ok(());
    }
    bail!(
        "This build of Zellij cannot open the desktop terminal. Use the GNU build (eg. {}) instead.",
        gnu_build_name()
    )
}
