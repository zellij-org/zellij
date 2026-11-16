pub use zellij_utils::session_link::*;

pub fn local_session_attention(
    senders: &crate::thread_bus::ThreadSenders,
    include_command_text: bool,
) -> zellij_utils::attention::SessionAttention {
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);
    let sent = senders.send_to_screen(crate::screen::ScreenInstruction::GetSessionAttention {
        include_command_text,
        response_channel: response_sender,
    });
    if sent.is_err() {
        return zellij_utils::attention::SessionAttention::default();
    }
    response_receiver
        .recv_timeout(zellij_utils::session_link::LOCAL_ANSWER_TIMEOUT)
        .unwrap_or_default()
}
