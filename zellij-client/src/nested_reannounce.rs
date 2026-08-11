use crate::os_input_output::ClientOsApi;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zellij_utils::nested_session::{
    self, reannounce_check_interval_ms, reannounce_silence_ms, NestedSessionCapability,
    NestedSessionMessage,
};

#[derive(Clone)]
pub struct NestedReannounce {
    last_heard_from_host: Arc<Mutex<Instant>>,
    stop: Arc<AtomicBool>,
}

impl NestedReannounce {
    pub fn spawn(os_input: Box<dyn ClientOsApi>, session_name: String) -> Self {
        let handle = NestedReannounce {
            last_heard_from_host: Arc::new(Mutex::new(Instant::now())),
            stop: Arc::new(AtomicBool::new(false)),
        };
        let last_heard_from_host = handle.last_heard_from_host.clone();
        let stop = handle.stop.clone();
        let _ = std::thread::Builder::new()
            .name("nested_reannounce".to_string())
            .spawn(move || {
                let check_interval = Duration::from_millis(reannounce_check_interval_ms());
                loop {
                    std::thread::sleep(check_interval);
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let silent_for = {
                        let last = last_heard_from_host.lock().unwrap();
                        last.elapsed()
                    };
                    if silent_for >= Duration::from_millis(reannounce_silence_ms()) {
                        let announce = NestedSessionMessage::Announce {
                            session_name: session_name.clone(),
                            capabilities: vec![NestedSessionCapability::NestedControl],
                        };
                        let mut stdout = os_input.get_stdout_writer();
                        if stdout
                            .write_all(&nested_session::encode_frame(&announce))
                            .is_ok()
                        {
                            let _ = stdout.flush();
                        }
                    }
                }
            });
        handle
    }

    pub fn note_host_contact(&self) {
        *self.last_heard_from_host.lock().unwrap() = Instant::now();
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
