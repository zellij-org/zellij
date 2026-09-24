#[cfg(not(test))]
use std::process::{Command, Stdio};

use winit::keyboard::ModifiersState;

use crate::screen_buffer::ScreenBuffer;

const ALLOWED_SCHEMES: [&str; 6] = ["http", "https", "ftp", "ftps", "mailto", "file"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRun {
    pub id: u8,
    pub uri: String,
    pub spans: Vec<(usize, usize, usize)>,
}

impl LinkRun {
    pub fn covers(&self, row: usize, col: usize) -> bool {
        self.spans
            .iter()
            .any(|(at, start, end)| *at == row && col >= *start && col <= *end)
    }
}

pub fn run_at(buffer: &ScreenBuffer, row: usize, col: usize) -> Option<LinkRun> {
    let size = buffer.size();
    if row >= size.rows || col >= size.cols {
        return None;
    }
    let id = buffer.link_id_at(row, col);
    let uri = buffer.link_at(row, col)?.to_owned();

    let same = |row: usize, col: usize| buffer.link_id_at(row, col) == id;
    let row_start = |row: usize, from: usize| {
        let mut start = from;
        while start > 0 && same(row, start - 1) {
            start -= 1;
        }
        start
    };
    let row_end = |row: usize, from: usize| {
        let mut end = from;
        while end + 1 < size.cols && same(row, end + 1) {
            end += 1;
        }
        end
    };

    let mut spans = vec![(row, row_start(row, col), row_end(row, col))];
    let mut first = row;
    while first > 0 && spans[0].1 == 0 && same(first - 1, size.cols - 1) {
        first -= 1;
        spans.insert(0, (first, row_start(first, size.cols - 1), size.cols - 1));
    }
    let mut last = row;
    while last + 1 < size.rows && spans[spans.len() - 1].2 == size.cols - 1 && same(last + 1, 0) {
        last += 1;
        spans.push((last, 0, row_end(last, 0)));
    }

    Some(LinkRun { id, uri, spans })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Armed {
    pub uri: String,
    pub cell: (u16, u16),
}

pub fn armed_by_press(
    enabled: bool,
    pane_wants_mouse: bool,
    modifiers: ModifiersState,
    cell: Option<(u16, u16)>,
    hovered: Option<&LinkRun>,
) -> Option<Armed> {
    if !enabled || !gesture_matches(pane_wants_mouse, modifiers) {
        return None;
    }
    let (x, y) = cell?;
    let run = hovered?;
    if !run.covers(y as usize, x as usize) || !is_openable(&run.uri) {
        return None;
    }
    Some(Armed {
        uri: run.uri.clone(),
        cell: (x, y),
    })
}

pub fn released_on(armed: Option<Armed>, cell: Option<(u16, u16)>) -> Option<String> {
    let armed = armed?;
    (Some(armed.cell) == cell).then_some(armed.uri)
}

pub fn gesture_matches(pane_wants_mouse: bool, modifiers: ModifiersState) -> bool {
    if modifiers.control_key() || modifiers.alt_key() || modifiers.super_key() {
        return false;
    }
    modifiers.shift_key() == pane_wants_mouse
}

pub fn is_reachable(
    enabled: bool,
    pane_wants_mouse: bool,
    modifiers: ModifiersState,
    hovered: Option<&LinkRun>,
) -> bool {
    enabled
        && gesture_matches(pane_wants_mouse, modifiers)
        && hovered.is_some_and(|run| is_openable(&run.uri))
}

pub fn scheme_of(uri: &str) -> Option<&str> {
    let (scheme, _) = uri.split_once(':')?;
    if scheme.is_empty() {
        return None;
    }
    Some(scheme)
}

pub fn is_openable(uri: &str) -> bool {
    if uri.is_empty() || uri.len() > 2048 {
        return false;
    }
    if uri.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    let Some(scheme) = scheme_of(uri) else {
        return false;
    };
    if !scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return false;
    }
    ALLOWED_SCHEMES
        .iter()
        .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
}

pub fn open(uri: &str) -> Result<(), String> {
    if !is_openable(uri) {
        return Err(format!("refusing to open {:?}", uri));
    }
    let Some(opener) = opener() else {
        return Err("this platform has no known way to open a link".to_owned());
    };
    launch(opener, uri)
}

#[cfg(not(test))]
fn launch(opener: &str, uri: &str) -> Result<(), String> {
    #[cfg(windows)]
    if opener == SHELL_EXECUTE {
        return shell_execute(uri);
    }
    let child = Command::new(opener)
        .arg(uri)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not run {}: {}", opener, e))?;
    reap(child);
    Ok(())
}

#[cfg(not(test))]
fn reap(mut child: std::process::Child) {
    std::thread::Builder::new()
        .name("zellij-window-open".to_owned())
        .spawn(move || {
            let _ = child.wait();
        })
        .ok();
}

#[cfg(test)]
thread_local! {
    static OPENED: std::cell::RefCell<Vec<(String, String)>> =
        std::cell::RefCell::new(Vec::new());
}

#[cfg(test)]
fn launch(opener: &str, uri: &str) -> Result<(), String> {
    OPENED.with(|opened| {
        opened
            .borrow_mut()
            .push((opener.to_owned(), uri.to_owned()))
    });
    Ok(())
}

#[cfg(test)]
pub fn opened() -> Vec<(String, String)> {
    OPENED.with(|opened| opened.borrow().clone())
}

#[cfg(test)]
pub fn forget_opened() {
    OPENED.with(|opened| opened.borrow_mut().clear());
}

#[cfg(target_os = "linux")]
pub fn opener() -> Option<&'static str> {
    Some("xdg-open")
}

#[cfg(target_os = "macos")]
pub fn opener() -> Option<&'static str> {
    Some("open")
}

#[cfg(windows)]
pub fn opener() -> Option<&'static str> {
    Some(SHELL_EXECUTE)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub fn opener() -> Option<&'static str> {
    None
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
const SHELL_EXECUTE: &str = "ShellExecuteW";

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub struct ShellExecuteArgs {
    pub verb: Vec<u16>,
    pub file: Vec<u16>,
}

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub fn shell_execute_args(uri: &str) -> Result<ShellExecuteArgs, String> {
    if uri.contains('\0') {
        return Err(format!("refusing to open {:?}: it contains a NUL", uri));
    }
    let wide = |text: &str| text.encode_utf16().chain(std::iter::once(0)).collect();
    Ok(ShellExecuteArgs {
        verb: wide("open"),
        file: wide(uri),
    })
}

#[cfg(all(windows, not(test)))]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen_buffer::painter;

    fn buffer(rows: usize, cols: usize, frames: &[Vec<u8>]) -> ScreenBuffer {
        let mut buffer = ScreenBuffer::new(rows, cols);
        for frame in frames {
            let view = zellij_utils::structured_render::decode(frame).expect("a decodable frame");
            buffer.apply(&view).expect("an applicable frame");
        }
        buffer
    }

    #[test]
    fn the_run_under_the_pointer_is_the_contiguous_span_of_one_id() {
        let frame = painter::linked_frame(
            1,
            8,
            0,
            true,
            &[(0, 0, "ab  cdef", &[1, 1, 0, 0, 2, 2, 2, 2])],
            &[
                (1, "https://example.com/one"),
                (2, "https://example.com/two"),
            ],
        );
        let buffer = buffer(1, 8, &[frame]);
        let run = run_at(&buffer, 0, 5).expect("a link under the pointer");
        assert_eq!(run.uri, "https://example.com/two");
        assert_eq!(run.spans, vec![(0, 4, 7)]);
        assert!(run.covers(0, 4));
        assert!(!run.covers(0, 3));
        assert_eq!(
            run_at(&buffer, 0, 1).expect("the first run").spans,
            vec![(0, 0, 1)]
        );
    }

    #[test]
    fn a_cell_outside_any_link_resolves_to_nothing() {
        let frame = painter::linked_frame(
            1,
            8,
            0,
            true,
            &[(0, 0, "ab  cdef", &[1, 1, 0, 0, 0, 0, 0, 0])],
            &[(1, "https://example.com/one")],
        );
        let buffer = buffer(1, 8, &[frame]);
        assert!(run_at(&buffer, 0, 5).is_none());
        assert!(run_at(&buffer, 9, 9).is_none());
    }

    #[test]
    fn a_run_that_wrapped_at_the_row_end_is_followed_onto_the_next_row() {
        let frame = painter::linked_frame(
            2,
            4,
            0,
            true,
            &[(0, 0, "  ab", &[0, 0, 1, 1]), (1, 0, "cd  ", &[1, 1, 0, 0])],
            &[(1, "https://example.com/wrapped")],
        );
        let buffer = buffer(2, 4, &[frame]);
        let run = run_at(&buffer, 0, 3).expect("a link under the pointer");
        assert_eq!(run.spans, vec![(0, 2, 3), (1, 0, 1)]);
        let from_below = run_at(&buffer, 1, 0).expect("the same run from the other row");
        assert_eq!(from_below.spans, run.spans);
    }

    #[test]
    fn a_link_id_that_the_table_does_not_name_resolves_to_nothing() {
        let frame = painter::linked_frame(1, 4, 0, true, &[(0, 0, "abcd", &[7, 7, 0, 0])], &[]);
        let buffer = buffer(1, 4, &[frame]);
        assert!(run_at(&buffer, 0, 0).is_none());
    }

    #[test]
    fn the_allow_list_accepts_the_schemes_a_terminal_link_really_uses() {
        for uri in [
            "http://example.com",
            "https://example.com/a?b=c#d",
            "HTTPS://EXAMPLE.COM",
            "ftp://example.com/x",
            "ftps://example.com/x",
            "mailto:someone@example.com",
            "file:///tmp/notes.txt",
        ] {
            assert!(is_openable(uri), "{} should be openable", uri);
        }
    }

    #[test]
    fn the_allow_list_refuses_everything_else() {
        for uri in [
            "javascript:alert(1)",
            "data:text/html,<script>",
            "vbscript:msgbox",
            "about:blank",
            "chrome://settings",
            "ssh://host",
            "",
            "example.com",
            ":no-scheme",
            "https://example.com/a b",
            "https://example.com/\u{1b}]8;;\u{1b}\\",
            "https://example.com/\n",
        ] {
            assert!(!is_openable(uri), "{:?} should be refused", uri);
        }
    }

    #[test]
    fn a_url_longer_than_the_wire_carries_is_refused() {
        let long = format!("https://example.com/{}", "a".repeat(4096));
        assert!(!is_openable(&long));
    }

    #[test]
    fn opening_a_refused_scheme_never_reaches_the_opener() {
        forget_opened();
        let refused = open("javascript:alert(1)").expect_err("a refusal");
        assert!(refused.contains("refusing to open"));
        assert!(opened().is_empty());
    }

    #[test]
    fn an_allowed_url_reaches_the_platform_opener_as_one_argument() {
        forget_opened();
        open("https://example.com/a?b=c").expect("an allowed url opens");
        assert_eq!(
            opened(),
            vec![(
                opener().expect("this platform has an opener").to_owned(),
                "https://example.com/a?b=c".to_owned()
            )],
            "the url travels as a single argument to the platform's opener, never through a shell"
        );
    }

    fn run(uri: &str) -> LinkRun {
        LinkRun {
            id: 1,
            uri: uri.to_owned(),
            spans: vec![(0, 0, 3)],
        }
    }

    const NONE: ModifiersState = ModifiersState::empty();

    fn armed(wants: bool, modifiers: ModifiersState, uri: &str) -> Option<Armed> {
        armed_by_press(true, wants, modifiers, Some((1, 0)), Some(&run(uri)))
    }

    #[test]
    fn a_plain_click_opens_a_link_in_a_pane_that_is_not_watching_the_mouse() {
        let armed = armed(false, NONE, "https://example.com").expect("the press arms the open");
        assert_eq!(armed.cell, (1, 0));
        assert_eq!(
            released_on(Some(armed), Some((1, 0))),
            Some("https://example.com".to_owned())
        );
    }

    #[test]
    fn a_plain_click_is_not_enough_once_the_pane_is_watching_the_mouse() {
        assert_eq!(armed(true, NONE, "https://example.com"), None);
    }

    #[test]
    fn shift_opens_a_link_in_a_pane_that_is_watching_the_mouse() {
        assert!(armed(true, ModifiersState::SHIFT, "https://example.com").is_some());
    }

    #[test]
    fn shift_over_a_pane_that_is_not_watching_the_mouse_is_a_selection_not_an_open() {
        assert_eq!(
            armed(false, ModifiersState::SHIFT, "https://example.com"),
            None
        );
    }

    #[test]
    fn no_other_modifier_opens_a_link() {
        for modifiers in [
            ModifiersState::CONTROL,
            ModifiersState::ALT,
            ModifiersState::SUPER,
            ModifiersState::CONTROL | ModifiersState::SHIFT,
        ] {
            assert_eq!(armed(false, modifiers, "https://example.com"), None);
            assert_eq!(armed(true, modifiers, "https://example.com"), None);
        }
    }

    #[test]
    fn a_drag_off_the_link_is_a_selection_and_opens_nothing() {
        let armed = armed(false, NONE, "https://example.com").expect("the press arms the open");
        assert_eq!(released_on(Some(armed.clone()), Some((9, 0))), None);
        assert_eq!(released_on(Some(armed), None), None);
        assert_eq!(released_on(None, Some((1, 0))), None);
    }

    #[test]
    fn a_press_beside_the_run_or_on_a_refused_scheme_arms_nothing() {
        assert_eq!(
            armed_by_press(
                true,
                false,
                NONE,
                Some((9, 0)),
                Some(&run("https://example.com"))
            ),
            None,
            "the pointer must be on the run the hover found"
        );
        assert_eq!(armed(false, NONE, "javascript:alert(1)"), None);
        assert_eq!(armed_by_press(true, false, NONE, Some((1, 0)), None), None);
    }

    #[test]
    fn opening_switched_off_arms_nothing() {
        assert_eq!(
            armed_by_press(
                false,
                false,
                NONE,
                Some((1, 0)),
                Some(&run("https://example.com"))
            ),
            None
        );
    }

    #[test]
    fn a_link_is_reachable_only_when_this_click_would_really_open_it() {
        let allowed = run("https://example.com");
        let refused = run("javascript:alert(1)");
        assert!(is_reachable(true, false, NONE, Some(&allowed)));
        assert!(
            !is_reachable(true, true, NONE, Some(&allowed)),
            "a pane watching the mouse needs shift, so a bare pointer promises nothing"
        );
        assert!(is_reachable(
            true,
            true,
            ModifiersState::SHIFT,
            Some(&allowed)
        ));
        assert!(
            !is_reachable(true, false, NONE, Some(&refused)),
            "a scheme the allow-list refuses must not be advertised as openable"
        );
        assert!(!is_reachable(true, false, NONE, None));
        assert!(
            !is_reachable(false, false, NONE, Some(&allowed)),
            "with opening switched off nothing is reachable"
        );
    }

    #[test]
    fn the_opener_is_named_for_this_platform() {
        if cfg!(target_os = "linux") {
            assert_eq!(opener(), Some("xdg-open"));
        } else if cfg!(target_os = "macos") {
            assert_eq!(opener(), Some("open"));
        } else if cfg!(windows) {
            assert_eq!(opener(), Some("ShellExecuteW"));
        } else {
            assert_eq!(opener(), None);
        }
    }

    fn narrow(wide: &[u16]) -> String {
        assert_eq!(wide.last(), Some(&0), "a wide string must end in NUL");
        String::from_utf16(&wide[..wide.len() - 1]).unwrap()
    }

    #[test]
    fn the_windows_opener_passes_the_link_whole_to_the_open_verb() {
        let uri = "https://example.com/a b?q=\"x\"&y=%20|dir";
        let args = shell_execute_args(uri).unwrap();
        assert_eq!(narrow(&args.verb), "open");
        assert_eq!(narrow(&args.file), uri);
        assert_eq!(args.file.iter().filter(|unit| **unit == 0).count(), 1);
    }

    #[test]
    fn the_windows_opener_keeps_characters_outside_the_basic_plane() {
        let uri = "https://example.com/\u{1f600}/\u{4f60}";
        let args = shell_execute_args(uri).unwrap();
        assert_eq!(narrow(&args.file), uri);
        assert!(args.file.len() > uri.chars().count() + 1);
    }

    #[test]
    fn the_windows_opener_refuses_a_link_with_an_embedded_nul() {
        assert!(shell_execute_args("https://example.com/\0calc.exe").is_err());
    }
}
