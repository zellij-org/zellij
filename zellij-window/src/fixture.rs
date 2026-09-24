#[cfg(test)]
use std::fs::File;
#[cfg(test)]
use std::io::{BufRead, BufReader};
#[cfg(test)]
use std::path::Path;

#[cfg(test)]
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use zellij_utils::consts::CLIENT_SERVER_CONTRACT_VERSION;
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

#[cfg(test)]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Message {
    pub ts_us: u64,
    pub msg: ServerToClientMsg,
}

#[cfg(test)]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Record {
    Header(Header),
    Message(Message),
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

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub struct Fixture {
    pub header: Header,
    pub messages: Vec<Message>,
}

#[cfg(test)]
pub fn read(path: &Path) -> Result<Fixture> {
    let file = File::open(path).with_context(|| format!("failed to open fixture {:?}", path))?;
    parse(BufReader::new(file), &path.display().to_string())
}

#[cfg(test)]
pub fn parse<R: BufRead>(reader: R, origin: &str) -> Result<Fixture> {
    let mut header: Option<Header> = None;
    let mut messages = Vec::new();

    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("failed to read {} line {}", origin, index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let record: Record = serde_json::from_str(&line)
            .with_context(|| format!("undecodable record at {} line {}", origin, index + 1))?;
        match record {
            Record::Header(new_header) => {
                if header.is_some() {
                    bail!("{} carries more than one header record", origin);
                }
                if new_header.format_version != FIXTURE_FORMAT_VERSION {
                    bail!(
                        "{} is fixture format version {}, this build reads version {}",
                        origin,
                        new_header.format_version,
                        FIXTURE_FORMAT_VERSION
                    );
                }
                if new_header.contract_version != CLIENT_SERVER_CONTRACT_VERSION {
                    eprintln!(
                        "zellij-window: {} was recorded against client-server contract {}, this build carries {}",
                        origin, new_header.contract_version, CLIENT_SERVER_CONTRACT_VERSION
                    );
                }
                header = Some(new_header);
            },
            Record::Message(message) => {
                if header.is_none() {
                    bail!("{} carries a message before its header", origin);
                }
                messages.push(message);
            },
        }
    }

    match header {
        Some(header) => Ok(Fixture { header, messages }),
        None => bail!("{} carries no header record", origin),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_line(format_version: u32, contract_version: usize) -> String {
        serde_json::to_string(&Record::Header(Header {
            format_version,
            contract_version,
            session: "window-test".to_owned(),
            rows: 40,
            cols: 120,
            cell_width: 10,
            cell_height: 20,
            recorded_at_unix_secs: 0,
        }))
        .unwrap()
    }

    fn render_line(content: &str) -> String {
        serde_json::to_string(&Record::Message(Message {
            ts_us: 7,
            msg: ServerToClientMsg::Render {
                content: content.to_owned(),
            },
        }))
        .unwrap()
    }

    fn parse_str(text: &str) -> Result<Fixture> {
        parse(text.as_bytes(), "in-memory")
    }

    #[test]
    fn a_header_and_its_messages_round_trip_through_the_reader() {
        let text = format!(
            "{}\n{}\n{}\n",
            header_line(FIXTURE_FORMAT_VERSION, CLIENT_SERVER_CONTRACT_VERSION),
            render_line("first"),
            render_line("second")
        );
        let fixture = parse_str(&text).unwrap();
        assert_eq!(fixture.header.session, "window-test");
        assert_eq!((fixture.header.rows, fixture.header.cols), (40, 120));
        assert_eq!(
            (fixture.header.cell_width, fixture.header.cell_height),
            (10, 20)
        );
        assert_eq!(fixture.messages.len(), 2);
        assert_eq!(
            fixture.messages[1].msg,
            ServerToClientMsg::Render {
                content: "second".to_owned()
            }
        );
    }

    #[test]
    fn blank_lines_are_tolerated() {
        let text = format!(
            "{}\n\n{}\n\n",
            header_line(FIXTURE_FORMAT_VERSION, CLIENT_SERVER_CONTRACT_VERSION),
            render_line("only")
        );
        assert_eq!(parse_str(&text).unwrap().messages.len(), 1);
    }

    #[test]
    fn a_foreign_fixture_format_is_refused() {
        let text = format!("{}\n", header_line(FIXTURE_FORMAT_VERSION + 1, 1));
        assert!(parse_str(&text).is_err());
    }

    #[test]
    fn a_contract_mismatch_is_a_warning_rather_than_a_failure() {
        let text = format!(
            "{}\n{}\n",
            header_line(FIXTURE_FORMAT_VERSION, CLIENT_SERVER_CONTRACT_VERSION + 1),
            render_line("still readable")
        );
        assert_eq!(parse_str(&text).unwrap().messages.len(), 1);
    }

    #[test]
    fn a_headerless_or_undecodable_fixture_is_refused() {
        assert!(parse_str(&format!("{}\n", render_line("orphan"))).is_err());
        assert!(parse_str("").is_err());
        assert!(parse_str("{\"kind\":\"nonsense\"}\n").is_err());
    }

    #[test]
    fn the_borrowed_and_owned_record_shapes_serialize_identically() {
        let header = Header {
            format_version: FIXTURE_FORMAT_VERSION,
            contract_version: CLIENT_SERVER_CONTRACT_VERSION,
            session: "window-test".to_owned(),
            rows: 40,
            cols: 120,
            cell_width: 10,
            cell_height: 20,
            recorded_at_unix_secs: 0,
        };
        assert_eq!(
            serde_json::to_string(&RecordRef::Header(&header)).unwrap(),
            serde_json::to_string(&Record::Header(header.clone())).unwrap()
        );

        let msg = ServerToClientMsg::Render {
            content: "payload".to_owned(),
        };
        assert_eq!(
            serde_json::to_string(&RecordRef::Message(MessageRef {
                ts_us: 7,
                msg: &msg
            }))
            .unwrap(),
            serde_json::to_string(&Record::Message(Message {
                ts_us: 7,
                msg: msg.clone()
            }))
            .unwrap()
        );
    }
}
