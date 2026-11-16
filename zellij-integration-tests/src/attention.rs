use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::time::Duration;
use zellij_utils::attention::AttentionCounts;

pub fn program_status(body: &str) -> Vec<u8> {
    format!("\u{1b}]7501;{}\u{1b}\\", body).into_bytes()
}

pub fn needs_you(text: &str) -> Vec<u8> {
    program_status(&format!(
        "state=blocked:kind=question:msg={}",
        STANDARD.encode(text)
    ))
}

pub fn encoded(text: &str) -> String {
    STANDARD.encode(text)
}

pub fn session_counts(session: &str) -> Option<AttentionCounts> {
    zellij_utils::session_link::request_session_attention(session, Duration::from_secs(2))
        .ok()
        .map(|attention| attention.counts)
}

pub fn counted(counts: &AttentionCounts) -> usize {
    counts.needs_you + counts.failed + counts.done + counts.working
}

pub fn wait_for_counted(session: &str, expected: usize) {
    crate::wait_for(
        &format!("{} to count {} item(s)", session, expected),
        || session_counts(session).map(|counts| counted(&counts)) == Some(expected),
    );
}
