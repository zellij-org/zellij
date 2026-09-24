use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use zellij_utils::consts::CLIENT_SERVER_CONTRACT_VERSION;
use zellij_utils::ipc::ServerToClientMsg;

use crate::connection::Geometry;
use crate::fixture::{Header, MessageRef, RecordRef, FIXTURE_FORMAT_VERSION};

pub struct Recorder {
    writer: BufWriter<File>,
    started_at: Instant,
    message_count: usize,
}

impl Recorder {
    pub fn create(path: &Path, session: &str, geometry: Geometry) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {:?}", parent))?;
            }
        }
        let file =
            File::create(path).with_context(|| format!("failed to create fixture {:?}", path))?;
        let mut recorder = Self {
            writer: BufWriter::new(file),
            started_at: Instant::now(),
            message_count: 0,
        };
        let recorded_at_unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let header = Header {
            format_version: FIXTURE_FORMAT_VERSION,
            contract_version: CLIENT_SERVER_CONTRACT_VERSION,
            session: session.to_owned(),
            rows: geometry.rows,
            cols: geometry.cols,
            cell_width: geometry.cell_width,
            cell_height: geometry.cell_height,
            recorded_at_unix_secs,
        };
        recorder.write(&RecordRef::Header(&header))?;
        Ok(recorder)
    }

    pub fn record(&mut self, msg: &ServerToClientMsg) -> Result<()> {
        let ts_us = self.started_at.elapsed().as_micros() as u64;
        self.write(&RecordRef::Message(MessageRef { ts_us, msg }))?;
        self.message_count += 1;
        Ok(())
    }

    pub fn message_count(&self) -> usize {
        self.message_count
    }

    pub fn finish(mut self) -> Result<()> {
        self.writer.flush().context("failed to flush fixture")
    }

    fn write(&mut self, record: &RecordRef<'_>) -> Result<()> {
        let line = serde_json::to_string(record).context("failed to serialize fixture record")?;
        self.writer
            .write_all(line.as_bytes())
            .and_then(|_| self.writer.write_all(b"\n"))
            .context("failed to write fixture record")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tempfile::TempDir;
    use zellij_utils::ipc::ExitReason;

    fn geometry() -> Geometry {
        Geometry {
            rows: 40,
            cols: 120,
            cell_width: 10,
            cell_height: 20,
        }
    }

    fn record_all(msgs: &[ServerToClientMsg]) -> (TempDir, Vec<Value>) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("fixture.jsonl");
        let mut recorder = Recorder::create(&path, "window-test", geometry()).unwrap();
        for msg in msgs {
            recorder.record(msg).unwrap();
        }
        assert_eq!(recorder.message_count(), msgs.len());
        recorder.finish().unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        let records = contents
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        (dir, records)
    }

    #[test]
    fn the_header_describes_the_capture_geometry_and_contract() {
        let (_dir, records) = record_all(&[]);
        assert_eq!(records.len(), 1);
        let header = &records[0];
        assert_eq!(header["kind"], "header");
        assert_eq!(header["format_version"], FIXTURE_FORMAT_VERSION);
        assert_eq!(header["contract_version"], CLIENT_SERVER_CONTRACT_VERSION);
        assert_eq!(header["session"], "window-test");
        assert_eq!(header["rows"], 40);
        assert_eq!(header["cols"], 120);
        assert_eq!(header["cell_width"], 10);
        assert_eq!(header["cell_height"], 20);
    }

    #[test]
    fn every_message_variant_is_recorded_in_arrival_order() {
        let msgs = vec![
            ServerToClientMsg::UnblockInputThread,
            ServerToClientMsg::Render {
                content: "first".to_owned(),
            },
            ServerToClientMsg::QueryTerminalSize,
            ServerToClientMsg::Exit {
                exit_reason: ExitReason::NormalDetached,
            },
        ];
        let (_dir, records) = record_all(&msgs);
        assert_eq!(records.len(), msgs.len() + 1);
        for (record, expected) in records[1..].iter().zip(&msgs) {
            assert_eq!(record["kind"], "message");
            let decoded: ServerToClientMsg = serde_json::from_value(record["msg"].clone()).unwrap();
            assert_eq!(&decoded, expected);
        }
    }

    #[test]
    fn render_payloads_survive_the_round_trip_byte_for_byte() {
        let content = "\u{1b}[?25l\u{1b}[1;31mRED\u{1b}[m \u{1b}]8;;\u{1b}\\ \u{9b}odd\u{7f}";
        let msgs = vec![ServerToClientMsg::Render {
            content: content.to_owned(),
        }];
        let (_dir, records) = record_all(&msgs);
        let decoded: ServerToClientMsg = serde_json::from_value(records[1]["msg"].clone()).unwrap();
        match decoded {
            ServerToClientMsg::Render { content: decoded } => assert_eq!(decoded, content),
            other => panic!("expected a render payload, got {:?}", other),
        }
    }

    #[test]
    fn timestamps_are_monotonic_offsets_from_the_start_of_the_capture() {
        let msgs = vec![
            ServerToClientMsg::UnblockInputThread,
            ServerToClientMsg::UnblockInputThread,
            ServerToClientMsg::UnblockInputThread,
        ];
        let (_dir, records) = record_all(&msgs);
        let stamps: Vec<u64> = records[1..]
            .iter()
            .map(|r| r["ts_us"].as_u64().unwrap())
            .collect();
        assert!(stamps.windows(2).all(|w| w[0] <= w[1]), "{:?}", stamps);
    }
}
