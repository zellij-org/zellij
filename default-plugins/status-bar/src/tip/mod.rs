pub mod cache;
pub mod consts;
pub mod data;
pub mod utils;

use crate::LinePart;
use zellij_tile::prelude::*;

pub type TipFn = fn(&ModeInfo) -> LinePart;

pub struct TipBody {
    pub short: TipFn,
    pub medium: TipFn,
    pub full: TipFn,
}
