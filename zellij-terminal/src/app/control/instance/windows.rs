use std::path::Path;

use crate::app::control::instance::Handler;

pub struct Guard;

impl Guard {
    pub fn new(_control: &Path) -> Self {
        Guard
    }

    pub fn start(&self, _control: &Path, _handle: Handler) -> std::io::Result<()> {
        Ok(())
    }

    pub fn release(&self, control: &Path) {
        let _ = std::fs::remove_file(control);
    }
}
