use std::collections::HashMap;

use lazy_static::lazy_static;

use crate::tip::TipBody;

mod compact_layout;
mod edit_scrollbuffer;
mod floating_panes_mouse;
mod move_focus_hjkl_tab_switch;
mod move_tabs;
mod quicknav;
mod send_mouse_click_to_terminal;
mod sync_tab;
mod use_mouse;
mod zellij_setup_check;

lazy_static! {
    pub static ref TIPS: HashMap<&'static str, TipBody> = HashMap::from([
        (
            "quicknav",
            TipBody {
                short: quicknav::quicknav_short,
                medium: quicknav::quicknav_medium,
                full: quicknav::quicknav_full,
            }
        ),
        (
            "floating_panes_mouse",
            TipBody {
                short: floating_panes_mouse::floating_panes_mouse_short,
                medium: floating_panes_mouse::floating_panes_mouse_medium,
                full: floating_panes_mouse::floating_panes_mouse_full,
            }
        ),
        (
            "send_mouse_clicks_to_terminal",
            TipBody {
                short: send_mouse_click_to_terminal::mouse_click_to_terminal_short,
                medium: send_mouse_click_to_terminal::mouse_click_to_terminal_medium,
                full: send_mouse_click_to_terminal::mouse_click_to_terminal_full,
            }
        ),
        (
            "move_focus_hjkl_tab_switch",
            TipBody {
                short: move_focus_hjkl_tab_switch::move_focus_hjkl_tab_switch_short,
                medium: move_focus_hjkl_tab_switch::move_focus_hjkl_tab_switch_medium,
                full: move_focus_hjkl_tab_switch::move_focus_hjkl_tab_switch_full,
            }
        ),
        (
            "zellij_setup_check",
            TipBody {
                short: zellij_setup_check::zellij_setup_check_short,
                medium: zellij_setup_check::zellij_setup_check_medium,
                full: zellij_setup_check::zellij_setup_check_full,
            }
        ),
        (
            "use_mouse",
            TipBody {
                short: use_mouse::use_mouse_short,
                medium: use_mouse::use_mouse_medium,
                full: use_mouse::use_mouse_full,
            }
        ),
        (
            "sync_tab",
            TipBody {
                short: sync_tab::sync_tab_short,
                medium: sync_tab::sync_tab_medium,
                full: sync_tab::sync_tab_full,
            }
        ),
        (
            "edit_scrollbuffer",
            TipBody {
                short: edit_scrollbuffer::edit_scrollbuffer_short,
                medium: edit_scrollbuffer::edit_scrollbuffer_medium,
                full: edit_scrollbuffer::edit_scrollbuffer_full,
            }
        ),
        (
            "compact_layout",
            TipBody {
                short: compact_layout::compact_layout_short,
                medium: compact_layout::compact_layout_medium,
                full: compact_layout::compact_layout_full,
            }
        ),
        (
            "move_tabs",
            TipBody {
                short: move_tabs::move_tabs_short,
                medium: move_tabs::move_tabs_medium,
                full: move_tabs::move_tabs_full,
            }
        )
    ]);
}
