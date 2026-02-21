use crate::os_input_output::AsyncReader;
use crate::panes::PaneId;

use std::{io, path::PathBuf};

use zellij_utils::{errors::prelude::*, input::command::RunCommand};

fn terminate_process(pid: u32) -> std::result::Result<(), std::io::Error> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let ok = TerminateProcess(handle, 1);
        CloseHandle(handle);
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

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

    pub fn kill(&self, pid: u32) -> Result<()> {
        terminate_process(pid)
            .with_context(|| format!("failed to kill pid {}", pid))?;
        Ok(())
    }

    pub fn force_kill(&self, pid: u32) -> Result<()> {
        terminate_process(pid)
            .with_context(|| format!("failed to force-kill pid {}", pid))?;
        Ok(())
    }

    pub fn send_sigint(&self, pid: u32) -> Result<()> {
        use windows_sys::Win32::System::Console::{GenerateConsoleCtrlEvent, CTRL_C_EVENT};

        let ok = unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, pid) };
        if ok != 0 {
            Ok(())
        } else {
            // Fallback: if GenerateConsoleCtrlEvent fails (e.g. different
            // process group), terminate the process instead.
            terminate_process(pid)
                .with_context(|| format!("failed to send SIGINT to pid {}", pid))?;
            Ok(())
        }
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
