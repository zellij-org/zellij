use crate::attention::SessionAttention;
use crate::input::actions::Action;
use crate::ipc::{
    ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub const PEER_REPLY_TIMEOUT: Duration = Duration::from_millis(700);
pub const PEER_ACTION_TIMEOUT: Duration = Duration::from_millis(2000);
pub const LOCAL_ANSWER_TIMEOUT: Duration = Duration::from_millis(500);
pub const REMOTE_ACTIONS_IDLE_EXIT: Duration = Duration::from_secs(30);

pub struct SessionLink {
    sender: IpcSenderWithContext<ClientToServerMsg>,
    receiver: IpcReceiverWithContext<ServerToClientMsg>,
}

impl SessionLink {
    pub fn open(session_name: &str) -> Result<Self, String> {
        Self::open_path(&crate::consts::session_socket_path(session_name))
    }
    pub fn open_path(socket_path: &Path) -> Result<Self, String> {
        let stream = crate::consts::ipc_connect(socket_path).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        let reply_stream =
            crate::consts::ipc_connect_reply(socket_path).map_err(|e| e.to_string())?;
        let sender = IpcSenderWithContext::<ClientToServerMsg>::new(stream);
        #[cfg(windows)]
        let receiver: IpcReceiverWithContext<ServerToClientMsg> =
            IpcReceiverWithContext::new(reply_stream);
        #[cfg(not(windows))]
        let receiver: IpcReceiverWithContext<ServerToClientMsg> = sender.get_receiver();
        Ok(SessionLink { sender, receiver })
    }
    pub fn send(&mut self, msg: ClientToServerMsg) -> Result<(), String> {
        self.sender.send_client_msg(msg).map_err(|e| e.to_string())
    }
    pub fn recv(&mut self, timeout: Duration) -> Result<ServerToClientMsg, String> {
        let _ = self.receiver.set_read_timeout(Some(timeout));
        self.receiver
            .try_recv_server_msg()
            .map(|(msg, _)| msg)
            .map_err(|e| e.to_string())
    }
}

pub fn request_session_attention(
    session_name: &str,
    timeout: Duration,
) -> Result<SessionAttention, String> {
    request_session_attention_with(session_name, true, timeout)
}

pub fn request_session_attention_with(
    session_name: &str,
    include_command_text: bool,
    timeout: Duration,
) -> Result<SessionAttention, String> {
    let mut link = SessionLink::open(session_name)?;
    link.send(ClientToServerMsg::RequestSessionAttention {
        include_command_text,
    })?;
    match link.recv(timeout)? {
        ServerToClientMsg::SessionAttention { attention } => Ok(attention),
        _ => Err("The session did not answer with its attention".to_owned()),
    }
}

pub fn push_session_attention(
    target_session: &str,
    own_session: &str,
    attention: Option<SessionAttention>,
    wait_for_reply: bool,
) -> Result<(), String> {
    push_session_attention_to(
        &crate::consts::session_socket_path(target_session),
        own_session,
        attention,
        wait_for_reply,
    )
}

pub fn push_session_attention_to(
    socket_path: &Path,
    own_session: &str,
    attention: Option<SessionAttention>,
    wait_for_reply: bool,
) -> Result<(), String> {
    let mut link = SessionLink::open_path(socket_path)?;
    link.send(ClientToServerMsg::PeerAttentionChanged {
        session_name: own_session.to_owned(),
        attention,
    })?;
    if wait_for_reply {
        let _ = link.recv(PEER_REPLY_TIMEOUT);
    }
    Ok(())
}

pub fn session_answers(session_name: &str, timeout: Duration) -> bool {
    let Ok(mut link) = SessionLink::open(session_name) else {
        return false;
    };
    if link.send(ClientToServerMsg::ConnStatus).is_err() {
        return false;
    }
    matches!(link.recv(timeout), Ok(ServerToClientMsg::Connected))
}

pub fn stop_session(session_name: &str, timeout: Duration) -> Result<(), String> {
    let mut link = SessionLink::open(session_name)?;
    link.send(ClientToServerMsg::KillSession)?;
    let _ = link.recv(timeout);
    Ok(())
}

pub fn detach_all_clients(session_name: &str) -> Result<(), String> {
    let mut link = SessionLink::open(session_name)?;
    link.send(ClientToServerMsg::DetachSession { client_ids: vec![] })
}

pub fn send_actions_to_session(session_name: &str, actions: Vec<Action>) -> Result<(), String> {
    let mut link = SessionLink::open(session_name)?;
    for action in actions {
        link.send(ClientToServerMsg::Action {
            action,
            terminal_id: None,
            client_id: None,
            is_cli_client: true,
        })?;
        loop {
            match link.recv(PEER_ACTION_TIMEOUT)? {
                ServerToClientMsg::UnblockInputThread => break,
                ServerToClientMsg::Exit { .. } | ServerToClientMsg::LogError { .. } => {
                    let _ = link.send(ClientToServerMsg::ClientExited);
                    return Err(format!("The session {} refused the request", session_name));
                },
                _ => {},
            }
        }
    }
    let _ = link.send(ClientToServerMsg::ClientExited);
    Ok(())
}

#[derive(Debug)]
struct RemoteActionQueue {
    sender: Sender<Vec<Action>>,
    generation: u64,
}

#[derive(Debug, Default)]
pub struct RemoteActions {
    queues: Arc<Mutex<HashMap<String, RemoteActionQueue>>>,
    next_generation: u64,
}

impl RemoteActions {
    pub fn send(&mut self, session_name: &str, actions: Vec<Action>) {
        let Ok(mut queues) = self.queues.lock() else {
            return;
        };
        let actions = match queues.get(session_name) {
            Some(queue) => match queue.sender.send(actions) {
                Ok(()) => return,
                Err(returned) => returned.0,
            },
            None => actions,
        };
        self.next_generation += 1;
        let generation = self.next_generation;
        let (sender, receiver) = channel();
        let name = session_name.to_owned();
        let worker_queues = self.queues.clone();
        let spawned = thread::Builder::new()
            .name(format!("remote_actions_{}", name))
            .spawn(move || {
                remote_action_worker(
                    name,
                    receiver,
                    worker_queues,
                    generation,
                    REMOTE_ACTIONS_IDLE_EXIT,
                )
            });
        if spawned.is_ok() && sender.send(actions).is_ok() {
            queues.insert(
                session_name.to_owned(),
                RemoteActionQueue { sender, generation },
            );
        } else {
            queues.remove(session_name);
        }
    }
    #[cfg(test)]
    fn queue_count(&self) -> usize {
        self.queues.lock().map(|q| q.len()).unwrap_or(0)
    }
}

fn leave_queue(
    queues: &Arc<Mutex<HashMap<String, RemoteActionQueue>>>,
    session_name: &str,
    generation: u64,
    receiver: &Receiver<Vec<Action>>,
) -> Option<Vec<Action>> {
    let mut queues = queues.lock().ok()?;
    if let Ok(late) = receiver.try_recv() {
        return Some(late);
    }
    if queues.get(session_name).map(|q| q.generation) == Some(generation) {
        queues.remove(session_name);
    }
    None
}

fn remote_action_worker(
    session_name: String,
    receiver: Receiver<Vec<Action>>,
    queues: Arc<Mutex<HashMap<String, RemoteActionQueue>>>,
    generation: u64,
    idle_exit: Duration,
) {
    loop {
        let mut batch = match receiver.recv_timeout(idle_exit) {
            Ok(batch) => batch,
            Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {
                match leave_queue(&queues, &session_name, generation, &receiver) {
                    Some(late) => late,
                    None => return,
                }
            },
        };
        while let Ok(more) = receiver.try_recv() {
            batch.extend(more);
        }
        if let Err(e) = send_actions_to_session(&session_name, batch) {
            log::error!("Failed to reach session {}: {}", session_name, e);
            if !session_socket_exists(&session_name) {
                let _ = leave_queue(&queues, &session_name, generation, &receiver);
                return;
            }
        }
    }
}

fn session_socket_exists(session_name: &str) -> bool {
    crate::consts::session_socket_path(session_name).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_session_does_not_answer() {
        assert!(!session_answers(
            "this-session-does-not-exist-anywhere",
            Duration::from_millis(50)
        ));
        assert!(request_session_attention(
            "this-session-does-not-exist-anywhere",
            Duration::from_millis(50)
        )
        .is_err());
    }

    #[test]
    fn remote_action_workers_leave_when_the_session_is_gone() {
        let mut remote_actions = RemoteActions::default();
        remote_actions.send(
            "this-session-does-not-exist-anywhere",
            vec![Action::GoToPreviousSession],
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while remote_actions.queue_count() > 0 && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(remote_actions.queue_count(), 0);
    }

    #[test]
    fn an_idle_remote_action_worker_leaves_its_queue() {
        let queues: Arc<Mutex<HashMap<String, RemoteActionQueue>>> = Default::default();
        let (sender, receiver) = channel();
        queues.lock().unwrap().insert(
            "api".to_owned(),
            RemoteActionQueue {
                sender,
                generation: 1,
            },
        );
        remote_action_worker(
            "api".to_owned(),
            receiver,
            queues.clone(),
            1,
            Duration::from_millis(10),
        );
        assert!(queues.lock().unwrap().is_empty());
    }

    #[test]
    fn a_newer_worker_keeps_its_queue_when_an_old_one_leaves() {
        let queues: Arc<Mutex<HashMap<String, RemoteActionQueue>>> = Default::default();
        let (old_sender, old_receiver) = channel::<Vec<Action>>();
        drop(old_sender);
        let (sender, _receiver) = channel();
        queues.lock().unwrap().insert(
            "api".to_owned(),
            RemoteActionQueue {
                sender,
                generation: 2,
            },
        );
        assert!(leave_queue(&queues, "api", 1, &old_receiver).is_none());
        assert_eq!(queues.lock().unwrap().len(), 1);
    }
}
