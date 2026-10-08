use std::path::PathBuf;

macro_rules! report {
    ($($arg:tt)*) => {
        $crate::diagnostics::emit(&format!($($arg)*))
    };
}

pub fn emit(message: &str) {
    eprintln!("zellij-window: {}", message);
    log::warn!("{}", message);
}

pub fn log_file() -> PathBuf {
    zellij_utils::consts::ZELLIJ_TMP_LOG_FILE.clone()
}

pub fn log_crashes() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        log::error!(
            "{}",
            crash_report(&info.to_string(), &backtrace.to_string())
        );
        previous(info);
    }));
}

pub fn crash_report(panic: &str, backtrace: &str) -> String {
    format!("zellij-window crashed: {}\n{}", panic, backtrace)
}

pub fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        return (*text).to_owned();
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    "an unknown error".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_report_names_the_crash_and_carries_the_backtrace() {
        let report = crash_report("index out of bounds", "0: frame one\n1: frame two");
        assert!(report.starts_with("zellij-window crashed: index out of bounds"));
        assert!(report.contains("1: frame two"));
    }

    #[test]
    fn the_text_of_a_panic_is_recovered_whatever_its_payload() {
        let borrowed = std::panic::catch_unwind(|| panic!("borrowed")).unwrap_err();
        assert_eq!(panic_text(borrowed.as_ref()), "borrowed");
        let owned =
            std::panic::catch_unwind(|| std::panic::panic_any(String::from("owned"))).unwrap_err();
        assert_eq!(panic_text(owned.as_ref()), "owned");
        let other = std::panic::catch_unwind(|| std::panic::panic_any(7_u8)).unwrap_err();
        assert_eq!(panic_text(other.as_ref()), "an unknown error");
    }
}
