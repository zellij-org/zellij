const ESC_BYTE: u8 = 0x1b;

const fn ctrl(character: u8) -> [u8; 1] {
    [character & 0x1f]
}

const fn alt(character: u8) -> [u8; 2] {
    [ESC_BYTE, character]
}

pub const QUIT: [u8; 1] = ctrl(b'q');
pub const ESC: [u8; 1] = [ESC_BYTE];
pub const ENTER: [u8; 1] = [b'\r'];
pub const SPACE: [u8; 1] = [b' '];
pub const LOCK_MODE: [u8; 1] = ctrl(b'g');

pub const MOVE_FOCUS_LEFT_IN_NORMAL_MODE: [u8; 2] = alt(b'h');
pub const MOVE_FOCUS_RIGHT_IN_NORMAL_MODE: [u8; 2] = alt(b'l');

pub const PANE_MODE: [u8; 1] = ctrl(b'p');
pub const TMUX_MODE: [u8; 1] = ctrl(b'b');
pub const SPAWN_TERMINAL_IN_PANE_MODE: [u8; 1] = [b'n'];
pub const MOVE_FOCUS_IN_PANE_MODE: [u8; 1] = [b'p'];
pub const SPLIT_DOWN_IN_PANE_MODE: [u8; 1] = [b'd'];
pub const SPLIT_RIGHT_IN_PANE_MODE: [u8; 1] = [b'r'];
pub const SPLIT_RIGHT_IN_TMUX_MODE: [u8; 1] = [b'%'];
pub const TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE: [u8; 1] = [b'f'];
pub const TOGGLE_FLOATING_PANES: [u8; 1] = [b'w'];
pub const CLOSE_PANE_IN_PANE_MODE: [u8; 1] = [b'x'];
pub const MOVE_FOCUS_DOWN_IN_PANE_MODE: [u8; 1] = [b'j'];
pub const MOVE_FOCUS_UP_IN_PANE_MODE: [u8; 1] = [b'k'];
pub const MOVE_FOCUS_LEFT_IN_PANE_MODE: [u8; 1] = [b'h'];
pub const MOVE_FOCUS_RIGHT_IN_PANE_MODE: [u8; 1] = [b'l'];
pub const RENAME_PANE_MODE: [u8; 1] = [b'c'];

pub const SCROLL_MODE: [u8; 1] = ctrl(b's');
pub const SCROLL_UP_IN_SCROLL_MODE: [u8; 1] = [b'k'];
pub const SCROLL_DOWN_IN_SCROLL_MODE: [u8; 1] = [b'j'];
pub const SCROLL_PAGE_UP_IN_SCROLL_MODE: [u8; 1] = ctrl(b'b');
pub const SCROLL_PAGE_DOWN_IN_SCROLL_MODE: [u8; 1] = ctrl(b'f');
pub const EDIT_SCROLLBACK: [u8; 1] = [b'e'];

pub const RESIZE_MODE: [u8; 1] = ctrl(b'n');
pub const RESIZE_DOWN_IN_RESIZE_MODE: [u8; 1] = [b'j'];
pub const RESIZE_UP_IN_RESIZE_MODE: [u8; 1] = [b'k'];
pub const RESIZE_LEFT_IN_RESIZE_MODE: [u8; 1] = [b'h'];
pub const RESIZE_RIGHT_IN_RESIZE_MODE: [u8; 1] = [b'l'];

pub const TAB_MODE: [u8; 1] = ctrl(b't');
pub const NEW_TAB_IN_TAB_MODE: [u8; 1] = [b'n'];
pub const SWITCH_NEXT_TAB_IN_TAB_MODE: [u8; 1] = [b'l'];
pub const SWITCH_PREV_TAB_IN_TAB_MODE: [u8; 1] = [b'h'];
pub const CLOSE_TAB_IN_TAB_MODE: [u8; 1] = [b'x'];
pub const RENAME_TAB_MODE: [u8; 1] = [b'r'];

pub const MOVE_TAB_LEFT: [u8; 2] = alt(b'i');
pub const MOVE_TAB_RIGHT: [u8; 2] = alt(b'o');

pub const SESSION_MODE: [u8; 1] = ctrl(b'o');
pub const DETACH_IN_SESSION_MODE: [u8; 1] = [b'd'];

pub const BRACKETED_PASTE_START: [u8; 6] = *b"\x1b[200~";
pub const BRACKETED_PASTE_END: [u8; 6] = *b"\x1b[201~";
