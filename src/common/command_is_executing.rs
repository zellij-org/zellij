#![allow(clippy::mutex_atomic)]
use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone)]
pub struct CommandIsExecuting {
    opening_new_pane: Arc<(Mutex<bool>, Condvar)>,
    closing_pane: Arc<(Mutex<bool>, Condvar)>,
    updating_tabs: Arc<(Mutex<bool>, Condvar)>,
}

impl CommandIsExecuting {
    pub fn new() -> Self {
        CommandIsExecuting {
            opening_new_pane: Arc::new((Mutex::new(false), Condvar::new())),
            closing_pane: Arc::new((Mutex::new(false), Condvar::new())),
            updating_tabs: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }
    pub fn closing_pane(&mut self) {
        let (lock, _cvar) = &*self.closing_pane;
        let mut closing_pane = lock.lock().unwrap();
        *closing_pane = true;
    }
    pub fn done_closing_pane(&mut self) {
        let (lock, cvar) = &*self.closing_pane;
        let mut closing_pane = lock.lock().unwrap();
        *closing_pane = false;
        cvar.notify_all();
    }
    pub fn opening_new_pane(&mut self) {
        let (lock, _cvar) = &*self.opening_new_pane;
        let mut opening_new_pane = lock.lock().unwrap();
        *opening_new_pane = true;
    }
    pub fn done_opening_new_pane(&mut self) {
        let (lock, cvar) = &*self.opening_new_pane;
        let mut opening_new_pane = lock.lock().unwrap();
        *opening_new_pane = false;
        cvar.notify_all();
    }
    pub fn updating_tabs(&mut self) {
        let (lock, _cvar) = &*self.updating_tabs;
        let mut updating_tabs = lock.lock().unwrap();
        *updating_tabs = true;
    }
    pub fn done_updating_tabs(&self) {
        let (lock, cvar) = &*self.updating_tabs;
        let mut updating_tabs = lock.lock().unwrap();
        *updating_tabs = false;
        cvar.notify_one();
    }
    pub fn wait_until_pane_is_closed(&self) {
        let (lock, cvar) = &*self.closing_pane;
        let mut closing_pane = lock.lock().unwrap();
        while *closing_pane {
            closing_pane = cvar.wait(closing_pane).unwrap();
        }
    }
    pub fn wait_until_new_pane_is_opened(&self) {
        let (lock, cvar) = &*self.opening_new_pane;
        let mut opening_new_pane = lock.lock().unwrap();
        while *opening_new_pane {
            opening_new_pane = cvar.wait(opening_new_pane).unwrap();
        }
    }
    pub fn wait_until_tabs_are_updated(&self) {
        let (lock, cvar) = &*self.updating_tabs;
        let mut updating_tabs = lock.lock().unwrap();
        while *updating_tabs {
            updating_tabs = cvar.wait(updating_tabs).unwrap();
        }
    }
}
