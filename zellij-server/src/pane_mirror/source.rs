use crate::screen::ScreenInstruction;
use crate::thread_bus::ThreadSenders;
use crate::SessionMetaData;
use crossbeam::channel::{unbounded, Receiver, Sender};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use zellij_utils::data::MirrorClosedReason;
use zellij_utils::ipc::{
    ClientToServerMsg, IpcReceiveError, IpcReceiverWithContext, IpcSenderWithContext,
    ServerToClientMsg,
};
use zellij_utils::pane_mirror::{
    clamp_mirror_size, MirrorAttach, SourceMessage, ViewerMessage, PANE_MIRROR_VERSION,
};

const SESSION_READY_WAIT: Duration = Duration::from_secs(10);
const SESSION_READY_POLL_INTERVAL: Duration = Duration::from_millis(5);
const MAX_CONSECUTIVE_UNDECODABLE: usize = 1000;

static NEXT_MIRROR_SUBSCRIBER_ID: AtomicU64 = AtomicU64::new(1);

pub type MirrorSubscriberId = u64;

#[derive(Debug, Clone)]
pub enum MirrorOutgoing {
    Message(SourceMessage),
    Finish,
}

#[derive(Debug, Clone)]
pub struct MirrorOutbox {
    sender: Sender<MirrorOutgoing>,
}

impl MirrorOutbox {
    pub fn new(sender: Sender<MirrorOutgoing>) -> Self {
        MirrorOutbox { sender }
    }
    pub fn send(&self, message: SourceMessage) -> bool {
        self.sender.send(MirrorOutgoing::Message(message)).is_ok()
    }
    pub fn close(&self, reason: MirrorClosedReason) {
        let _ = self
            .sender
            .send(MirrorOutgoing::Message(SourceMessage::closed(reason)));
    }
    pub fn finish(&self) {
        let _ = self.sender.send(MirrorOutgoing::Finish);
    }
}

fn reply_now(sender: &mut IpcSenderWithContext<ServerToClientMsg>, message: SourceMessage) {
    let _ = sender.send_server_msg(ServerToClientMsg::PaneMirror {
        payload: message.encode(),
    });
}

fn senders_when_ready(
    session_data: &Arc<RwLock<Option<SessionMetaData>>>,
) -> Option<ThreadSenders> {
    let deadline = Instant::now() + SESSION_READY_WAIT;
    loop {
        let senders = session_data
            .read()
            .ok()
            .and_then(|session_data| session_data.as_ref().map(|s| s.senders.clone()));
        if senders.is_some() || Instant::now() >= deadline {
            return senders;
        }
        std::thread::sleep(SESSION_READY_POLL_INTERVAL);
    }
}

pub(crate) fn serve_mirror_connection(
    session_data: Arc<RwLock<Option<SessionMetaData>>>,
    mut receiver: IpcReceiverWithContext<ClientToServerMsg>,
    #[cfg(windows)] reply_stream: interprocess::local_socket::Stream,
    first_payload: Vec<u8>,
) {
    #[cfg(windows)]
    let mut sender = IpcSenderWithContext::<ServerToClientMsg>::new(reply_stream);
    #[cfg(not(windows))]
    let mut sender = receiver.get_sender::<ServerToClientMsg>();
    let attach = match ViewerMessage::decode(&first_payload) {
        Ok(ViewerMessage::Attach(attach)) => MirrorAttach {
            viewer_size: clamp_mirror_size(attach.viewer_size),
            ..attach
        },
        _ => {
            reply_now(
                &mut sender,
                SourceMessage::closed(MirrorClosedReason::Rejected),
            );
            return;
        },
    };
    if attach.version != PANE_MIRROR_VERSION {
        reply_now(
            &mut sender,
            SourceMessage::closed(MirrorClosedReason::VersionMismatch),
        );
        return;
    }
    if matches!(attach.target_pane_id, zellij_utils::data::PaneId::Plugin(_)) {
        reply_now(
            &mut sender,
            SourceMessage::closed(MirrorClosedReason::Rejected),
        );
        return;
    }
    let Some(senders) = senders_when_ready(&session_data) else {
        reply_now(
            &mut sender,
            SourceMessage::closed(MirrorClosedReason::SessionEnding),
        );
        return;
    };
    let subscriber_id = NEXT_MIRROR_SUBSCRIBER_ID.fetch_add(1, Ordering::Relaxed);
    let (outbox_sender, outgoing) = unbounded();
    let writer = std::thread::Builder::new()
        .name(format!("pane_mirror_writer_{}", subscriber_id))
        .spawn(move || write_source_messages(sender, outgoing));
    if writer.is_err() {
        return;
    }
    let outbox = MirrorOutbox::new(outbox_sender);
    if senders
        .send_to_screen(ScreenInstruction::MirrorAttach {
            subscriber_id,
            attach,
            outbox: outbox.clone(),
        })
        .is_err()
    {
        outbox.close(MirrorClosedReason::SessionEnding);
        return;
    }
    let mut undecodable = 0;
    loop {
        match receiver.try_recv_client_msg() {
            Ok((ClientToServerMsg::PaneMirror { payload }, _)) => {
                undecodable = 0;
                let instruction = match ViewerMessage::decode(&payload) {
                    Ok(ViewerMessage::Input(input)) => ScreenInstruction::MirrorInput {
                        subscriber_id,
                        input,
                    },
                    Ok(ViewerMessage::Resize {
                        size,
                        focused,
                        resize_allowed,
                    }) => ScreenInstruction::MirrorResize {
                        subscriber_id,
                        size: clamp_mirror_size(size),
                        focused,
                        resize_allowed,
                    },
                    Ok(ViewerMessage::FrameAck { seq }) => {
                        ScreenInstruction::MirrorFrameAck { subscriber_id, seq }
                    },
                    Ok(ViewerMessage::Park) => ScreenInstruction::MirrorPark { subscriber_id },
                    Ok(ViewerMessage::Unpark { resize_allowed }) => {
                        ScreenInstruction::MirrorUnpark {
                            subscriber_id,
                            resize_allowed,
                        }
                    },
                    Ok(ViewerMessage::Detach) => break,
                    Ok(ViewerMessage::Attach(_)) => continue,
                    Err(e) => {
                        log::warn!("Dropping a mirror message: {}", e);
                        continue;
                    },
                };
                if senders.send_to_screen(instruction).is_err() {
                    break;
                }
            },
            Ok(_) => {},
            Err(IpcReceiveError::Undecodable) => {
                undecodable += 1;
                if undecodable >= MAX_CONSECUTIVE_UNDECODABLE {
                    break;
                }
            },
            Err(IpcReceiveError::Disconnected) => break,
        }
    }
    let _ = senders.send_to_screen(ScreenInstruction::MirrorDetach { subscriber_id });
}

fn write_source_messages(
    mut sender: IpcSenderWithContext<ServerToClientMsg>,
    outgoing: Receiver<MirrorOutgoing>,
) {
    loop {
        match outgoing.recv() {
            Ok(MirrorOutgoing::Message(message)) => {
                let closed = matches!(message, SourceMessage::Closed { .. });
                let sent = sender.send_server_msg(ServerToClientMsg::PaneMirror {
                    payload: message.encode(),
                });
                if sent.is_err() || closed {
                    return;
                }
            },
            Ok(MirrorOutgoing::Finish) => return,
            Err(_) => {
                reply_now(
                    &mut sender,
                    SourceMessage::closed(MirrorClosedReason::SessionEnding),
                );
                return;
            },
        }
    }
}
