use base64::alphabet::STANDARD as BASE64_STANDARD_ALPHABET;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use base64::engine::{DecodePaddingMode, Engine as _};
use std::collections::BTreeMap;

use zellij_utils::attention::{AttentionStatus, BlockedKind, ProgramState, ProgramStatusRecord};

pub const MAX_SEQUENCE_BYTES: usize = 4096;
pub const MAX_KEY_BYTES: usize = 16;
pub const MAX_MSG_ENCODED_BYTES: usize = 2732;
pub const MAX_MSG_DECODED_BYTES: usize = 2048;
pub const MAX_TITLE_ENCODED_BYTES: usize = 256;
pub const MAX_TITLE_DECODED_BYTES: usize = 192;
pub const MAX_APP_BYTES: usize = 32;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_ID_SEGMENT_BYTES: usize = 32;
pub const MAX_ID_DEPTH: usize = 8;
pub const MAX_RECORDS: usize = 256;

pub const QUERY_REPLY_BODY: &str =
    "?:states=idle,working,done,blocked,error:kinds=permission,question,auth";

const BASE64_DECODER: GeneralPurpose = GeneralPurpose::new(
    &BASE64_STANDARD_ALPHABET,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramStatusMessage {
    Query,
    Set(ProgramStatusRecord),
    Clear { id: Option<String> },
}

pub fn query_reply(bell_terminated: bool) -> Vec<u8> {
    let terminator = if bell_terminated { "\x07" } else { "\x1b\\" };
    format!("\x1b]7501;{}{}", QUERY_REPLY_BODY, terminator).into_bytes()
}

fn sequence_length(params: &[&[u8]], bell_terminated: bool) -> usize {
    let separators = params.len().saturating_sub(1);
    let body: usize = params.iter().map(|p| p.len()).sum();
    let terminator = if bell_terminated { 1 } else { 2 };
    2 + body + separators + terminator
}

fn is_value_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b',' | b'+' | b'/' | b'=' | b'-')
}

fn is_app_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'+' | b'-')
}

pub fn valid_id(id: &str) -> bool {
    if id.is_empty() || id.len() > MAX_ID_BYTES {
        return false;
    }
    let segments: Vec<&str> = id.split('/').collect();
    if segments.len() > MAX_ID_DEPTH {
        return false;
    }
    segments.iter().all(|segment| {
        !segment.is_empty()
            && segment.len() <= MAX_ID_SEGMENT_BYTES
            && segment.bytes().all(is_app_byte)
    })
}

fn has_forbidden_char(text: &str) -> bool {
    text.chars().any(|c| {
        let code = c as u32;
        code <= 0x1f || code == 0x7f || (0x80..=0x9f).contains(&code)
    })
}

fn decode_text(value: &str, max_encoded: usize, max_decoded: usize) -> Result<Option<String>, ()> {
    if value.len() > max_encoded {
        return Err(());
    }
    if value.is_empty() {
        return Ok(None);
    }
    let bytes = BASE64_DECODER.decode(value.as_bytes()).map_err(|_| ())?;
    if bytes.len() > max_decoded {
        return Err(());
    }
    let text = String::from_utf8(bytes).map_err(|_| ())?;
    if has_forbidden_char(&text) {
        return Err(());
    }
    if text.is_empty() {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}

pub fn parse(params: &[&[u8]], bell_terminated: bool) -> Option<ProgramStatusMessage> {
    if sequence_length(params, bell_terminated) > MAX_SEQUENCE_BYTES {
        return None;
    }
    let body_bytes = params.get(1..)?.join(&b';');
    let body = std::str::from_utf8(&body_bytes).ok()?;
    if body.trim() == "?" {
        return Some(ProgramStatusMessage::Query);
    }
    let mut pairs: BTreeMap<&str, &str> = BTreeMap::new();
    for pair in body.split(':') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_lowercase()) {
            continue;
        }
        if key.len() > MAX_KEY_BYTES {
            return None;
        }
        if !value.bytes().all(is_value_byte) {
            continue;
        }
        pairs.insert(key, value);
    }
    let state_value = *pairs.get("state")?;
    let state = if state_value == "clear" {
        None
    } else {
        Some(ProgramState::from_str(state_value)?)
    };
    let id = match pairs.get("id") {
        Some(id) => {
            if !valid_id(id) {
                return None;
            }
            Some((*id).to_owned())
        },
        None => None,
    };
    let app = match pairs.get("app") {
        Some(app) if app.len() > MAX_APP_BYTES => return None,
        Some(app) if !app.is_empty() && app.bytes().all(is_app_byte) => Some((*app).to_owned()),
        _ => None,
    };
    let title = match pairs.get("title") {
        Some(title) => decode_text(title, MAX_TITLE_ENCODED_BYTES, MAX_TITLE_DECODED_BYTES).ok()?,
        None => None,
    };
    let msg = match pairs.get("msg") {
        Some(msg) => decode_text(msg, MAX_MSG_ENCODED_BYTES, MAX_MSG_DECODED_BYTES).ok()?,
        None => None,
    };
    let Some(state) = state else {
        return Some(ProgramStatusMessage::Clear { id });
    };
    let kind = if state == ProgramState::Blocked {
        pairs.get("kind").and_then(|k| BlockedKind::from_str(k))
    } else {
        None
    };
    let progress = if matches!(state, ProgramState::Working | ProgramState::Blocked) {
        pairs
            .get("progress")
            .filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|p| p.parse::<u32>().ok())
            .filter(|p| *p <= 100)
            .map(|p| p as u8)
    } else {
        None
    };
    Some(ProgramStatusMessage::Set(ProgramStatusRecord {
        id,
        state: Some(state),
        kind,
        progress,
        app,
        title,
        msg,
        updated_ms: 0,
    }))
}

#[derive(Debug, Clone, Default)]
pub struct ProgramStatusRecords {
    records: BTreeMap<String, (u64, ProgramStatusRecord)>,
    sequence: u64,
    working_since: BTreeMap<String, u64>,
    worked: BTreeMap<String, u64>,
}

fn record_key(id: &Option<String>) -> String {
    id.clone().unwrap_or_default()
}

impl ProgramStatusRecords {
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    pub fn len(&self) -> usize {
        self.records.len()
    }
    pub fn root(&self) -> Option<&ProgramStatusRecord> {
        self.records.get("").map(|(_, r)| r)
    }
    pub fn get(&self, id: &str) -> Option<&ProgramStatusRecord> {
        self.records.get(id).map(|(_, r)| r)
    }
    pub fn iter(&self) -> impl Iterator<Item = &ProgramStatusRecord> {
        self.records.values().map(|(_, r)| r)
    }
    pub fn all(&self) -> Vec<ProgramStatusRecord> {
        self.records
            .values()
            .map(|(_, r)| {
                let mut record = r.clone();
                if record.app.is_none() {
                    record.app = self.inherited_app(&record.id);
                }
                record
            })
            .collect()
    }
    pub fn worked_ms(&self, id: &Option<String>) -> Option<u64> {
        self.worked.get(&record_key(id)).copied()
    }
    fn follow_work(&mut self, key: &str, state: ProgramState, now_ms: u64) {
        match state {
            ProgramState::Working | ProgramState::Blocked => {
                self.working_since.entry(key.to_owned()).or_insert(now_ms);
                self.worked.remove(key);
            },
            ProgramState::Done | ProgramState::Error => {
                if let Some(started) = self.working_since.remove(key) {
                    self.worked
                        .insert(key.to_owned(), now_ms.saturating_sub(started));
                }
            },
            ProgramState::Idle => {
                self.working_since.remove(key);
                self.worked.remove(key);
            },
        }
    }
    fn forget_missing(&mut self) {
        let records = &self.records;
        self.working_since
            .retain(|key, _| records.contains_key(key));
        self.worked.retain(|key, _| records.contains_key(key));
    }
    pub fn apply(&mut self, message: ProgramStatusMessage, now_ms: u64) -> bool {
        let changed = self.apply_message(message, now_ms);
        self.forget_missing();
        changed
    }
    fn apply_message(&mut self, message: ProgramStatusMessage, now_ms: u64) -> bool {
        match message {
            ProgramStatusMessage::Query => false,
            ProgramStatusMessage::Clear { id: None } => {
                let changed = !self.records.is_empty();
                self.records.clear();
                changed
            },
            ProgramStatusMessage::Clear { id: Some(id) } => {
                let prefix = format!("{}/", id);
                let before = self.records.len();
                self.records
                    .retain(|key, _| key != &id && !key.starts_with(&prefix));
                before != self.records.len()
            },
            ProgramStatusMessage::Set(mut record) => {
                record.updated_ms = now_ms;
                let key = record_key(&record.id);
                if !self.records.contains_key(&key) && self.records.len() >= MAX_RECORDS {
                    if let Some(oldest) = self
                        .records
                        .iter()
                        .min_by_key(|(_, (seq, _))| *seq)
                        .map(|(k, _)| k.clone())
                    {
                        self.records.remove(&oldest);
                    }
                }
                self.sequence += 1;
                self.follow_work(&key, record.state(), now_ms);
                let changed = self
                    .records
                    .get(&key)
                    .map(|(_, existing)| {
                        let mut existing = existing.clone();
                        existing.updated_ms = now_ms;
                        existing != record
                    })
                    .unwrap_or(true);
                self.records.insert(key, (self.sequence, record));
                changed
            },
        }
    }
    pub fn clear_all(&mut self) -> bool {
        let changed = !self.records.is_empty();
        self.records.clear();
        self.forget_missing();
        changed
    }
    pub fn drop_transient(&mut self) -> bool {
        let before = self.records.len();
        self.records.retain(|_, (_, r)| !r.state().is_transient());
        self.forget_missing();
        before != self.records.len()
    }
    pub fn inherited_app(&self, id: &Option<String>) -> Option<String> {
        let parent = |path: &str| path.rsplit_once('/').map(|(parent, _)| parent.to_owned());
        let mut current = match id {
            None => return None,
            Some(path) => parent(path),
        };
        loop {
            let key = record_key(&current);
            if let Some(app) = self.records.get(&key).and_then(|(_, r)| r.app.clone()) {
                return Some(app);
            }
            match current {
                None => return None,
                Some(path) => current = parent(&path),
            }
        }
    }
    pub fn deciding_record(&self) -> Option<(AttentionStatus, ProgramStatusRecord)> {
        let root = self.root().cloned();
        let mut best: Option<(AttentionStatus, ProgramStatusRecord)> =
            root.map(|r| (r.state().attention_status(), r));
        for (key, (_, record)) in self.records.iter() {
            if key.is_empty() {
                continue;
            }
            let child_status = match record.state() {
                ProgramState::Blocked => AttentionStatus::NeedsYou,
                ProgramState::Error => AttentionStatus::Failed,
                _ => continue,
            };
            let raises = match &best {
                None => true,
                Some((status, _)) => child_status > *status,
            };
            if raises {
                best = Some((child_status, record.clone()));
            }
        }
        best.map(|(status, mut record)| {
            if record.app.is_none() {
                record.app = self.inherited_app(&record.id);
            }
            (status, record)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;

    fn b64(text: &str) -> String {
        STANDARD.encode(text)
    }

    fn parse_body(body: &str) -> Option<ProgramStatusMessage> {
        let mut params: Vec<&[u8]> = vec![b"7501"];
        params.extend(body.split(';').map(|p| p.as_bytes()));
        parse(&params, false)
    }

    fn set(body: &str) -> ProgramStatusRecord {
        match parse_body(body) {
            Some(ProgramStatusMessage::Set(record)) => record,
            other => panic!("expected a record, got {:?}", other),
        }
    }

    #[test]
    fn query_is_recognized() {
        assert_eq!(parse_body("?"), Some(ProgramStatusMessage::Query));
        assert_eq!(
            query_reply(false),
            b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\".to_vec()
        );
    }

    #[test]
    fn full_report_is_parsed() {
        let record = set(&format!(
            "state=blocked:kind=permission:app=terraform:progress=40:msg={}:title={}",
            b64("Apply 3 to add?"),
            b64("Plan")
        ));
        assert_eq!(record.state, Some(ProgramState::Blocked));
        assert_eq!(record.kind, Some(BlockedKind::Permission));
        assert_eq!(record.app.as_deref(), Some("terraform"));
        assert_eq!(record.progress, Some(40));
        assert_eq!(record.msg.as_deref(), Some("Apply 3 to add?"));
        assert_eq!(record.title.as_deref(), Some("Plan"));
        assert_eq!(record.id, None);
    }

    #[test]
    fn whitespace_is_trimmed_and_last_key_wins() {
        let record = set(" state = working : app=a : app = b ");
        assert_eq!(record.state, Some(ProgramState::Working));
        assert_eq!(record.app.as_deref(), Some("b"));
    }

    #[test]
    fn malformed_pairs_are_skipped() {
        let record = set("state=done:nonsense:=x:APP=y:app=ok:bad=a b");
        assert_eq!(record.state, Some(ProgramState::Done));
        assert_eq!(record.app.as_deref(), Some("ok"));
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let record = set("state=idle:future=1");
        assert_eq!(record.state, Some(ProgramState::Idle));
    }

    #[test]
    fn missing_or_unknown_state_is_ignored() {
        assert_eq!(parse_body("app=x"), None);
        assert_eq!(parse_body("state=sleeping"), None);
    }

    #[test]
    fn kind_and_progress_only_apply_to_their_states() {
        let record = set("state=working:kind=auth:progress=101");
        assert_eq!(record.kind, None);
        assert_eq!(record.progress, None);
        let record = set("state=done:progress=50");
        assert_eq!(record.progress, None);
        let record = set("state=blocked:kind=unknown:progress=7");
        assert_eq!(record.kind, None);
        assert_eq!(record.progress, Some(7));
        let record = set("state=working:progress=-1");
        assert_eq!(record.progress, None);
    }

    #[test]
    fn app_outside_charset_is_absent_and_too_long_discards() {
        assert_eq!(set("state=idle:app=a,b").app, None);
        assert_eq!(
            parse_body(&format!("state=idle:app={}", "a".repeat(33))),
            None
        );
    }

    #[test]
    fn invalid_ids_ignore_the_report() {
        assert_eq!(parse_body("state=idle:id=a//b"), None);
        assert_eq!(parse_body("state=idle:id=a,b"), None);
        assert_eq!(
            parse_body(&format!("state=idle:id={}", "a".repeat(33))),
            None
        );
        assert_eq!(parse_body("state=idle:id=1/2/3/4/5/6/7/8/9"), None);
        let long_id = vec!["abcdefghijklmno"; 8].join("/");
        assert_eq!(long_id.len(), 127);
        assert!(parse_body(&format!("state=idle:id={}", long_id)).is_some());
        let too_long = vec!["abcdefghijklmnop"; 8].join("/");
        assert_eq!(parse_body(&format!("state=idle:id={}", too_long)), None);
        assert_eq!(
            set("state=idle:id=build/test").id.as_deref(),
            Some("build/test")
        );
    }

    #[test]
    fn control_characters_in_text_discard_the_report() {
        assert_eq!(parse_body(&format!("state=idle:msg={}", b64("a\nb"))), None);
        assert_eq!(
            parse_body(&format!("state=idle:msg={}", b64("a\u{85}b"))),
            None
        );
        assert_eq!(
            parse_body(&format!("state=idle:title={}", b64("\u{7f}"))),
            None
        );
        assert_eq!(set("state=idle:msg=!!!").msg, None);
        assert_eq!(parse_body("state=idle:msg=_-_-"), None);
    }

    #[test]
    fn padding_is_optional() {
        let record = set("state=idle:msg=YQ");
        assert_eq!(record.msg.as_deref(), Some("a"));
    }

    #[test]
    fn limits_discard_the_report() {
        let long_key = format!("state=idle:{}=1", "a".repeat(17));
        assert_eq!(parse_body(&long_key), None);
        let msg = b64(&"x".repeat(2049));
        assert_eq!(parse_body(&format!("state=idle:msg={}", msg)), None);
        let msg = b64(&"x".repeat(2048));
        assert!(parse_body(&format!("state=idle:msg={}", msg)).is_some());
        let title = b64(&"x".repeat(193));
        assert_eq!(parse_body(&format!("state=idle:title={}", title)), None);
        let huge = format!("state=idle:pad={}", "a".repeat(4100));
        assert_eq!(parse_body(&huge), None);
        let encoded_too_long = format!("state=idle:msg={}", "QUFB".repeat(684));
        assert_eq!(parse_body(&encoded_too_long), None);
    }

    #[test]
    fn clear_reports() {
        assert_eq!(
            parse_body("state=clear"),
            Some(ProgramStatusMessage::Clear { id: None })
        );
        assert_eq!(
            parse_body("state=clear:id=a/b"),
            Some(ProgramStatusMessage::Clear {
                id: Some("a/b".to_owned())
            })
        );
        assert_eq!(parse_body(&format!("state=clear:msg={}", b64("\n"))), None);
    }

    #[test]
    fn reports_replace_records_completely() {
        let mut records = ProgramStatusRecords::default();
        records.apply(parse_body("state=working:app=x:msg=YQ").unwrap(), 1);
        records.apply(parse_body("state=working").unwrap(), 2);
        let root = records.root().unwrap();
        assert_eq!(root.app, None);
        assert_eq!(root.msg, None);
    }

    #[test]
    fn clear_removes_descendants() {
        let mut records = ProgramStatusRecords::default();
        for body in [
            "state=working",
            "state=working:id=a",
            "state=working:id=a/b",
            "state=working:id=ab",
            "state=working:id=c",
        ] {
            records.apply(parse_body(body).unwrap(), 1);
        }
        records.apply(parse_body("state=clear:id=a").unwrap(), 2);
        let ids: Vec<Option<String>> = records.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, vec![None, Some("ab".to_owned()), Some("c".to_owned())]);
        records.apply(parse_body("state=clear").unwrap(), 3);
        assert!(records.is_empty());
    }

    #[test]
    fn app_is_inherited_from_the_nearest_ancestor() {
        let mut records = ProgramStatusRecords::default();
        records.apply(parse_body("state=working:app=deploy").unwrap(), 1);
        records.apply(parse_body("state=working:id=a:app=inner").unwrap(), 1);
        records.apply(parse_body("state=blocked:id=a/b/c").unwrap(), 1);
        records.apply(parse_body("state=blocked:id=z").unwrap(), 1);
        assert_eq!(
            records.inherited_app(&Some("a/b/c".to_owned())).as_deref(),
            Some("inner")
        );
        assert_eq!(
            records.inherited_app(&Some("z".to_owned())).as_deref(),
            Some("deploy")
        );
    }

    #[test]
    fn least_recently_updated_record_is_evicted() {
        let mut records = ProgramStatusRecords::default();
        for i in 0..MAX_RECORDS {
            records.apply(parse_body(&format!("state=working:id=r{}", i)).unwrap(), 1);
        }
        records.apply(parse_body("state=done:id=r0").unwrap(), 2);
        records.apply(parse_body("state=working:id=new").unwrap(), 3);
        assert_eq!(records.len(), MAX_RECORDS);
        assert!(records.get("r0").is_some());
        assert!(records.get("r1").is_none());
        assert!(records.get("new").is_some());
    }

    #[test]
    fn transient_records_are_dropped() {
        let mut records = ProgramStatusRecords::default();
        records.apply(parse_body("state=working").unwrap(), 1);
        records.apply(parse_body("state=blocked:id=a").unwrap(), 1);
        records.apply(parse_body("state=idle:id=b").unwrap(), 1);
        records.apply(parse_body("state=done:id=c").unwrap(), 1);
        records.apply(parse_body("state=error:id=d").unwrap(), 1);
        records.drop_transient();
        let ids: Vec<Option<String>> = records.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, vec![Some("c".to_owned()), Some("d".to_owned())]);
    }

    #[test]
    fn children_raise_the_pane_status() {
        let mut records = ProgramStatusRecords::default();
        records.apply(parse_body("state=working:app=deploy").unwrap(), 1);
        records.apply(parse_body("state=blocked:id=eu:msg=YQ").unwrap(), 1);
        let (status, record) = records.deciding_record().unwrap();
        assert_eq!(status, AttentionStatus::NeedsYou);
        assert_eq!(record.id.as_deref(), Some("eu"));
        assert_eq!(record.app.as_deref(), Some("deploy"));
        records.apply(parse_body("state=clear:id=eu").unwrap(), 2);
        let (status, record) = records.deciding_record().unwrap();
        assert_eq!(status, AttentionStatus::Working);
        assert_eq!(record.id, None);
    }

    #[test]
    fn a_finished_program_remembers_how_long_it_worked() {
        let mut records = ProgramStatusRecords::default();
        records.apply(parse_body("state=working").unwrap(), 1_000);
        records.apply(parse_body("state=blocked:kind=question").unwrap(), 4_000);
        records.apply(parse_body("state=working").unwrap(), 6_000);
        assert_eq!(records.worked_ms(&None), None);
        records.apply(parse_body("state=done").unwrap(), 13_000);
        assert_eq!(records.worked_ms(&None), Some(12_000));
        records.apply(parse_body("state=done").unwrap(), 20_000);
        assert_eq!(
            records.worked_ms(&None),
            Some(12_000),
            "repeating done does not restart the clock"
        );
        records.apply(parse_body("state=working:id=a").unwrap(), 30_000);
        records.apply(parse_body("state=error:id=a").unwrap(), 30_500);
        assert_eq!(records.worked_ms(&Some("a".to_owned())), Some(500));
        records.apply(parse_body("state=done:id=b").unwrap(), 31_000);
        assert_eq!(
            records.worked_ms(&Some("b".to_owned())),
            None,
            "a program that never said it was working has no known duration"
        );
        records.apply(parse_body("state=working").unwrap(), 40_000);
        assert_eq!(records.worked_ms(&None), None);
        records.clear_all();
        records.apply(parse_body("state=done").unwrap(), 50_000);
        assert_eq!(records.worked_ms(&None), None);
    }
}
