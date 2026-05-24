#[cfg(not(target_family = "wasm"))]
use crate::consts::ASSET_MAP;
use crate::input::theme::Themes;
#[allow(unused_imports)]
use crate::{
    cli::{CliArgs, Command, SessionCommand, Sessions},
    consts::{FEATURES, VERSION, ZELLIJ_CACHE_DIR, ZELLIJ_DEFAULT_THEMES},
    data::LayoutInfo,
    errors::prelude::*,
    home::*,
    input::{
        config::{Config, ConfigError},
        layout::Layout,
        options::Options,
    },
};
use clap::{Args, IntoApp};
use clap_complete::Shell;
use log::info;
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, fmt::Write as FmtWrite, fs, io::Write, path::PathBuf, process};

const CONFIG_NAME: &str = "config.kdl";
static ARROW_SEPARATOR: &str = "";
