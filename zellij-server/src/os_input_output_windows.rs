use crate::os_input_output::AsyncReader;
use crate::panes::PaneId;

use std::{io, path::PathBuf};

use zellij_utils::{errors::prelude::*, input::command::RunCommand};

/// Windows PTY backend stub. Not yet implemented.
#[derive(Clone)]
pub(crate) struct WindowsPtyBackend;

impl WindowsPtyBackend {
    pub fn new() -> Result<Self, io::Error> {
        Ok(Self)
    }

    pub fn spawn_terminal(
        &self,
        _cmd: RunCommand,
        _failover_cmd: Option<RunCommand>,
        _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        _terminal_id: u32,
    ) -> Result<(Box<dyn AsyncReader>, u32)> {
        unimplemented!("Windows PTY not yet implemented")
    }

    pub fn set_terminal_size(
        &self,
        _terminal_id: u32,
        _cols: u16,
        _rows: u16,
        _width_in_pixels: Option<u16>,
        _height_in_pixels: Option<u16>,
    ) -> Result<()> {
        unimplemented!("Windows PTY not yet implemented")
    }

    pub fn write_to_tty_stdin(&self, _terminal_id: u32, _buf: &[u8]) -> Result<usize> {
        unimplemented!("Windows PTY not yet implemented")
    }

    pub fn tcdrain(&self, _terminal_id: u32) -> Result<()> {
        unimplemented!("Windows PTY not yet implemented")
    }

    pub fn kill(&self, _pid: u32) -> Result<()> {
        unimplemented!("Windows signals not yet implemented")
    }

    pub fn force_kill(&self, _pid: u32) -> Result<()> {
        unimplemented!("Windows signals not yet implemented")
    }

    pub fn send_sigint(&self, _pid: u32) -> Result<()> {
        unimplemented!("Windows signals not yet implemented")
    }

    pub fn reserve_terminal_id(&self, _terminal_id: u32) {
        unimplemented!("Windows PTY not yet implemented")
    }

    pub fn clear_terminal_id(&self, _terminal_id: u32) {
        unimplemented!("Windows PTY not yet implemented")
    }

    pub fn next_terminal_id(&self) -> Option<u32> {
        unimplemented!("Windows PTY not yet implemented")
    }
}
