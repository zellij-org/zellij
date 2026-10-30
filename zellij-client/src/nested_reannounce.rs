use crate::os_input_output::ClientOsApi;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use zellij_utils::nested_session::{
    self, reannounce_check_interval_ms, NestedSessionCapability, NestedSessionMessage,
    ReannounceScheduler,
};

#[derive(Clone)]
pub struct NestedReannounce {
    scheduler: Arc<Mutex<ReannounceScheduler>>,
    wakeup: Arc<Condvar>,
    stop: Arc<AtomicBool>,
    thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl NestedReannounce {
    pub fn spawn(os_input: Box<dyn ClientOsApi>, session_name: String) -> Self {
        let handle = NestedReannounce {
            scheduler: Arc::new(Mutex::new(ReannounceScheduler::new(Instant::now()))),
            wakeup: Arc::new(Condvar::new()),
            stop: Arc::new(AtomicBool::new(false)),
            thread: Arc::new(Mutex::new(None)),
        };
        let scheduler = handle.scheduler.clone();
        let wakeup = handle.wakeup.clone();
        let stop = handle.stop.clone();
        let thread = std::thread::Builder::new()
            .name("nested_reannounce".to_string())
            .spawn(move || {
                let check_interval = Duration::from_millis(reannounce_check_interval_ms());
                loop {
                    let mut current = scheduler.lock().unwrap();
                    while !stop.load(Ordering::Relaxed) && current.budget_exhausted() {
                        current = wakeup.wait(current).unwrap();
                    }
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let (mut current, _) = wakeup.wait_timeout(current, check_interval).unwrap();
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let should_announce = current.on_tick(Instant::now());
                    drop(current);
                    if !should_announce {
                        continue;
                    }
                    let announce = NestedSessionMessage::Announce {
                        session_name: session_name.clone(),
                        capabilities: vec![
                            NestedSessionCapability::NestedControl,
                            NestedSessionCapability::HintReporting,
                        ],
                    };
                    let mut stdout = os_input.get_stdout_writer();
                    if stdout
                        .write_all(&nested_session::encode_frame(&announce))
                        .is_ok()
                    {
                        let _ = stdout.flush();
                    }
                }
            });
        *handle.thread.lock().unwrap() = thread.ok();
        handle
    }

    pub fn note_host_contact(&self) {
        let first_contact = self
            .scheduler
            .lock()
            .unwrap()
            .note_host_contact(Instant::now());
        if first_contact {
            self.wakeup.notify_all();
        }
    }

    pub fn host_contacted(&self) -> bool {
        self.scheduler.lock().unwrap().host_contacted()
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.wakeup.notify_all();
        let thread = self.thread.lock().unwrap().take();
        if let Some(thread) = thread {
            let _ = thread.join();
        }
    }
}
