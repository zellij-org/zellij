
pub struct Layout {
    pub spinner_frame: usize,
    pub spinner_timer_scheduled: bool,
}

impl Layout {
    pub fn new() -> Self {
        Self {
            spinner_frame: 0,
            spinner_timer_scheduled: false,
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}
