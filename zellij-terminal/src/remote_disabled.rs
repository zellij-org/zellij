use anyhow::{bail, Result};
use zellij_utils::cli::{AttachArgs, CliArgs, TerminalArgs};

use crate::launch::{Refusal, Target};
use crate::session::connection::{Capabilities, Connection, Geometry};

pub type LocalConfig = ();

pub fn watched_config(_opts: &CliArgs) -> LocalConfig {}

pub fn target(
    _args: &TerminalArgs,
    _capabilities: Capabilities,
) -> std::result::Result<Target, Refusal> {
    Err(Refusal::new(
        "This zellij was built without web server support, so it cannot reach a remote session. \
         Rebuild it with the `web_server_capability` feature.",
    ))
}

pub fn connect(
    _attach: &AttachArgs,
    _geometry: Geometry,
    _local_config: LocalConfig,
) -> Result<Connection> {
    bail!("This zellij was built without web server support, so it cannot reach a remote session.")
}
