use serde::{Deserialize, Serialize};
use zellij_utils::ipc::ServerToClientMsg;

pub const FIXTURE_FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Header {
    pub format_version: u32,
    pub contract_version: usize,
    pub session: String,
    pub rows: usize,
    pub cols: usize,
    pub cell_width: usize,
    pub cell_height: usize,
    pub recorded_at_unix_secs: u64,
}

#[derive(Serialize)]
pub struct MessageRef<'a> {
    pub ts_us: u64,
    pub msg: &'a ServerToClientMsg,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordRef<'a> {
    Header(&'a Header),
    Message(MessageRef<'a>),
}
