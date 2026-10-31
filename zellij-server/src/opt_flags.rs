use std::sync::atomic::{AtomicU32, Ordering};

pub const BULK_TEXT: u32 = 1 << 0;
pub const PARSER_SCAN: u32 = 1 << 1;
pub const STYLE_MEMO: u32 = 1 << 2;
pub const LAZY_LINKS: u32 = 1 << 3;
pub const PTY_FAST_PATH: u32 = 1 << 4;
pub const BIG_READS: u32 = 1 << 5;
pub const ACK_WAIT: u32 = 1 << 6;
pub const ROW_SHIFTS: u32 = 1 << 7;
pub const ROW_RECYCLE: u32 = 1 << 8;
pub const FRAME_STYLE_RUNS: u32 = 1 << 9;

pub const NAMES: [(&str, u32); 10] = [
    ("bulk_text", BULK_TEXT),
    ("parser_scan", PARSER_SCAN),
    ("style_memo", STYLE_MEMO),
    ("lazy_links", LAZY_LINKS),
    ("pty_fast_path", PTY_FAST_PATH),
    ("big_reads", BIG_READS),
    ("ack_wait", ACK_WAIT),
    ("row_shifts", ROW_SHIFTS),
    ("row_recycle", ROW_RECYCLE),
    ("frame_style_runs", FRAME_STYLE_RUNS),
];

const UNSET: u32 = u32::MAX;
static ENABLED: AtomicU32 = AtomicU32::new(UNSET);

pub fn parse(disabled: &str) -> u32 {
    let mut enabled = NAMES.iter().fold(0, |mask, (_, bit)| mask | bit);
    for name in disabled.split(',').map(str::trim) {
        if name == "all" {
            enabled = 0;
        } else if let Some((_, bit)) = NAMES.iter().find(|(known, _)| *known == name) {
            enabled &= !bit;
        } else if !name.is_empty() {
            log::warn!("ZELLIJ_OPT_OFF: unknown group {:?}", name);
        }
    }
    enabled
}

#[cold]
fn load() -> u32 {
    let enabled = parse(&std::env::var("ZELLIJ_OPT_OFF").unwrap_or_default());
    ENABLED.store(enabled, Ordering::Relaxed);
    enabled
}

#[inline]
pub fn on(flag: u32) -> bool {
    let enabled = match ENABLED.load(Ordering::Relaxed) {
        UNSET => load(),
        enabled => enabled,
    };
    enabled & flag != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_are_turned_off_by_name_and_all_turns_off_every_group() {
        let every = NAMES.iter().fold(0, |mask, (_, bit)| mask | bit);
        assert_eq!(parse(""), every);
        assert_eq!(
            parse("bulk_text, row_shifts"),
            every & !BULK_TEXT & !ROW_SHIFTS
        );
        assert_eq!(parse("all"), 0);
        assert_eq!(parse("no_such_group"), every);
    }
}
