
pub struct Layout {
    pub cached_cursor_position: Option<(usize, usize)>,
    pub spinner_frame: usize,
    pub spinner_timer_scheduled: bool,
}

impl Layout {
    pub fn new() -> Self {
        Self {
            cached_cursor_position: None,
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
