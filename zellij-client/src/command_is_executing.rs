use parking_lot::{Condvar, Mutex};
use std::sync::Arc;

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
        let mut input_thread = lock.lock();
        *input_thread = true;
    }
    pub fn unblock_input_thread(&mut self) {
        let (lock, cvar) = &*self.input_thread;
        let mut input_thread = lock.lock();
        *input_thread = false;
        cvar.notify_all();
    }
    pub fn wait_until_input_thread_is_unblocked(&self) {
        let (lock, cvar) = &*self.input_thread;
        let mut input_thread = lock.lock();
        while *input_thread {
            cvar.wait(&mut input_thread);
        }
    }
}
