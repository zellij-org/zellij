use super::cases::{
    MOVE_FOCUS_LEFT_IN_NORMAL_MODE, MOVE_TAB_LEFT, MOVE_TAB_RIGHT, NEW_TAB_IN_TAB_MODE,
    SECOND_TAB_CONTENT, TAB_MODE,
};
use super::remote_runner::{RemoteTerminal, Step};

pub fn new_tab() -> Step {
    Step {
        name: "Open new tab",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() {
                remote_terminal.send_key(&TAB_MODE);
                std::thread::sleep(std::time::Duration::from_millis(100));
                remote_terminal.send_key(&NEW_TAB_IN_TAB_MODE);
                step_is_complete = true;
            }
            step_is_complete
        },
    }
}

pub fn check_second_tab_opened() -> Step {
    Step {
        name: "Check second tab opened",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            remote_terminal.status_bar_appears() && remote_terminal.snapshot_contains("Tab #2")
        },
    }
}

pub fn move_tab_left() -> Step {
    Step {
        name: "Move tab left",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() {
                remote_terminal.send_key(&MOVE_TAB_LEFT);
                std::thread::sleep(std::time::Duration::from_millis(100));
                step_is_complete = true;
            }
            step_is_complete
        },
    }
}

pub fn check_third_tab_moved_left() -> Step {
    Step {
        name: "Check third tab is in the middle",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            remote_terminal.status_bar_appears()
                && remote_terminal.snapshot_contains("Tab #1  Tab #3  Tab #2")
        },
    }
}

pub fn type_second_tab_content() -> Step {
    Step {
        name: "Type second tab content",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() {
                remote_terminal.send_key(&SECOND_TAB_CONTENT);
                step_is_complete = true;
            }
            step_is_complete
        },
    }
}

pub fn check_third_tab_opened() -> Step {
    Step {
        name: "Check third tab opened",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            remote_terminal.status_bar_appears() && remote_terminal.snapshot_contains("Tab #3")
        },
    }
}

pub fn switch_focus_to_left_tab() -> Step {
    Step {
        name: "Move focus to tab on the left",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() {
                remote_terminal.send_key(&MOVE_FOCUS_LEFT_IN_NORMAL_MODE);
                step_is_complete = true;
            }
            step_is_complete
        },
    }
}

pub fn check_focus_on_second_tab() -> Step {
    Step {
        name: "Check focus is on the second tab",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            remote_terminal.status_bar_appears()
                && remote_terminal.snapshot_contains("Tab #2 content")
        },
    }
}

pub fn move_tab_right() -> Step {
    Step {
        name: "Move tab right",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() {
                remote_terminal.send_key(&MOVE_TAB_RIGHT);
                step_is_complete = true;
            }
            step_is_complete
        },
    }
}

pub fn check_third_tab_moved_to_beginning() -> Step {
    Step {
        name: "Check third tab moved to beginning",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            remote_terminal.status_bar_appears()
                && remote_terminal.snapshot_contains("Tab #3  Tab #1  Tab #2")
        },
    }
}

pub fn check_third_tab_is_left_wrapped() -> Step {
    Step {
        name: "Check third tab is in last position",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            remote_terminal.status_bar_appears()
                && remote_terminal.snapshot_contains("Tab #2  Tab #1  Tab #3")
        },
    }
}

pub fn check_third_tab_is_right_wrapped() -> Step {
    Step {
        name: "Check third tab is in last position",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            remote_terminal.status_bar_appears()
                && remote_terminal.snapshot_contains("Tab #3  Tab #2  Tab #1")
        },
    }
}
