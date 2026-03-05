pub mod terminfo;
#[cfg(windows)]
pub mod windows;

pub trait RenderTty: std::io::Write {
    /// Returns the (cols, rows) for the terminal
    fn get_size_in_cells(&mut self) -> crate::vendored::termwiz::Result<(usize, usize)>;
}
