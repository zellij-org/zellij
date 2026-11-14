use crate::input::links::{shell_execute_args, SHELL_EXECUTE};

pub fn open(opener: &str, uri: &str) -> Option<Result<(), String>> {
    (opener == SHELL_EXECUTE).then(|| shell_execute(uri))
}

fn shell_execute(uri: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let args = shell_execute_args(uri)?;
    let instance = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            args.verb.as_ptr(),
            args.file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    let code = instance as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(format!(
            "the system refused to open the link (code {})",
            code
        ))
    }
}
