#![allow(clippy::mutex_atomic)]
use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone)]
pub(crate) struct CommandIsExecuting {
    input_thread: Arc<(Mutex<bool>, Condvar)>,
}

impl CommandIsExecuting {
    pub fn new() -> Self {
        CommandIsExecuting {
            input_thread: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }
    pub fn blocking_input_thread(&mut self) {
        let (lock, _cvar) = &*self.input_thread;
        let mut input_thread = lock.lock().unwrap();
        *input_thread = true;
    }
    pub fn unblock_input_thread(&mut self) {
        let (lock, cvar) = &*self.input_thread;
        let mut input_thread = lock.lock().unwrap();
        *input_thread = false;
        cvar.notify_all();
    }
    pub fn wait_until_input_thread_is_unblocked(&self) {
        let (lock, cvar) = &*self.input_thread;
        let mut input_thread = lock.lock().unwrap();
        while *input_thread {
            input_thread = cvar.wait(input_thread).unwrap();
        }
    }
}
