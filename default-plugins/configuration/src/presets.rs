pub fn unlock_first_keybinds(primary_modifier: String, secondary_modifier: String) -> String {
    format!(
        r#"
default_mode "locked"
keybinds clear-defaults=true {{
    normal {{
    }}
    locked {{
        bind "{primary_modifier} g" {{ SwitchToMode "Normal"; }}
    }}
    resize {{
        bind "r" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ Resize "Increase Left"; }}
        bind "j" "Down" {{ Resize "Increase Down"; }}
        bind "k" "Up" {{ Resize "Increase Up"; }}
        bind "l" "Right" {{ Resize "Increase Right"; }}
        bind "H" {{ Resize "Decrease Left"; }}
        bind "J" {{ Resize "Decrease Down"; }}
        bind "K" {{ Resize "Decrease Up"; }}
        bind "L" {{ Resize "Decrease Right"; }}
        bind "=" "+" {{ Resize "Increase"; }}
        bind "-" {{ Resize "Decrease"; }}
    }}
    pane {{
        bind "p" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ MoveFocus "Left"; }}
        bind "l" "Right" {{ MoveFocus "Right"; }}
        bind "j" "Down" {{ MoveFocus "Down"; }}
        bind "k" "Up" {{ MoveFocus "Up"; }}
        bind "Tab" {{ SwitchFocus; }}
        bind "n" {{ NewPane; SwitchToMode "Locked"; }}
        bind "d" {{ NewPane "Down"; SwitchToMode "Locked"; }}
        bind "r" {{ NewPane "Right"; SwitchToMode "Locked"; }}
        bind "s" {{ NewPane "stacked"; SwitchToMode "Locked"; }}
        bind "x" {{ CloseFocus; SwitchToMode "Locked"; }}
        bind "f" {{ ToggleFocusFullscreen; SwitchToMode "Locked"; }}
        bind "z" {{ TogglePaneFrames; SwitchToMode "Locked"; }}
        bind "w" {{ ToggleFloatingPanes; SwitchToMode "Locked"; }}
        bind "e" {{ TogglePaneEmbedOrFloating; SwitchToMode "Locked"; }}
        bind "c" {{ SwitchToMode "RenamePane"; PaneNameInput 0;}}
        bind "i" {{ TogglePanePinned; SwitchToMode "Locked"; }}
    }}
    move {{
        bind "m" {{ SwitchToMode "Normal"; }}
        bind "n" "Tab" {{ MovePane; }}
        bind "p" {{ MovePaneBackwards; }}
        bind "h" "Left" {{ MovePane "Left"; }}
        bind "j" "Down" {{ MovePane "Down"; }}
        bind "k" "Up" {{ MovePane "Up"; }}
        bind "l" "Right" {{ MovePane "Right"; }}
    }}
    tab {{
        bind "t" {{ SwitchToMode "Normal"; }}
        bind "r" {{ SwitchToMode "RenameTab"; TabNameInput 0; }}
        bind "h" "Left" "Up" "k" {{ GoToPreviousTab; }}
        bind "l" "Right" "Down" "j" {{ GoToNextTab; }}
        bind "n" {{ NewTab; SwitchToMode "Locked"; }}
        bind "x" {{ CloseTab; SwitchToMode "Locked"; }}
        bind "s" {{ ToggleActiveSyncTab; SwitchToMode "Locked"; }}
        bind "b" {{ BreakPane; SwitchToMode "Locked"; }}
        bind "]" {{ BreakPaneRight; SwitchToMode "Locked"; }}
        bind "[" {{ BreakPaneLeft; SwitchToMode "Locked"; }}
        bind "1" {{ GoToTab 1; SwitchToMode "Locked"; }}
        bind "2" {{ GoToTab 2; SwitchToMode "Locked"; }}
        bind "3" {{ GoToTab 3; SwitchToMode "Locked"; }}
        bind "4" {{ GoToTab 4; SwitchToMode "Locked"; }}
        bind "5" {{ GoToTab 5; SwitchToMode "Locked"; }}
        bind "6" {{ GoToTab 6; SwitchToMode "Locked"; }}
        bind "7" {{ GoToTab 7; SwitchToMode "Locked"; }}
        bind "8" {{ GoToTab 8; SwitchToMode "Locked"; }}
        bind "9" {{ GoToTab 9; SwitchToMode "Locked"; }}
        bind "Tab" {{ ToggleTab; }}
    }}
    scroll {{
        bind "s" {{ SwitchToMode "Normal"; }}
        bind "e" {{ EditScrollback; SwitchToMode "Locked"; }}
        bind "f" {{ SwitchToMode "EnterSearch"; SearchInput 0; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Locked"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "Alt left" {{ MoveFocusOrTab "left"; SwitchToMode "locked"; }}
        bind "Alt down" {{ MoveFocus "down"; SwitchToMode "locked"; }}
        bind "Alt up" {{ MoveFocus "up"; SwitchToMode "locked"; }}
        bind "Alt right" {{ MoveFocusOrTab "right"; SwitchToMode "locked"; }}
        bind "Alt h" {{ MoveFocusOrTab "left"; SwitchToMode "locked"; }}
        bind "Alt j" {{ MoveFocus "down"; SwitchToMode "locked"; }}
        bind "Alt k" {{ MoveFocus "up"; SwitchToMode "locked"; }}
        bind "Alt l" {{ MoveFocusOrTab "right"; SwitchToMode "locked"; }}
    }}
    search {{
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Locked"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "n" {{ Search "down"; }}
        bind "p" {{ Search "up"; }}
        bind "c" {{ SearchToggleOption "CaseSensitivity"; }}
        bind "w" {{ SearchToggleOption "Wrap"; }}
        bind "o" {{ SearchToggleOption "WholeWord"; }}
    }}
    entersearch {{
        bind "Ctrl c" "Esc" {{ SwitchToMode "Scroll"; }}
        bind "Enter" {{ SwitchToMode "Search"; }}
    }}
    renametab {{
        bind "Ctrl c" "Enter" {{ SwitchToMode "Locked"; }}
        bind "Esc" {{ UndoRenameTab; SwitchToMode "Tab"; }}
    }}
    renamepane {{
        bind "Ctrl c" "Enter" {{ SwitchToMode "Locked"; }}
        bind "Esc" {{ UndoRenamePane; SwitchToMode "Pane"; }}
    }}
    session {{
        bind "o" {{ SwitchToMode "Normal"; }}
        bind "d" {{ Detach; }}
        bind "w" {{
            LaunchOrFocusPlugin "session-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Locked"
        }}
        bind "c" {{
            LaunchOrFocusPlugin "configuration" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Locked"
        }}
        bind "p" {{
            LaunchOrFocusPlugin "plugin-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Locked"
        }}
        bind "a" {{
            LaunchOrFocusPlugin "zellij:about" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Locked"
        }}
        bind "s" {{
            LaunchOrFocusPlugin "zellij:share" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Locked"
        }}
        bind "l" {{
            LaunchOrFocusPlugin "zellij:layout-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Locked"
        }}
    }}
    shared_except "locked" "renametab" "renamepane" {{
        bind "{primary_modifier} g" {{ SwitchToMode "Locked"; }}
        bind "{primary_modifier} q" {{ Quit; }}
    }}
    shared_except "renamepane" "renametab" "entersearch" "locked" {{
        bind "esc" {{ SwitchToMode "locked"; }}
    }}
    shared_among "normal" "locked" {{
        bind "{secondary_modifier} n" {{ NewPane; }}
        bind "{secondary_modifier} f" {{ ToggleFloatingPanes; }}
        bind "{secondary_modifier} i" {{ MoveTab "Left"; }}
        bind "{secondary_modifier} o" {{ MoveTab "Right"; }}
        bind "{secondary_modifier} h" "{secondary_modifier} Left" {{ MoveFocusOrTab "Left"; }}
        bind "{secondary_modifier} l" "{secondary_modifier} Right" {{ MoveFocusOrTab "Right"; }}
        bind "{secondary_modifier} j" "{secondary_modifier} Down" {{ MoveFocus "Down"; }}
        bind "{secondary_modifier} k" "{secondary_modifier} Up" {{ MoveFocus "Up"; }}
        bind "{secondary_modifier} =" "{secondary_modifier} +" {{ Resize "Increase"; }}
        bind "{secondary_modifier} -" {{ Resize "Decrease"; }}
        bind "{secondary_modifier} [" {{ PreviousSwapLayout; }}
        bind "{secondary_modifier} ]" {{ NextSwapLayout; }}
        bind "{secondary_modifier} p" {{ TogglePaneInGroup; }}
        bind "{secondary_modifier} Shift p" {{ ToggleGroupMarking; }}
    }}
    shared_except "locked" "renametab" "renamepane" {{
        bind "Enter" {{ SwitchToMode "Locked"; }}
    }}
    shared_except "pane" "locked" "renametab" "renamepane" "entersearch" {{
        bind "p" {{ SwitchToMode "Pane"; }}
    }}
    shared_except "resize" "locked" "renametab" "renamepane" "entersearch" {{
        bind "r" {{ SwitchToMode "Resize"; }}
    }}
    shared_except "scroll" "locked" "renametab" "renamepane" "entersearch" {{
        bind "s" {{ SwitchToMode "Scroll"; }}
    }}
    shared_except "session" "locked" "renametab" "renamepane" "entersearch" {{
        bind "o" {{ SwitchToMode "Session"; }}
    }}
    shared_except "tab" "locked" "renametab" "renamepane" "entersearch" {{
        bind "t" {{ SwitchToMode "Tab"; }}
    }}
    shared_except "move" "locked" "renametab" "renamepane" "entersearch" {{
        bind "m" {{ SwitchToMode "Move"; }}
    }}
}}"#
    )
}

pub fn default_keybinds(primary_modifier: String, secondary_modifier: String) -> String {
    if primary_modifier.is_empty() && secondary_modifier.is_empty() {
        return default_keybinds_no_modifiers();
    } else if primary_modifier == secondary_modifier {
        return non_colliding_default_keybinds(primary_modifier, secondary_modifier);
    } else if primary_modifier.is_empty() {
        return default_keybinds_no_primary_modifier(secondary_modifier);
    } else if secondary_modifier.is_empty() {
        return default_keybinds_no_secondary_modifier(primary_modifier);
    }
    format!(
        r#"
default_mode "normal"
keybinds clear-defaults=true {{
    normal {{}}
    locked {{
        bind "{primary_modifier} g" {{ SwitchToMode "Normal"; }}
    }}
    resize {{
        bind "{primary_modifier} n" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ Resize "Increase Left"; }}
        bind "j" "Down" {{ Resize "Increase Down"; }}
        bind "k" "Up" {{ Resize "Increase Up"; }}
        bind "l" "Right" {{ Resize "Increase Right"; }}
        bind "H" {{ Resize "Decrease Left"; }}
        bind "J" {{ Resize "Decrease Down"; }}
        bind "K" {{ Resize "Decrease Up"; }}
        bind "L" {{ Resize "Decrease Right"; }}
        bind "=" "+" {{ Resize "Increase"; }}
        bind "-" {{ Resize "Decrease"; }}
    }}
    pane {{
        bind "{primary_modifier} p" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ MoveFocus "Left"; }}
        bind "l" "Right" {{ MoveFocus "Right"; }}
        bind "j" "Down" {{ MoveFocus "Down"; }}
        bind "k" "Up" {{ MoveFocus "Up"; }}
        bind "p" {{ SwitchFocus; }}
        bind "n" {{ NewPane; SwitchToMode "Normal"; }}
        bind "d" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "r" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "s" {{ NewPane "stacked"; SwitchToMode "Normal"; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
        bind "f" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "z" {{ TogglePaneFrames; SwitchToMode "Normal"; }}
        bind "w" {{ ToggleFloatingPanes; SwitchToMode "Normal"; }}
        bind "e" {{ TogglePaneEmbedOrFloating; SwitchToMode "Normal"; }}
        bind "c" {{ SwitchToMode "RenamePane"; PaneNameInput 0;}}
        bind "i" {{ TogglePanePinned; SwitchToMode "Normal"; }}
    }}
    move {{
        bind "{primary_modifier} h" {{ SwitchToMode "Normal"; }}
        bind "n" "Tab" {{ MovePane; }}
        bind "p" {{ MovePaneBackwards; }}
        bind "h" "Left" {{ MovePane "Left"; }}
        bind "j" "Down" {{ MovePane "Down"; }}
        bind "k" "Up" {{ MovePane "Up"; }}
        bind "l" "Right" {{ MovePane "Right"; }}
    }}
    tab {{
        bind "{primary_modifier} t" {{ SwitchToMode "Normal"; }}
        bind "r" {{ SwitchToMode "RenameTab"; TabNameInput 0; }}
        bind "h" "Left" "Up" "k" {{ GoToPreviousTab; }}
        bind "l" "Right" "Down" "j" {{ GoToNextTab; }}
        bind "n" {{ NewTab; SwitchToMode "Normal"; }}
        bind "x" {{ CloseTab; SwitchToMode "Normal"; }}
        bind "s" {{ ToggleActiveSyncTab; SwitchToMode "Normal"; }}
        bind "b" {{ BreakPane; SwitchToMode "Normal"; }}
        bind "]" {{ BreakPaneRight; SwitchToMode "Normal"; }}
        bind "[" {{ BreakPaneLeft; SwitchToMode "Normal"; }}
        bind "1" {{ GoToTab 1; SwitchToMode "Normal"; }}
        bind "2" {{ GoToTab 2; SwitchToMode "Normal"; }}
        bind "3" {{ GoToTab 3; SwitchToMode "Normal"; }}
        bind "4" {{ GoToTab 4; SwitchToMode "Normal"; }}
        bind "5" {{ GoToTab 5; SwitchToMode "Normal"; }}
        bind "6" {{ GoToTab 6; SwitchToMode "Normal"; }}
        bind "7" {{ GoToTab 7; SwitchToMode "Normal"; }}
        bind "8" {{ GoToTab 8; SwitchToMode "Normal"; }}
        bind "9" {{ GoToTab 9; SwitchToMode "Normal"; }}
        bind "Tab" {{ ToggleTab; }}
    }}
    scroll {{
        bind "{primary_modifier} s" {{ SwitchToMode "Normal"; }}
        bind "e" {{ EditScrollback; SwitchToMode "Normal"; }}
        bind "s" {{ SwitchToMode "EnterSearch"; SearchInput 0; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "{secondary_modifier} left" {{ MoveFocusOrTab "left"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} down" {{ MoveFocus "down"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} up" {{ MoveFocus "up"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} right" {{ MoveFocusOrTab "right"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} h" {{ MoveFocusOrTab "left"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} j" {{ MoveFocus "down"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} k" {{ MoveFocus "up"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} l" {{ MoveFocusOrTab "right"; SwitchToMode "normal"; }}
    }}
    search {{
        bind "{primary_modifier} s" {{ SwitchToMode "Normal"; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "n" {{ Search "down"; }}
        bind "p" {{ Search "up"; }}
        bind "c" {{ SearchToggleOption "CaseSensitivity"; }}
        bind "w" {{ SearchToggleOption "Wrap"; }}
        bind "o" {{ SearchToggleOption "WholeWord"; }}
    }}
    entersearch {{
        bind "Ctrl c" "Esc" {{ SwitchToMode "Scroll"; }}
        bind "Enter" {{ SwitchToMode "Search"; }}
    }}
    renametab {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenameTab; SwitchToMode "Tab"; }}
    }}
    renamepane {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenamePane; SwitchToMode "Pane"; }}
    }}
    session {{
        bind "{primary_modifier} o" {{ SwitchToMode "Normal"; }}
        bind "{primary_modifier} s" {{ SwitchToMode "Scroll"; }}
        bind "d" {{ Detach; }}
        bind "w" {{
            LaunchOrFocusPlugin "session-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "c" {{
            LaunchOrFocusPlugin "configuration" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "p" {{
            LaunchOrFocusPlugin "plugin-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "a" {{
            LaunchOrFocusPlugin "zellij:about" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "s" {{
            LaunchOrFocusPlugin "zellij:share" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "l" {{
            LaunchOrFocusPlugin "zellij:layout-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
    }}
    tmux {{
        bind "[" {{ SwitchToMode "Scroll"; }}
        bind "{primary_modifier} b" {{ Write 2; SwitchToMode "Normal"; }}
        bind "\"" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "%" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "z" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "c" {{ NewTab; SwitchToMode "Normal"; }}
        bind "," {{ SwitchToMode "RenameTab"; }}
        bind "p" {{ GoToPreviousTab; SwitchToMode "Normal"; }}
        bind "n" {{ GoToNextTab; SwitchToMode "Normal"; }}
        bind "Left" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "Right" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "Down" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "Up" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "h" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "l" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "j" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "k" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "o" {{ FocusNextPane; }}
        bind "d" {{ Detach; }}
        bind "Space" {{ NextSwapLayout; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
    }}
    shared_except "locked" {{
        bind "{primary_modifier} g" {{ SwitchToMode "Locked"; }}
        bind "{primary_modifier} q" {{ Quit; }}
        bind "{secondary_modifier} f" {{ ToggleFloatingPanes; }}
        bind "{secondary_modifier} n" {{ NewPane; }}
        bind "{secondary_modifier} i" {{ MoveTab "Left"; }}
        bind "{secondary_modifier} o" {{ MoveTab "Right"; }}
        bind "{secondary_modifier} h" "{secondary_modifier} Left" {{ MoveFocusOrTab "Left"; }}
        bind "{secondary_modifier} l" "{secondary_modifier} Right" {{ MoveFocusOrTab "Right"; }}
        bind "{secondary_modifier} j" "{secondary_modifier} Down" {{ MoveFocus "Down"; }}
        bind "{secondary_modifier} k" "{secondary_modifier} Up" {{ MoveFocus "Up"; }}
        bind "{secondary_modifier} =" "{secondary_modifier} +" {{ Resize "Increase"; }}
        bind "{secondary_modifier} -" {{ Resize "Decrease"; }}
        bind "{secondary_modifier} [" {{ PreviousSwapLayout; }}
        bind "{secondary_modifier} ]" {{ NextSwapLayout; }}
        bind "{secondary_modifier} p" {{ TogglePaneInGroup; }}
        bind "{secondary_modifier} Shift p" {{ ToggleGroupMarking; }}
    }}
    shared_except "normal" "locked" {{
        bind "Enter" "Esc" {{ SwitchToMode "Normal"; }}
    }}
    shared_except "pane" "locked" {{
        bind "{primary_modifier} p" {{ SwitchToMode "Pane"; }}
    }}
    shared_except "resize" "locked" {{
        bind "{primary_modifier} n" {{ SwitchToMode "Resize"; }}
    }}
    shared_except "scroll" "locked" {{
        bind "{primary_modifier} s" {{ SwitchToMode "Scroll"; }}
    }}
    shared_except "session" "locked" {{
        bind "{primary_modifier} o" {{ SwitchToMode "Session"; }}
    }}
    shared_except "tab" "locked" {{
        bind "{primary_modifier} t" {{ SwitchToMode "Tab"; }}
    }}
    shared_except "move" "locked" {{
        bind "{primary_modifier} h" {{ SwitchToMode "Move"; }}
    }}
    shared_except "tmux" "locked" {{
        bind "{primary_modifier} b" {{ SwitchToMode "Tmux"; }}
    }}
}}
"#
    )
}

pub fn default_keybinds_no_primary_modifier(secondary_modifier: String) -> String {
    format!(
        r#"
default_mode "normal"
keybinds clear-defaults=true {{
    normal {{}}
    locked {{}}
    resize {{
        bind "h" "Left" {{ Resize "Increase Left"; }}
        bind "j" "Down" {{ Resize "Increase Down"; }}
        bind "k" "Up" {{ Resize "Increase Up"; }}
        bind "l" "Right" {{ Resize "Increase Right"; }}
        bind "H" {{ Resize "Decrease Left"; }}
        bind "J" {{ Resize "Decrease Down"; }}
        bind "K" {{ Resize "Decrease Up"; }}
        bind "L" {{ Resize "Decrease Right"; }}
        bind "=" "+" {{ Resize "Increase"; }}
        bind "-" {{ Resize "Decrease"; }}
    }}
    pane {{
        bind "h" "Left" {{ MoveFocus "Left"; }}
        bind "l" "Right" {{ MoveFocus "Right"; }}
        bind "j" "Down" {{ MoveFocus "Down"; }}
        bind "k" "Up" {{ MoveFocus "Up"; }}
        bind "p" {{ SwitchFocus; }}
        bind "n" {{ NewPane; SwitchToMode "Normal"; }}
        bind "d" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "r" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "s" {{ NewPane "stacked"; SwitchToMode "Normal"; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
        bind "f" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "z" {{ TogglePaneFrames; SwitchToMode "Normal"; }}
        bind "w" {{ ToggleFloatingPanes; SwitchToMode "Normal"; }}
        bind "e" {{ TogglePaneEmbedOrFloating; SwitchToMode "Normal"; }}
        bind "c" {{ SwitchToMode "RenamePane"; PaneNameInput 0;}}
        bind "i" {{ TogglePanePinned; SwitchToMode "Normal"; }}
    }}
    move {{
        bind "n" "Tab" {{ MovePane; }}
        bind "p" {{ MovePaneBackwards; }}
        bind "h" "Left" {{ MovePane "Left"; }}
        bind "j" "Down" {{ MovePane "Down"; }}
        bind "k" "Up" {{ MovePane "Up"; }}
        bind "l" "Right" {{ MovePane "Right"; }}
    }}
    tab {{
        bind "r" {{ SwitchToMode "RenameTab"; TabNameInput 0; }}
        bind "h" "Left" "Up" "k" {{ GoToPreviousTab; }}
        bind "l" "Right" "Down" "j" {{ GoToNextTab; }}
        bind "n" {{ NewTab; SwitchToMode "Normal"; }}
        bind "x" {{ CloseTab; SwitchToMode "Normal"; }}
        bind "s" {{ ToggleActiveSyncTab; SwitchToMode "Normal"; }}
        bind "b" {{ BreakPane; SwitchToMode "Normal"; }}
        bind "]" {{ BreakPaneRight; SwitchToMode "Normal"; }}
        bind "[" {{ BreakPaneLeft; SwitchToMode "Normal"; }}
        bind "1" {{ GoToTab 1; SwitchToMode "Normal"; }}
        bind "2" {{ GoToTab 2; SwitchToMode "Normal"; }}
        bind "3" {{ GoToTab 3; SwitchToMode "Normal"; }}
        bind "4" {{ GoToTab 4; SwitchToMode "Normal"; }}
        bind "5" {{ GoToTab 5; SwitchToMode "Normal"; }}
        bind "6" {{ GoToTab 6; SwitchToMode "Normal"; }}
        bind "7" {{ GoToTab 7; SwitchToMode "Normal"; }}
        bind "8" {{ GoToTab 8; SwitchToMode "Normal"; }}
        bind "9" {{ GoToTab 9; SwitchToMode "Normal"; }}
        bind "Tab" {{ ToggleTab; }}
    }}
    scroll {{
        bind "e" {{ EditScrollback; SwitchToMode "Normal"; }}
        bind "s" {{ SwitchToMode "EnterSearch"; SearchInput 0; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "{secondary_modifier} left" {{ MoveFocusOrTab "left"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} down" {{ MoveFocus "down"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} up" {{ MoveFocus "up"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} right" {{ MoveFocusOrTab "right"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} h" {{ MoveFocusOrTab "left"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} j" {{ MoveFocus "down"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} k" {{ MoveFocus "up"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} l" {{ MoveFocusOrTab "right"; SwitchToMode "normal"; }}
    }}
    search {{
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "n" {{ Search "down"; }}
        bind "p" {{ Search "up"; }}
        bind "c" {{ SearchToggleOption "CaseSensitivity"; }}
        bind "w" {{ SearchToggleOption "Wrap"; }}
        bind "o" {{ SearchToggleOption "WholeWord"; }}
    }}
    entersearch {{
        bind "Ctrl c" "Esc" {{ SwitchToMode "Scroll"; }}
        bind "Enter" {{ SwitchToMode "Search"; }}
    }}
    renametab {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenameTab; SwitchToMode "Tab"; }}
    }}
    renamepane {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenamePane; SwitchToMode "Pane"; }}
    }}
    session {{
        bind "d" {{ Detach; }}
        bind "w" {{
            LaunchOrFocusPlugin "session-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "c" {{
            LaunchOrFocusPlugin "configuration" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "p" {{
            LaunchOrFocusPlugin "plugin-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "a" {{
            LaunchOrFocusPlugin "zellij:about" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "s" {{
            LaunchOrFocusPlugin "zellij:share" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "l" {{
            LaunchOrFocusPlugin "zellij:layout-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
    }}
    tmux {{
        bind "[" {{ SwitchToMode "Scroll"; }}
        bind "\"" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "%" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "z" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "c" {{ NewTab; SwitchToMode "Normal"; }}
        bind "," {{ SwitchToMode "RenameTab"; }}
        bind "p" {{ GoToPreviousTab; SwitchToMode "Normal"; }}
        bind "n" {{ GoToNextTab; SwitchToMode "Normal"; }}
        bind "Left" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "Right" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "Down" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "Up" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "h" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "l" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "j" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "k" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "o" {{ FocusNextPane; }}
        bind "d" {{ Detach; }}
        bind "Space" {{ NextSwapLayout; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
    }}
    shared_except "locked" {{
        bind "{secondary_modifier} n" {{ NewPane; }}
        bind "{secondary_modifier} f" {{ ToggleFloatingPanes; }}
        bind "{secondary_modifier} i" {{ MoveTab "Left"; }}
        bind "{secondary_modifier} o" {{ MoveTab "Right"; }}
        bind "{secondary_modifier} h" "{secondary_modifier} Left" {{ MoveFocusOrTab "Left"; }}
        bind "{secondary_modifier} l" "{secondary_modifier} Right" {{ MoveFocusOrTab "Right"; }}
        bind "{secondary_modifier} j" "{secondary_modifier} Down" {{ MoveFocus "Down"; }}
        bind "{secondary_modifier} k" "{secondary_modifier} Up" {{ MoveFocus "Up"; }}
        bind "{secondary_modifier} =" "{secondary_modifier} +" {{ Resize "Increase"; }}
        bind "{secondary_modifier} -" {{ Resize "Decrease"; }}
        bind "{secondary_modifier} [" {{ PreviousSwapLayout; }}
        bind "{secondary_modifier} ]" {{ NextSwapLayout; }}
        bind "{secondary_modifier} p" {{ TogglePaneInGroup; }}
        bind "{secondary_modifier} Shift p" {{ ToggleGroupMarking; }}
    }}
    shared_except "normal" "locked" {{
        bind "Enter" "Esc" {{ SwitchToMode "Normal"; }}
    }}
}}
"#
    )
}

pub fn default_keybinds_no_secondary_modifier(primary_modifier: String) -> String {
    format!(
        r#"
default_mode "normal"
keybinds clear-defaults=true {{
    normal {{}}
    locked {{
        bind "{primary_modifier} g" {{ SwitchToMode "Normal"; }}
    }}
    resize {{
        bind "{primary_modifier} n" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ Resize "Increase Left"; }}
        bind "j" "Down" {{ Resize "Increase Down"; }}
        bind "k" "Up" {{ Resize "Increase Up"; }}
        bind "l" "Right" {{ Resize "Increase Right"; }}
        bind "H" {{ Resize "Decrease Left"; }}
        bind "J" {{ Resize "Decrease Down"; }}
        bind "K" {{ Resize "Decrease Up"; }}
        bind "L" {{ Resize "Decrease Right"; }}
        bind "=" "+" {{ Resize "Increase"; }}
        bind "-" {{ Resize "Decrease"; }}
    }}
    pane {{
        bind "{primary_modifier} p" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ MoveFocus "Left"; }}
        bind "l" "Right" {{ MoveFocus "Right"; }}
        bind "j" "Down" {{ MoveFocus "Down"; }}
        bind "k" "Up" {{ MoveFocus "Up"; }}
        bind "p" {{ SwitchFocus; }}
        bind "n" {{ NewPane; SwitchToMode "Normal"; }}
        bind "d" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "r" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "s" {{ NewPane "stacked"; SwitchToMode "Normal"; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
        bind "f" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "z" {{ TogglePaneFrames; SwitchToMode "Normal"; }}
        bind "w" {{ ToggleFloatingPanes; SwitchToMode "Normal"; }}
        bind "e" {{ TogglePaneEmbedOrFloating; SwitchToMode "Normal"; }}
        bind "c" {{ SwitchToMode "RenamePane"; PaneNameInput 0;}}
        bind "i" {{ TogglePanePinned; SwitchToMode "Normal"; }}
    }}
    move {{
        bind "{primary_modifier} h" {{ SwitchToMode "Normal"; }}
        bind "n" "Tab" {{ MovePane; }}
        bind "p" {{ MovePaneBackwards; }}
        bind "h" "Left" {{ MovePane "Left"; }}
        bind "j" "Down" {{ MovePane "Down"; }}
        bind "k" "Up" {{ MovePane "Up"; }}
        bind "l" "Right" {{ MovePane "Right"; }}
    }}
    tab {{
        bind "{primary_modifier} t" {{ SwitchToMode "Normal"; }}
        bind "r" {{ SwitchToMode "RenameTab"; TabNameInput 0; }}
        bind "h" "Left" "Up" "k" {{ GoToPreviousTab; }}
        bind "l" "Right" "Down" "j" {{ GoToNextTab; }}
        bind "n" {{ NewTab; SwitchToMode "Normal"; }}
        bind "x" {{ CloseTab; SwitchToMode "Normal"; }}
        bind "s" {{ ToggleActiveSyncTab; SwitchToMode "Normal"; }}
        bind "b" {{ BreakPane; SwitchToMode "Normal"; }}
        bind "]" {{ BreakPaneRight; SwitchToMode "Normal"; }}
        bind "[" {{ BreakPaneLeft; SwitchToMode "Normal"; }}
        bind "1" {{ GoToTab 1; SwitchToMode "Normal"; }}
        bind "2" {{ GoToTab 2; SwitchToMode "Normal"; }}
        bind "3" {{ GoToTab 3; SwitchToMode "Normal"; }}
        bind "4" {{ GoToTab 4; SwitchToMode "Normal"; }}
        bind "5" {{ GoToTab 5; SwitchToMode "Normal"; }}
        bind "6" {{ GoToTab 6; SwitchToMode "Normal"; }}
        bind "7" {{ GoToTab 7; SwitchToMode "Normal"; }}
        bind "8" {{ GoToTab 8; SwitchToMode "Normal"; }}
        bind "9" {{ GoToTab 9; SwitchToMode "Normal"; }}
        bind "Tab" {{ ToggleTab; }}
    }}
    scroll {{
        bind "{primary_modifier} s" {{ SwitchToMode "Normal"; }}
        bind "e" {{ EditScrollback; SwitchToMode "Normal"; }}
        bind "s" {{ SwitchToMode "EnterSearch"; SearchInput 0; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
    }}
    search {{
        bind "{primary_modifier} s" {{ SwitchToMode "Normal"; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "n" {{ Search "down"; }}
        bind "p" {{ Search "up"; }}
        bind "c" {{ SearchToggleOption "CaseSensitivity"; }}
        bind "w" {{ SearchToggleOption "Wrap"; }}
        bind "o" {{ SearchToggleOption "WholeWord"; }}
    }}
    entersearch {{
        bind "Ctrl c" "Esc" {{ SwitchToMode "Scroll"; }}
        bind "Enter" {{ SwitchToMode "Search"; }}
    }}
    renametab {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenameTab; SwitchToMode "Tab"; }}
    }}
    renamepane {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenamePane; SwitchToMode "Pane"; }}
    }}
    session {{
        bind "{primary_modifier} o" {{ SwitchToMode "Normal"; }}
        bind "{primary_modifier} s" {{ SwitchToMode "Scroll"; }}
        bind "d" {{ Detach; }}
        bind "w" {{
            LaunchOrFocusPlugin "session-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "c" {{
            LaunchOrFocusPlugin "configuration" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "p" {{
            LaunchOrFocusPlugin "plugin-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "a" {{
            LaunchOrFocusPlugin "zellij:about" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "s" {{
            LaunchOrFocusPlugin "zellij:share" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "l" {{
            LaunchOrFocusPlugin "zellij:layout-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
    }}
    tmux {{
        bind "[" {{ SwitchToMode "Scroll"; }}
        bind "{primary_modifier} b" {{ Write 2; SwitchToMode "Normal"; }}
        bind "\"" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "%" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "z" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "c" {{ NewTab; SwitchToMode "Normal"; }}
        bind "," {{ SwitchToMode "RenameTab"; }}
        bind "p" {{ GoToPreviousTab; SwitchToMode "Normal"; }}
        bind "n" {{ GoToNextTab; SwitchToMode "Normal"; }}
        bind "Left" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "Right" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "Down" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "Up" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "h" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "l" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "j" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "k" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "o" {{ FocusNextPane; }}
        bind "d" {{ Detach; }}
        bind "Space" {{ NextSwapLayout; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
    }}
    shared_except "locked" {{
        bind "{primary_modifier} g" {{ SwitchToMode "Locked"; }}
        bind "{primary_modifier} q" {{ Quit; }}
    }}
    shared_except "normal" "locked" {{
        bind "Enter" "Esc" {{ SwitchToMode "Normal"; }}
    }}
    shared_except "pane" "locked" {{
        bind "{primary_modifier} p" {{ SwitchToMode "Pane"; }}
    }}
    shared_except "resize" "locked" {{
        bind "{primary_modifier} n" {{ SwitchToMode "Resize"; }}
    }}
    shared_except "scroll" "locked" {{
        bind "{primary_modifier} s" {{ SwitchToMode "Scroll"; }}
    }}
    shared_except "session" "locked" {{
        bind "{primary_modifier} o" {{ SwitchToMode "Session"; }}
    }}
    shared_except "tab" "locked" {{
        bind "{primary_modifier} t" {{ SwitchToMode "Tab"; }}
    }}
    shared_except "move" "locked" {{
        bind "{primary_modifier} h" {{ SwitchToMode "Move"; }}
    }}
    shared_except "tmux" "locked" {{
        bind "{primary_modifier} b" {{ SwitchToMode "Tmux"; }}
    }}
}}
"#
    )
}

pub fn default_keybinds_no_modifiers() -> String {
    format!(
        r#"
default_mode "normal"
keybinds clear-defaults=true {{
    normal {{}}
    locked {{}}
    resize {{
        bind "h" "Left" {{ Resize "Increase Left"; }}
        bind "j" "Down" {{ Resize "Increase Down"; }}
        bind "k" "Up" {{ Resize "Increase Up"; }}
        bind "l" "Right" {{ Resize "Increase Right"; }}
        bind "H" {{ Resize "Decrease Left"; }}
        bind "J" {{ Resize "Decrease Down"; }}
        bind "K" {{ Resize "Decrease Up"; }}
        bind "L" {{ Resize "Decrease Right"; }}
        bind "=" "+" {{ Resize "Increase"; }}
        bind "-" {{ Resize "Decrease"; }}
    }}
    pane {{
        bind "h" "Left" {{ MoveFocus "Left"; }}
        bind "l" "Right" {{ MoveFocus "Right"; }}
        bind "j" "Down" {{ MoveFocus "Down"; }}
        bind "k" "Up" {{ MoveFocus "Up"; }}
        bind "p" {{ SwitchFocus; }}
        bind "n" {{ NewPane; SwitchToMode "Normal"; }}
        bind "d" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "r" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "s" {{ NewPane "stacked"; SwitchToMode "Normal"; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
        bind "f" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "z" {{ TogglePaneFrames; SwitchToMode "Normal"; }}
        bind "w" {{ ToggleFloatingPanes; SwitchToMode "Normal"; }}
        bind "e" {{ TogglePaneEmbedOrFloating; SwitchToMode "Normal"; }}
        bind "c" {{ SwitchToMode "RenamePane"; PaneNameInput 0;}}
        bind "i" {{ TogglePanePinned; SwitchToMode "Normal"; }}
    }}
    move {{
        bind "n" "Tab" {{ MovePane; }}
        bind "p" {{ MovePaneBackwards; }}
        bind "h" "Left" {{ MovePane "Left"; }}
        bind "j" "Down" {{ MovePane "Down"; }}
        bind "k" "Up" {{ MovePane "Up"; }}
        bind "l" "Right" {{ MovePane "Right"; }}
    }}
    tab {{
        bind "r" {{ SwitchToMode "RenameTab"; TabNameInput 0; }}
        bind "h" "Left" "Up" "k" {{ GoToPreviousTab; }}
        bind "l" "Right" "Down" "j" {{ GoToNextTab; }}
        bind "n" {{ NewTab; SwitchToMode "Normal"; }}
        bind "x" {{ CloseTab; SwitchToMode "Normal"; }}
        bind "s" {{ ToggleActiveSyncTab; SwitchToMode "Normal"; }}
        bind "b" {{ BreakPane; SwitchToMode "Normal"; }}
        bind "]" {{ BreakPaneRight; SwitchToMode "Normal"; }}
        bind "[" {{ BreakPaneLeft; SwitchToMode "Normal"; }}
        bind "1" {{ GoToTab 1; SwitchToMode "Normal"; }}
        bind "2" {{ GoToTab 2; SwitchToMode "Normal"; }}
        bind "3" {{ GoToTab 3; SwitchToMode "Normal"; }}
        bind "4" {{ GoToTab 4; SwitchToMode "Normal"; }}
        bind "5" {{ GoToTab 5; SwitchToMode "Normal"; }}
        bind "6" {{ GoToTab 6; SwitchToMode "Normal"; }}
        bind "7" {{ GoToTab 7; SwitchToMode "Normal"; }}
        bind "8" {{ GoToTab 8; SwitchToMode "Normal"; }}
        bind "9" {{ GoToTab 9; SwitchToMode "Normal"; }}
        bind "Tab" {{ ToggleTab; }}
    }}
    scroll {{
        bind "e" {{ EditScrollback; SwitchToMode "Normal"; }}
        bind "s" {{ SwitchToMode "EnterSearch"; SearchInput 0; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
    }}
    search {{
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "n" {{ Search "down"; }}
        bind "p" {{ Search "up"; }}
        bind "c" {{ SearchToggleOption "CaseSensitivity"; }}
        bind "w" {{ SearchToggleOption "Wrap"; }}
        bind "o" {{ SearchToggleOption "WholeWord"; }}
    }}
    entersearch {{
        bind "Ctrl c" "Esc" {{ SwitchToMode "Scroll"; }}
        bind "Enter" {{ SwitchToMode "Search"; }}
    }}
    renametab {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenameTab; SwitchToMode "Tab"; }}
    }}
    renamepane {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenamePane; SwitchToMode "Pane"; }}
    }}
    session {{
        bind "d" {{ Detach; }}
        bind "w" {{
            LaunchOrFocusPlugin "session-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "c" {{
            LaunchOrFocusPlugin "configuration" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "p" {{
            LaunchOrFocusPlugin "plugin-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "a" {{
            LaunchOrFocusPlugin "zellij:about" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "s" {{
            LaunchOrFocusPlugin "zellij:share" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "l" {{
            LaunchOrFocusPlugin "zellij:layout-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
    }}
    tmux {{
        bind "[" {{ SwitchToMode "Scroll"; }}
        bind "\"" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "%" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "z" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "c" {{ NewTab; SwitchToMode "Normal"; }}
        bind "," {{ SwitchToMode "RenameTab"; }}
        bind "p" {{ GoToPreviousTab; SwitchToMode "Normal"; }}
        bind "n" {{ GoToNextTab; SwitchToMode "Normal"; }}
        bind "Left" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "Right" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "Down" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "Up" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "h" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "l" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "j" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "k" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "o" {{ FocusNextPane; }}
        bind "d" {{ Detach; }}
        bind "Space" {{ NextSwapLayout; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
    }}
    shared_except "normal" "locked" {{
        bind "Enter" "Esc" {{ SwitchToMode "Normal"; }}
    }}
}}
"#
    )
}

pub fn non_colliding_default_keybinds(
    primary_modifier: String,
    secondary_modifier: String,
) -> String {
    format!(
        r#"
default_mode "normal"
keybinds clear-defaults=true {{
    normal {{}}
    locked {{
        bind "{primary_modifier} g" {{ SwitchToMode "Normal"; }}
    }}
    resize {{
        bind "{primary_modifier} r" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ Resize "Increase Left"; }}
        bind "j" "Down" {{ Resize "Increase Down"; }}
        bind "k" "Up" {{ Resize "Increase Up"; }}
        bind "l" "Right" {{ Resize "Increase Right"; }}
        bind "H" {{ Resize "Decrease Left"; }}
        bind "J" {{ Resize "Decrease Down"; }}
        bind "K" {{ Resize "Decrease Up"; }}
        bind "L" {{ Resize "Decrease Right"; }}
        bind "=" "+" {{ Resize "Increase"; }}
        bind "-" {{ Resize "Decrease"; }}
    }}
    pane {{
        bind "{primary_modifier} p" {{ SwitchToMode "Normal"; }}
        bind "h" "Left" {{ MoveFocus "Left"; }}
        bind "l" "Right" {{ MoveFocus "Right"; }}
        bind "j" "Down" {{ MoveFocus "Down"; }}
        bind "k" "Up" {{ MoveFocus "Up"; }}
        bind "p" {{ SwitchFocus; }}
        bind "n" {{ NewPane; SwitchToMode "Normal"; }}
        bind "d" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "r" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "s" {{ NewPane "stacked"; SwitchToMode "Normal"; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
        bind "f" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "z" {{ TogglePaneFrames; SwitchToMode "Normal"; }}
        bind "w" {{ ToggleFloatingPanes; SwitchToMode "Normal"; }}
        bind "e" {{ TogglePaneEmbedOrFloating; SwitchToMode "Normal"; }}
        bind "c" {{ SwitchToMode "RenamePane"; PaneNameInput 0;}}
        bind "i" {{ TogglePanePinned; SwitchToMode "Normal"; }}
    }}
    move {{
        bind "{primary_modifier} m" {{ SwitchToMode "Normal"; }}
        bind "n" "Tab" {{ MovePane; }}
        bind "p" {{ MovePaneBackwards; }}
        bind "h" "Left" {{ MovePane "Left"; }}
        bind "j" "Down" {{ MovePane "Down"; }}
        bind "k" "Up" {{ MovePane "Up"; }}
        bind "l" "Right" {{ MovePane "Right"; }}
    }}
    tab {{
        bind "{primary_modifier} t" {{ SwitchToMode "Normal"; }}
        bind "r" {{ SwitchToMode "RenameTab"; TabNameInput 0; }}
        bind "h" "Left" "Up" "k" {{ GoToPreviousTab; }}
        bind "l" "Right" "Down" "j" {{ GoToNextTab; }}
        bind "n" {{ NewTab; SwitchToMode "Normal"; }}
        bind "x" {{ CloseTab; SwitchToMode "Normal"; }}
        bind "s" {{ ToggleActiveSyncTab; SwitchToMode "Normal"; }}
        bind "b" {{ BreakPane; SwitchToMode "Normal"; }}
        bind "]" {{ BreakPaneRight; SwitchToMode "Normal"; }}
        bind "[" {{ BreakPaneLeft; SwitchToMode "Normal"; }}
        bind "1" {{ GoToTab 1; SwitchToMode "Normal"; }}
        bind "2" {{ GoToTab 2; SwitchToMode "Normal"; }}
        bind "3" {{ GoToTab 3; SwitchToMode "Normal"; }}
        bind "4" {{ GoToTab 4; SwitchToMode "Normal"; }}
        bind "5" {{ GoToTab 5; SwitchToMode "Normal"; }}
        bind "6" {{ GoToTab 6; SwitchToMode "Normal"; }}
        bind "7" {{ GoToTab 7; SwitchToMode "Normal"; }}
        bind "8" {{ GoToTab 8; SwitchToMode "Normal"; }}
        bind "9" {{ GoToTab 9; SwitchToMode "Normal"; }}
        bind "Tab" {{ ToggleTab; }}
    }}
    scroll {{
        bind "{primary_modifier} s" {{ SwitchToMode "Normal"; }}
        bind "e" {{ EditScrollback; SwitchToMode "Normal"; }}
        bind "s" {{ SwitchToMode "EnterSearch"; SearchInput 0; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "{secondary_modifier} left" {{ MoveFocusOrTab "left"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} down" {{ MoveFocus "down"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} up" {{ MoveFocus "up"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} right" {{ MoveFocusOrTab "right"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} h" {{ MoveFocusOrTab "left"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} j" {{ MoveFocus "down"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} k" {{ MoveFocus "up"; SwitchToMode "normal"; }}
        bind "{secondary_modifier} l" {{ MoveFocusOrTab "right"; SwitchToMode "normal"; }}
    }}
    search {{
        bind "{primary_modifier} s" {{ SwitchToMode "Normal"; }}
        bind "Ctrl c" {{ ScrollToBottom; SwitchToMode "Normal"; }}
        bind "j" "Down" {{ ScrollDown; }}
        bind "k" "Up" {{ ScrollUp; }}
        bind "Ctrl f" "PageDown" "Right" "l" {{ PageScrollDown; }}
        bind "Ctrl b" "PageUp" "Left" "h" {{ PageScrollUp; }}
        bind "d" {{ HalfPageScrollDown; }}
        bind "u" {{ HalfPageScrollUp; }}
        bind "n" {{ Search "down"; }}
        bind "p" {{ Search "up"; }}
        bind "c" {{ SearchToggleOption "CaseSensitivity"; }}
        bind "w" {{ SearchToggleOption "Wrap"; }}
        bind "o" {{ SearchToggleOption "WholeWord"; }}
    }}
    entersearch {{
        bind "Ctrl c" "Esc" {{ SwitchToMode "Scroll"; }}
        bind "Enter" {{ SwitchToMode "Search"; }}
    }}
    renametab {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenameTab; SwitchToMode "Tab"; }}
    }}
    renamepane {{
        bind "Ctrl c" {{ SwitchToMode "Normal"; }}
        bind "Esc" {{ UndoRenamePane; SwitchToMode "Pane"; }}
    }}
    session {{
        bind "{primary_modifier} o" {{ SwitchToMode "Normal"; }}
        bind "{primary_modifier} s" {{ SwitchToMode "Scroll"; }}
        bind "d" {{ Detach; }}
        bind "w" {{
            LaunchOrFocusPlugin "session-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "c" {{
            LaunchOrFocusPlugin "configuration" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "p" {{
            LaunchOrFocusPlugin "plugin-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "a" {{
            LaunchOrFocusPlugin "zellij:about" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "s" {{
            LaunchOrFocusPlugin "zellij:share" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
        bind "l" {{
            LaunchOrFocusPlugin "zellij:layout-manager" {{
                floating true
                move_to_focused_tab true
            }};
            SwitchToMode "Normal"
        }}
    }}
    tmux {{
        bind "[" {{ SwitchToMode "Scroll"; }}
        bind "{primary_modifier} b" {{ Write 2; SwitchToMode "Normal"; }}
        bind "\"" {{ NewPane "Down"; SwitchToMode "Normal"; }}
        bind "%" {{ NewPane "Right"; SwitchToMode "Normal"; }}
        bind "z" {{ ToggleFocusFullscreen; SwitchToMode "Normal"; }}
        bind "c" {{ NewTab; SwitchToMode "Normal"; }}
        bind "," {{ SwitchToMode "RenameTab"; }}
        bind "p" {{ GoToPreviousTab; SwitchToMode "Normal"; }}
        bind "n" {{ GoToNextTab; SwitchToMode "Normal"; }}
        bind "Left" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "Right" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "Down" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "Up" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "h" {{ MoveFocus "Left"; SwitchToMode "Normal"; }}
        bind "l" {{ MoveFocus "Right"; SwitchToMode "Normal"; }}
        bind "j" {{ MoveFocus "Down"; SwitchToMode "Normal"; }}
        bind "k" {{ MoveFocus "Up"; SwitchToMode "Normal"; }}
        bind "o" {{ FocusNextPane; }}
        bind "d" {{ Detach; }}
        bind "Space" {{ NextSwapLayout; }}
        bind "x" {{ CloseFocus; SwitchToMode "Normal"; }}
    }}
    shared_except "locked" {{
        bind "{primary_modifier} g" {{ SwitchToMode "Locked"; }}
        bind "{primary_modifier} q" {{ Quit; }}
        bind "{secondary_modifier} f" {{ ToggleFloatingPanes; }}
        bind "{secondary_modifier} n" {{ NewPane; }}
        bind "{secondary_modifier} i" {{ MoveTab "Left"; }}
        bind "{secondary_modifier} o" {{ MoveTab "Right"; }}
        bind "{secondary_modifier} h" "{secondary_modifier} Left" {{ MoveFocusOrTab "Left"; }}
        bind "{secondary_modifier} l" "{secondary_modifier} Right" {{ MoveFocusOrTab "Right"; }}
        bind "{secondary_modifier} j" "{secondary_modifier} Down" {{ MoveFocus "Down"; }}
        bind "{secondary_modifier} k" "{secondary_modifier} Up" {{ MoveFocus "Up"; }}
        bind "{secondary_modifier} =" "{secondary_modifier} +" {{ Resize "Increase"; }}
        bind "{secondary_modifier} -" {{ Resize "Decrease"; }}
        bind "{secondary_modifier} [" {{ PreviousSwapLayout; }}
        bind "{secondary_modifier} ]" {{ NextSwapLayout; }}
        bind "{secondary_modifier} p" {{ TogglePaneInGroup; }}
        bind "{secondary_modifier} Shift p" {{ ToggleGroupMarking; }}
    }}
    shared_except "normal" "locked" {{
        bind "Enter" "Esc" {{ SwitchToMode "Normal"; }}
    }}
    shared_except "pane" "locked" {{
        bind "{primary_modifier} p" {{ SwitchToMode "Pane"; }}
    }}
    shared_except "resize" "locked" {{
        bind "{primary_modifier} r" {{ SwitchToMode "Resize"; }}
    }}
    shared_except "scroll" "locked" {{
        bind "{primary_modifier} s" {{ SwitchToMode "Scroll"; }}
    }}
    shared_except "session" "locked" "tab" {{
        bind "{primary_modifier} o" {{ SwitchToMode "Session"; }}
    }}
    shared_except "tab" "locked" {{
        bind "{primary_modifier} t" {{ SwitchToMode "Tab"; }}
    }}
    shared_except "move" "locked" {{
        bind "{primary_modifier} m" {{ SwitchToMode "Move"; }}
    }}
    shared_except "tmux" "locked" {{
        bind "{primary_modifier} b" {{ SwitchToMode "Tmux"; }}
    }}
}}
"#
    )
}
