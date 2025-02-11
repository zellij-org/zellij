use zellij_tile::prelude::*;

use std::cell::RefCell;
use std::rc::Rc;

use crate::active_component::{ActiveComponent, ClickAction};

#[derive(Debug)]
pub struct Page {
    title: Option<Text>,
    components_to_render: Vec<RenderedComponent>,
    has_hover: bool,
    hovering_over_link: bool,
    menu_item_is_selected: bool,
    pub is_main_screen: bool,
}

// tips (TODO: probably move elsewhere?)

fn screencasts_link_selected() -> Box<dyn Fn(usize, usize) -> usize> {
    Box::new(move |x, y| {
        print!(
            "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://zellij.dev/screencasts",
            y + 1,
            x + 1,
        );
        30
    })
}

fn screencasts_link_selected_len() -> Box<dyn Fn() -> usize> {
    Box::new(move || 30)
}

fn tips_help_text(hovering_over_link: bool) -> Text {
    if hovering_over_link {
        let help_text = format!("Help: Click or Shift-Click to open in browser");
        Text::new(help_text)
            .color_range(3, 6..=10)
            .color_range(3, 15..=25)
    } else {
        let help_text = format!("Help: <ESC> - Dismiss, <↓↑> - Browse tips, <Ctrl c> - Don't show tips on startup");
        Text::new(help_text)
            .color_range(1, 6..=10)
            .color_range(1, 23..=26)
            .color_range(1, 43..=50)
    }
}

impl Page {
    pub fn new_tip_screen(
        link_executable: Rc<RefCell<String>>,
        zellij_version: String,
        base_mode: Rc<RefCell<InputMode>>,
        tip_index: usize,
    ) -> Self {
        if tip_index == 0 {
            Page::tip_1(link_executable)
        } else if tip_index == 1 {
            Page::tip_2(link_executable, base_mode)
        } else if tip_index == 2 {
            Page::tip_3(link_executable)
        } else if tip_index == 3 {
            Page::tip_4(link_executable, base_mode)
        } else if tip_index == 4 {
            Page::tip_5(link_executable)
        } else if tip_index == 5 {
            Page::tip_6(link_executable, base_mode)
        } else if tip_index == 6 {
            Page::tip_7(link_executable, base_mode)
        } else if tip_index == 7 {
            Page::tip_8(link_executable, base_mode)
        } else if tip_index == 8 {
            Page::tip_9(link_executable, base_mode)
        } else if tip_index == 9 {
            Page::tip_10(link_executable, base_mode)
        } else if tip_index == 10 {
            Page::tip_11(link_executable, base_mode)
        } else if tip_index == 11 {
            Page::tip_12(link_executable, base_mode)
        } else {
            Page::tip_1(link_executable)
        }
    }
    pub fn tip_1(
        link_executable: Rc<RefCell<String>>,
    ) -> Self {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij Tip #1").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Check out the Zellij screencasts/tutorials to learn how to better take advantage")
                    ))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("of all the Zellij features. Learn about basic usage, layouts, sessions and more!")
                    ))
                ])
            ])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(Text::new("Follow this link: ").color_range(2, ..))),
                ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://zellij.dev/screencasts")))
                .with_hover(TextOrCustomRender::CustomRender(
                    Box::new(screencasts_link_selected()),
                    Box::new(screencasts_link_selected_len()),
                ))
                .with_left_click_action(ClickAction::new_open_link(
                    format!("https://zellij.dev/screencasts"),
                    link_executable.clone(),
                )),
            ])])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(sponsors_link_text_selected),
                        Box::new(sponsors_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://github.com/sponsors/imsnif".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_help(Box::new(|hovering_over_link, _menu_item_is_selected| {
                tips_help_text(hovering_over_link)
            }))
    }
    pub fn tip_2(
        link_executable: Rc<RefCell<String>>,
        base_mode: Rc<RefCell<InputMode>>,
    ) -> Self {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij Tip #2").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("You can open the terminal contents in your $EDITOR, allowing you to search")
                                .color_range(2, 43..=49)
                    ))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("through them, copy to your clipboard or even save them for later.")
                    ))
                ])
            ])
            .with_paragraph(vec![ComponentLine::new(vec![
                match *base_mode.borrow() {
                    InputMode::Locked => {
                        ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("While focused on a terminal pane: Ctrl g + s + e")
                                .color_range(0, 34..=39)
                                .color_indices(0, vec![43, 47])
                        ))
                    },
                    _ => {
                        ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("While focused on a terminal pane: Ctrl s + e")
                                .color_range(0, 34..=39)
                                .color_indices(0, vec![43])
                        ))
                    }
                }
            ])])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(sponsors_link_text_selected),
                        Box::new(sponsors_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://github.com/sponsors/imsnif".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_help(Box::new(|hovering_over_link, _menu_item_is_selected| {
                tips_help_text(hovering_over_link)
            }))
    }
    pub fn tip_3(
        link_executable: Rc<RefCell<String>>,
    ) -> Self {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij Tip #3").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Want to make your floating pane bigger?")
                    ))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("You can switch to the ENLARGED layout with Alt ] while focused on it.")
                            .color_range(2, 22..=29)
                            .color_range(0, 43..=47)
                    ))
                ])
            ])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(sponsors_link_text_selected),
                        Box::new(sponsors_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://github.com/sponsors/imsnif".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_help(Box::new(|hovering_over_link, _menu_item_is_selected| {
                tips_help_text(hovering_over_link)
            }))
    }
    fn tip_4(link_executable: Rc<RefCell<String>>, base_mode: Rc<RefCell<InputMode>>) -> Page {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij tip #4").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new(
                        "It's possible to \"pin\" a floating pane so that it will always",
                    ),
                ))]),
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new("be visible even if floating panes are hidden."),
                ))]),
            ])
            .with_bulletin_list(
                BulletinList::new(
                    Text::new(format!("Floating panes can be \"pinned\": ")).color_range(2, ..),
                )
                .with_items(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new(format!("With a mouse click on their top right corner"))
                            .color_range(3, 7..=17),
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(match *base_mode.borrow() {
                        InputMode::Locked => Text::new(format!("With Ctrl g + p + i"))
                            .color_range(3, 5..=10)
                            .color_range(3, 14..15)
                            .color_range(3, 18..19),
                        _ => Text::new("With Ctrl p + i")
                            .color_range(3, 5..=10)
                            .color_range(3, 14..15),
                    })),
                ]),
            )
            .with_paragraph(vec![
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new("A great use case for these is to tail log files or to show"),
                ))]),
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new(format!(
                        "real-time compiler output while working in other panes."
                    )),
                ))]),
            ])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(sponsors_link_text_selected),
                        Box::new(sponsors_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://github.com/sponsors/imsnif".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_help(Box::new(|hovering_over_link, _menu_item_is_selected| {
                tips_help_text(hovering_over_link)
            }))
    }
    pub fn tip_5(link_executable: Rc<RefCell<String>>) -> Page {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij Tip #5").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("Panes can be resized into stacks to be managed easier."))),
                ]),
            ])
            .with_bulletin_list(BulletinList::new(Text::new("To try it out:").color_range(2, ..))
                .with_items(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Hide this pane with Alt f (you can bring it back with Alt f again)")
                                .color_range(3, 20..=24)
                                .color_range(3, 54..=58)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Open 4-5 panes with Alt n")
                                .color_range(3, 20..=24)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Press Alt + until you reach full screen")
                                .color_range(3, 6..=10)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Press Alt - until you are back at the original state")
                                .color_range(3, 6..=10)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("You can always snap back to the built-in swap layouts with Alt <[]>")
                                .color_range(3, 59..=61)
                                .color_range(3, 64..=65)
                    )),
                ])
            )
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("To disable this behavior, add stacked_resize false to the Zellij Configuration")
                                .color_range(3, 30..=49)
                    )),
                ])
            ])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("For more details, see: ")
                            .color_range(2, ..)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://zellij.dev/screencasts/stacked-resize")))
                        .with_hover(TextOrCustomRender::CustomRender(Box::new(stacked_resize_screencast_link_selected), Box::new(stacked_resize_screencast_link_selected_len)))
                        .with_left_click_action(ClickAction::new_open_link("https://zellij.dev/screencasts/stacked-resize".to_owned(), link_executable.clone()))
                ])
            ])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(sponsors_link_text_selected),
                        Box::new(sponsors_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://github.com/sponsors/imsnif".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_help(Box::new(|hovering_over_link, _menu_item_is_selected| {
                tips_help_text(hovering_over_link)
            }))
    }
    pub fn tip_6(link_executable: Rc<RefCell<String>>, base_mode: Rc<RefCell<InputMode>>) -> Page {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij Tip #6").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("Are the Zellij keybindings colliding with other applications for you?")))
                ]),
            ])
            .with_bulletin_list(BulletinList::new(Text::new("Check out the non-colliding keybindings preset:"))
                .with_items(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            match *base_mode.borrow() {
                                InputMode::Locked => {
                                    Text::new("Open the Zellij configuration with Ctrl g + o + c")
                                        .color_range(3, 35..=40)
                                        .color_indices(3, vec![44, 48])
                                },
                                _ => {
                                    Text::new("Open the Zellij configuration with Ctrl o + c")
                                        .color_range(3, 35..=40)
                                        .color_indices(3, vec![44])
                                }
                            }
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Press TAB to go to Chagne Mode Behavior")
                                .color_range(3, 6..=9)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Select non-colliding temporarily with ENTER or permanently with Ctrl a")
                                .color_range(3, 38..=42)
                                .color_range(3, 64..=69)
                    )),
                ])
            )
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("For more details, see: ")
                            .color_range(2, ..)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://zellij.dev/tutorials/colliding-keybindings")))
                        .with_hover(TextOrCustomRender::CustomRender(Box::new(colliding_keybindings_link_selected), Box::new(colliding_keybindings_link_selected_len)))
                        .with_left_click_action(ClickAction::new_open_link("https://zellij.dev/tutorials/colliding-keybindings".to_owned(), link_executable.clone()))
                ])
            ])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(sponsors_link_text_selected),
                        Box::new(sponsors_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://github.com/sponsors/imsnif".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_help(Box::new(|hovering_over_link, _menu_item_is_selected| {
                tips_help_text(hovering_over_link)
            }))
    }
    pub fn tip_7(link_executable: Rc<RefCell<String>>, base_mode: Rc<RefCell<InputMode>>) -> Page {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij Tip #7").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("Want to customize the appearance and colors of Zellij?")))
                ]),
            ])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Check out the built-in themes: ")
                            .color_range(2, ..)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://zellij.dev/documentation/theme-list")))
                        .with_hover(TextOrCustomRender::CustomRender(Box::new(theme_list_selected), Box::new(theme_list_selected_len)))
                        .with_left_click_action(ClickAction::new_open_link("https://zellij.dev/documentation/theme-list".to_owned(), link_executable.clone()))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Or create your own theme: ")
                            .color_range(2, ..)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://zellij.dev/documentation/themes")))
                        .with_hover(TextOrCustomRender::CustomRender(Box::new(theme_link_selected), Box::new(theme_link_selected_len)))
                        .with_left_click_action(ClickAction::new_open_link("https://zellij.dev/documentation/themes".to_owned(), link_executable.clone()))
                ])
            ])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(sponsors_link_text_selected),
                        Box::new(sponsors_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://github.com/sponsors/imsnif".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_help(Box::new(|hovering_over_link, _menu_item_is_selected| {
                tips_help_text(hovering_over_link)
            }))
    }
    pub fn tip_8(link_executable: Rc<RefCell<String>>, base_mode: Rc<RefCell<InputMode>>) -> Page {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij Tip #8").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("If you change the pane focus with Alt + <←↓↑→> or Alt + <hjkl> beyond the")
                            .color_range(0, 34..=36)
                            .color_range(2, 40..=45)
                            .color_range(0, 50..=52)
                            .color_range(2, 56..=60)
                    ))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("right or left side of the screen, the next or previous tab will be focused.")))
                ]),
            ])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(sponsors_link_text_selected),
                        Box::new(sponsors_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://github.com/sponsors/imsnif".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_help(Box::new(|hovering_over_link, _menu_item_is_selected| {
                tips_help_text(hovering_over_link)
            }))
    }
    pub fn tip_9(link_executable: Rc<RefCell<String>>, base_mode: Rc<RefCell<InputMode>>) -> Page {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij Tip #9").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("For plugins, integrations and tutorials created by the community, check out the")
                    ))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Awesome-zellij repository: ")
                            .color_range(2, ..=39)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://github.com/zellij-org/awesome-zellij")))
                        .with_hover(TextOrCustomRender::CustomRender(
                            Box::new(awesome_zellij_link_text_selected),
                            Box::new(awesome_zellij_link_text_selected_len),
                        ))
                        .with_left_click_action(ClickAction::new_open_link(
                            "https://github.com/zellij-org/awesome-zellij".to_owned(),
                            link_executable.clone(),
                        )),
                ]),
            ])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("For community and support:")
                            .color_range(2, ..)
                    ))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("Discord: "))),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://discord.com/invite/CrUAFH3")))
                        .with_hover(TextOrCustomRender::CustomRender(
                            Box::new(discord_link_text_selected),
                            Box::new(discord_link_text_selected_len),
                        ))
                        .with_left_click_action(ClickAction::new_open_link(
                            "https://discord.com/invite/CrUAFH3".to_owned(),
                            link_executable.clone(),
                        )),
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("Matrix: "))),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://matrix.to/#/#zellij_general:matrix.org")))
                        .with_hover(TextOrCustomRender::CustomRender(
                            Box::new(matrix_link_text_selected),
                            Box::new(matrix_link_text_selected_len),
                        ))
                        .with_left_click_action(ClickAction::new_open_link(
                            "https://matrix.to/#/#zellij_general:matrix.org".to_owned(),
                            link_executable.clone(),
                        )),
                ])
            ])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(sponsors_link_text_selected),
                        Box::new(sponsors_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://github.com/sponsors/imsnif".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_help(Box::new(|hovering_over_link, _menu_item_is_selected| {
                tips_help_text(hovering_over_link)
            }))
    }
    pub fn tip_10(link_executable: Rc<RefCell<String>>, base_mode: Rc<RefCell<InputMode>>) -> Page {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij Tip #10").color_range(0, ..))
            .with_bulletin_list(BulletinList::new(
                Text::new("The Zellij session-manager can:")
                    .color_range(2, 11..=25)
                )
                .with_items(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Create new sessions")
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Switch between existing sessions")
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Resurrect exited sessions")
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Change the session name")
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Disconnect other users from the current session")
                    )),
                ])
            )
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            match *base_mode.borrow() {
                                InputMode::Locked => {
                                    Text::new("Check it out with with: Ctrl g + o + w")
                                        .color_range(3, 24..=29)
                                        .color_indices(3, vec![33, 37])
                                },
                                _ => {
                                    Text::new("Check it out with with: Ctrl o + w")
                                        .color_range(3, 24..=29)
                                        .color_indices(3, vec![33])
                                }
                            }
                    )),
                ])
            ])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("You can also use it as a welcome screen with: zellij -l welcome")
                                .color_range(0, 46..=62)
                    )),
                ])
            ])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(sponsors_link_text_selected),
                        Box::new(sponsors_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://github.com/sponsors/imsnif".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_help(Box::new(|hovering_over_link, _menu_item_is_selected| {
                tips_help_text(hovering_over_link)
            }))
    }
    pub fn tip_11(link_executable: Rc<RefCell<String>>, base_mode: Rc<RefCell<InputMode>>) -> Page {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij Tip #11").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("You can change the arrangement of panes on screen with Alt + []")
                            .color_range(0, 55..=57)
                            .color_range(2, 61..=62)
                    )),
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("This works with tiled or floating panes, depending which is visible.")
                    ))
                ])
            ])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Resizing or splitting a pane breaks out of this arrangement. It is then possible")
                    )),
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("to snap back by pressing Alt + [] once more. This status can be seen")
                            .color_range(0, 25..=27)
                            .color_range(2, 31..=32)
                    )),
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("on the top right corner of the screen.")
                    )),
                ]),
            ])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(sponsors_link_text_selected),
                        Box::new(sponsors_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://github.com/sponsors/imsnif".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_help(Box::new(|hovering_over_link, _menu_item_is_selected| {
                tips_help_text(hovering_over_link)
            }))
    }
    pub fn tip_12(link_executable: Rc<RefCell<String>>, base_mode: Rc<RefCell<InputMode>>) -> Page {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij Tip #12").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Zellij plugins can be loaded, reloaded and tracked from the plugin-manager.")
                    )),
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            match *base_mode.borrow() {
                                InputMode::Locked => {
                                    Text::new("Check it out with with: Ctrl g + o + p")
                                        .color_range(3, 24..=29)
                                        .color_indices(3, vec![33, 37])
                                },
                                _ => {
                                    Text::new("Check it out with with: Ctrl o + p")
                                        .color_range(3, 24..=29)
                                        .color_indices(3, vec![33])
                                }
                            }
                    )),
                ]),
            ])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(Text::new("To learn more about plugins: ").color_range(2, ..))),
                ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://zellij.dev/documentation/plugins")))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(plugin_docs_link_text_selected),
                        Box::new(plugin_docs_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://zellij.dev/documentation/plugins".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(sponsors_link_text_selected),
                        Box::new(sponsors_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://github.com/sponsors/imsnif".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_help(Box::new(|hovering_over_link, _menu_item_is_selected| {
                tips_help_text(hovering_over_link)
            }))
    }
}

impl Page {
    pub fn new_main_screen(
        link_executable: Rc<RefCell<String>>,
        zellij_version: String,
        base_mode: Rc<RefCell<InputMode>>,
    ) -> Self {
        Page::new()
            .main_screen()
            .with_title(main_screen_title(zellij_version.clone()))
            .with_bulletin_list(BulletinList::new(whats_new_title()).with_items(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(main_menu_item(
                        "Stacked Resize",
                    )))
                    .with_hover(TextOrCustomRender::Text(
                        main_menu_item("Stacked Resize").selected(),
                    ))
                    .with_left_click_action(ClickAction::new_change_page({
                        let link_executable = link_executable.clone();
                        move || Page::new_stacked_resize(link_executable.clone())
                    })),
                    ActiveComponent::new(TextOrCustomRender::Text(main_menu_item(
                        "Pinned Floating Panes",
                    )))
                    .with_hover(TextOrCustomRender::Text(
                        main_menu_item("Pinned Floating Panes").selected(),
                    ))
                    .with_left_click_action(ClickAction::new_change_page(move || {
                        Page::new_pinned_panes(base_mode.clone())
                    })),
                    ActiveComponent::new(TextOrCustomRender::Text(main_menu_item(
                        "New Theme Definition Spec",
                    )))
                    .with_hover(TextOrCustomRender::Text(
                        main_menu_item("New Theme Definition Spec").selected(),
                    ))
                    .with_left_click_action(ClickAction::new_change_page({
                        let link_executable = link_executable.clone();
                        move || Page::new_theme_definition_spec(link_executable.clone())
                    })),
                    ActiveComponent::new(TextOrCustomRender::Text(main_menu_item(
                        "New Plugin APIs",
                    )))
                    .with_hover(TextOrCustomRender::Text(
                        main_menu_item("New Plugin APIs").selected(),
                    ))
                    .with_left_click_action(ClickAction::new_change_page(move || {
                        Page::new_plugin_apis()
                    })),
                    ActiveComponent::new(TextOrCustomRender::Text(main_menu_item(
                        "Mouse Any-Event Handling",
                    )))
                    .with_hover(TextOrCustomRender::Text(
                        main_menu_item("Mouse Any-Event Handling").selected(),
                    ))
                    .with_left_click_action(ClickAction::new_change_page({
                        move || Page::new_mouse_any_event()
                    })),
                ]))
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(Text::new("Full Changelog: "))),
                ActiveComponent::new(TextOrCustomRender::Text(changelog_link_unselected(
                    zellij_version.clone(),
                )))
                .with_hover(TextOrCustomRender::CustomRender(
                    Box::new(changelog_link_selected(zellij_version.clone())),
                    Box::new(changelog_link_selected_len(zellij_version.clone())),
                ))
                .with_left_click_action(ClickAction::new_open_link(
                    format!(
                        "https://github.com/zellij-org/zellij/releases/tag/v{}",
                        zellij_version.clone()
                    ),
                    link_executable.clone(),
                )),
            ])])
            .with_paragraph(vec![ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(sponsors_link_text_selected),
                        Box::new(sponsors_link_text_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://github.com/sponsors/imsnif".to_owned(),
                        link_executable.clone(),
                    )),
            ])])
            .with_help(Box::new(|hovering_over_link, menu_item_is_selected| {
                main_screen_help_text(hovering_over_link, menu_item_is_selected)
            }))
    }
    pub fn new_stacked_resize(link_executable: Rc<RefCell<String>>) -> Page {
        Page::new()
            .with_title(Text::new("Stacked Resize").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("This version includes a new resizing algorithm that helps better manage panes"))),
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("into stacks."))),
                ]),
            ])
            .with_bulletin_list(BulletinList::new(Text::new("To try it out:").color_range(2, ..))
                .with_items(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Hide this pane with Alt f (you can bring it back with Alt f again)")
                                .color_range(3, 20..=24)
                                .color_range(3, 54..=58)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Open 4-5 panes with Alt n")
                                .color_range(3, 20..=24)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Press Alt + until you reach full screen")
                                .color_range(3, 6..=10)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Press Alt - until you are back at the original state")
                                .color_range(3, 6..=10)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("5. You can always snap back to the built-in swap layouts with Alt <[]>")
                                .color_range(3, 62..=64)
                                .color_range(3, 67..=68)
                    )),
                ])
            )
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("To disable, add stacked_resize false to the Zellij Configuration")
                                .color_range(3, 16..=35)
                    )),
                ])
            ])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("For more details, see: ")
                            .color_range(2, ..)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://zellij.dev/screencasts/stacked-resize")))
                        .with_hover(TextOrCustomRender::CustomRender(Box::new(stacked_resize_screencast_link_selected), Box::new(stacked_resize_screencast_link_selected_len)))
                        .with_left_click_action(ClickAction::new_open_link("https://zellij.dev/screencasts/stacked-resize".to_owned(), link_executable.clone()))
                ])
            ])
            .with_help(Box::new(|hovering_over_link, menu_item_is_selected| esc_go_back_plus_link_hover(hovering_over_link, menu_item_is_selected)))
    }
    fn new_pinned_panes(base_mode: Rc<RefCell<InputMode>>) -> Page {
        Page::new()
            .with_title(Text::new("Pinned Floating Panes").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new(
                        "This version adds the ability to \"pin\" a floating pane so that it",
                    ),
                ))]),
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new("will always be visible even if floating panes are hidden."),
                ))]),
            ])
            .with_bulletin_list(
                BulletinList::new(
                    Text::new(format!("Floating panes can be \"pinned\": ")).color_range(2, ..),
                )
                .with_items(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new(format!("With a mouse click on their top right corner"))
                            .color_range(3, 7..=17),
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(match *base_mode.borrow() {
                        InputMode::Locked => Text::new(format!("With Ctrl g + p + i"))
                            .color_range(3, 5..=10)
                            .color_range(3, 14..15)
                            .color_range(3, 18..19),
                        _ => Text::new("With Ctrl p + i")
                            .color_range(3, 5..=10)
                            .color_range(3, 14..15),
                    })),
                ]),
            )
            .with_paragraph(vec![
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new("A great use case for these is to tail log files or to show"),
                ))]),
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new(format!(
                        "real-time compiler output while working in other panes."
                    )),
                ))]),
            ])
            .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| {
                esc_to_go_back_help()
            }))
    }
    fn new_theme_definition_spec(link_executable: Rc<RefCell<String>>) -> Page {
        Page::new()
            .with_title(Text::new("New Theme Definition Spec").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Starting this version, themes can be defined by UI components")
                            .color_range(3, 37..=60)
                    ))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("instead of the previously obscure color-to-color definitions.")
                    ))
                ]),
            ])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("This both improves the convenience of theme creation and allows greater freedom")
                    ))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("for theme authors.")
                    ))
                ]),
            ])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("For more information: ")
                            .color_range(2, ..)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://zellij.dev/documentation/themes")))
                        .with_hover(TextOrCustomRender::CustomRender(Box::new(theme_link_selected), Box::new(theme_link_selected_len)))
                        .with_left_click_action(ClickAction::new_open_link("https://zellij.dev/documentation/themes".to_owned(), link_executable.clone()))
                ])
            ])
            .with_help(Box::new(|hovering_over_link, menu_item_is_selected| esc_go_back_plus_link_hover(hovering_over_link, menu_item_is_selected)))
    }
    fn new_plugin_apis() -> Page {
        Page::new()
            .with_title(Text::new("New Plugin APIs").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new("New APIs were added in this version affording plugins"),
                ))]),
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new("finer control over the workspace."),
                ))]),
            ])
            .with_bulletin_list(
                BulletinList::new(Text::new("Some examples:").color_range(2, ..)).with_items(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Change floating panes' coordinates and size")
                            .color_range(3, 23..=33)
                            .color_range(3, 39..=42),
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Stack arbitrary panes").color_range(3, ..=4),
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Change /host folder").color_range(3, 7..=11),
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Discover the user's $SHELL and $EDITOR")
                            .color_range(3, 20..=25)
                            .color_range(3, 31..=37),
                    )),
                ]),
            )
            .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| {
                esc_to_go_back_help()
            }))
    }
    fn new_mouse_any_event() -> Page {
        Page::new()
            .with_title(Text::new("Mosue Any-Event Tracking").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new(
                        "This version adds the capability to track mouse motions more accurately",
                    ),
                ))]),
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new("both in Zellij, in terminal panes and in plugin panes."),
                ))]),
            ])
            .with_paragraph(vec![ComponentLine::new(vec![ActiveComponent::new(
                TextOrCustomRender::Text(Text::new(
                    "Future versions will also build on this capability to improve the Zellij UI",
                )),
            )])])
            .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| {
                esc_to_go_back_help()
            }))
    }
}

impl Page {
    pub fn new() -> Self {
        Page {
            title: None,
            components_to_render: vec![],
            has_hover: false,
            hovering_over_link: false,
            menu_item_is_selected: false,
            is_main_screen: false,
        }
    }
    pub fn main_screen(mut self) -> Self {
        self.is_main_screen = true;
        self
    }
    pub fn with_title(mut self, title: Text) -> Self {
        self.title = Some(title);
        self
    }
    pub fn with_bulletin_list(mut self, bulletin_list: BulletinList) -> Self {
        self.components_to_render
            .push(RenderedComponent::BulletinList(bulletin_list));
        self
    }
    pub fn with_paragraph(mut self, paragraph: Vec<ComponentLine>) -> Self {
        self.components_to_render
            .push(RenderedComponent::Paragraph(paragraph));
        self
    }
    pub fn with_help(mut self, help_text_fn: Box<dyn Fn(bool, bool) -> Text>) -> Self {
        self.components_to_render
            .push(RenderedComponent::HelpText(help_text_fn));
        self
    }
    pub fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        if key.bare_key == BareKey::Down && key.has_no_modifiers() {
            self.move_selection_down();
            should_render = true;
        } else if key.bare_key == BareKey::Up && key.has_no_modifiers() {
            self.move_selection_up();
            should_render = true;
        }
        should_render
    }
    pub fn handle_mouse_left_click(&mut self, x: usize, y: usize) -> Option<Page> {
        for rendered_component in &mut self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    let page_to_render = bulletin_list.handle_left_click_at_position(x, y);
                    if page_to_render.is_some() {
                        return page_to_render;
                    }
                },
                RenderedComponent::Paragraph(paragraph) => {
                    for component_line in paragraph {
                        let page_to_render = component_line.handle_left_click_at_position(x, y);
                        if page_to_render.is_some() {
                            return page_to_render;
                        }
                    }
                },
                _ => {},
            }
        }
        None
    }
    pub fn handle_selection(&mut self) -> Option<Page> {
        for rendered_component in &mut self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    let page_to_render = bulletin_list.handle_selection();
                    if page_to_render.is_some() {
                        return page_to_render;
                    }
                },
                _ => {},
            }
        }
        None
    }
    pub fn handle_mouse_hover(&mut self, x: usize, y: usize) -> bool {
        let hover_cleared = self.clear_hover(); // TODO: do the right thing if the same component was hovered from
                                                // previous motion
        for rendered_component in &mut self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    let should_render = bulletin_list.handle_hover_at_position(x, y);
                    if should_render {
                        self.has_hover = true;
                        self.menu_item_is_selected = true;
                        return should_render;
                    }
                },
                RenderedComponent::Paragraph(paragraph) => {
                    for component_line in paragraph {
                        let should_render = component_line.handle_hover_at_position(x, y);
                        if should_render {
                            self.has_hover = true;
                            self.hovering_over_link = true;
                            return should_render;
                        }
                    }
                },
                _ => {},
            }
        }
        hover_cleared
    }
    fn move_selection_up(&mut self) {
        match self.position_of_active_bulletin() {
            Some(position_of_active_bulletin) if position_of_active_bulletin > 0 => {
                self.clear_active_bulletins();
                self.set_active_bulletin(position_of_active_bulletin.saturating_sub(1));
            },
            Some(0) => {
                self.clear_active_bulletins();
            },
            _ => {
                self.clear_active_bulletins();
                self.set_last_active_bulletin();
            },
        }
    }
    fn move_selection_down(&mut self) {
        match self.position_of_active_bulletin() {
            Some(position_of_active_bulletin) => {
                self.clear_active_bulletins();
                self.set_active_bulletin(position_of_active_bulletin + 1);
            },
            None => {
                self.set_active_bulletin(0);
            },
        }
    }
    fn position_of_active_bulletin(&self) -> Option<usize> {
        self.components_to_render.iter().find_map(|c| match c {
            RenderedComponent::BulletinList(bulletin_list) => {
                bulletin_list.active_component_position()
            },
            _ => None,
        })
    }
    fn clear_active_bulletins(&mut self) {
        self.components_to_render.iter_mut().for_each(|c| {
            match c {
                RenderedComponent::BulletinList(bulletin_list) => {
                    Some(bulletin_list.clear_active_bulletins())
                },
                _ => None,
            };
        });
    }
    fn set_active_bulletin(&mut self, active_bulletin_position: usize) {
        self.components_to_render.iter_mut().for_each(|c| {
            match c {
                RenderedComponent::BulletinList(bulletin_list) => {
                    bulletin_list.set_active_bulletin(active_bulletin_position)
                },
                _ => {},
            };
        });
    }
    fn set_last_active_bulletin(&mut self) {
        self.components_to_render.iter_mut().for_each(|c| {
            match c {
                RenderedComponent::BulletinList(bulletin_list) => {
                    bulletin_list.set_last_active_bulletin()
                },
                _ => {},
            };
        });
    }
    fn clear_hover(&mut self) -> bool {
        let had_hover = self.has_hover;
        self.menu_item_is_selected = false;
        self.hovering_over_link = false;
        for rendered_component in &mut self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    bulletin_list.clear_hover();
                },
                RenderedComponent::Paragraph(paragraph) => {
                    for active_component in paragraph {
                        active_component.clear_hover();
                    }
                },
                _ => {},
            }
        }
        self.has_hover = false;
        had_hover
    }
    pub fn ui_column_count(&mut self) -> usize {
        let mut column_count = 0;
        for rendered_component in &self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    column_count = std::cmp::max(column_count, bulletin_list.column_count());
                },
                RenderedComponent::Paragraph(paragraph) => {
                    for active_component in paragraph {
                        column_count = std::cmp::max(column_count, active_component.column_count());
                    }
                },
                RenderedComponent::HelpText(_text) => {}, // we ignore help text in column
                                                          // calculation because it's always left
                                                          // justified
            }
        }
        column_count
    }
    pub fn ui_row_count(&mut self) -> usize {
        let mut row_count = 0;
        if self.title.is_some() {
            row_count += 1;
        }
        for rendered_component in &self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    row_count += bulletin_list.len();
                },
                RenderedComponent::Paragraph(paragraph) => {
                    row_count += paragraph.len();
                },
                RenderedComponent::HelpText(_text) => {}, // we ignore help text as it is outside
                                                          // the UI container
            }
        }
        row_count += self.components_to_render.len();
        row_count
    }
    pub fn render(&mut self, rows: usize, columns: usize) {
        let base_x = columns.saturating_sub(self.ui_column_count()) / 2;
        let base_y = rows.saturating_sub(self.ui_row_count()) / 2;
        let mut current_y = base_y;
        if let Some(title) = &self.title {
            print_text_with_coordinates(
                title.clone(),
                base_x,
                current_y,
                Some(columns),
                Some(rows),
            );
            current_y += 2;
        }
        for rendered_component in &mut self.components_to_render {
            let is_help = match rendered_component {
                RenderedComponent::HelpText(_) => true,
                _ => false,
            };
            let y = if is_help { rows } else { current_y };
            let columns = if is_help { columns } else { columns.saturating_sub(base_x * 2) };
            let rendered_rows = rendered_component.render(
                base_x,
                y,
                rows,
                columns,
                self.hovering_over_link,
                self.menu_item_is_selected,
            );
            current_y += rendered_rows + 1; // 1 for the line space between components
        }
    }
}

fn changelog_link_unselected(version: String) -> Text {
    let full_changelog_text = format!(
        "https://github.com/zellij-org/zellij/releases/tag/v{}",
        version
    );
    Text::new(full_changelog_text)
}

fn changelog_link_selected(version: String) -> Box<dyn Fn(usize, usize) -> usize> {
    Box::new(move |x, y| {
        print!(
            "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/zellij-org/zellij/releases/tag/v{}",
            y + 1,
            x + 1,
            version
        );
        51 + version.chars().count()
    })
}

fn changelog_link_selected_len(version: String) -> Box<dyn Fn() -> usize> {
    Box::new(move || 51 + version.chars().count())
}

fn sponsors_link_text_unselected() -> Text {
    Text::new("https://github.com/sponsors/imsnif")
}

fn sponsors_link_text_selected(x: usize, y: usize) -> usize {
    print!(
        "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/sponsors/imsnif",
        y + 1,
        x + 1
    );
    34
}

fn sponsors_link_text_selected_len() -> usize {
    34
}

fn plugin_docs_link_text_selected(x: usize, y: usize) -> usize {
    print!(
        "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://zellij.dev/documentation/plugins",
        y + 1,
        x + 1
    );
    40
}

fn plugin_docs_link_text_selected_len() -> usize {
    40
}

fn awesome_zellij_link_text_selected(x: usize, y: usize) -> usize {
    print!(
        "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/zellij-org/awesome-zellij",
        y + 1,
        x + 1
    );
    44
}

fn awesome_zellij_link_text_selected_len() -> usize {
    44
}

fn discord_link_text_selected(x: usize, y: usize) -> usize {
    print!(
        "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://discord.com/invite/CrUAFH3",
        y + 1,
        x + 1
    );
    34
}

fn discord_link_text_selected_len() -> usize {
    34
}

fn matrix_link_text_selected(x: usize, y: usize) -> usize {
    print!(
        "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://matrix.to/#/#zellij_general:matrix.org",
        y + 1,
        x + 1
    );
    46
}

fn matrix_link_text_selected_len() -> usize {
    46
}


fn stacked_resize_screencast_link_selected(x: usize, y: usize) -> usize {
    print!(
        "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://zellij.dev/screencasts/stacked-resize",
        y + 1,
        x + 1
    );
    45
}

fn stacked_resize_screencast_link_selected_len() -> usize {
    45
}

fn colliding_keybindings_link_selected(x: usize, y: usize) -> usize {
    print!(
        "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://zellij.dev/tutorials/colliding-keybindings",
        y + 1,
        x + 1
    );
    51
}

fn colliding_keybindings_link_selected_len() -> usize {
    51
}

fn theme_link_selected(x: usize, y: usize) -> usize {
    print!(
        "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://zellij.dev/documentation/themes",
        y + 1,
        x + 1
    );
    39
}
fn theme_link_selected_len() -> usize {
    39
}

fn theme_list_selected(x: usize, y: usize) -> usize {
    print!(
        "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://zellij.dev/documentation/theme-list",
        y + 1,
        x + 1
    );
    43
}
fn theme_list_selected_len() -> usize {
    43
}

// Text components
fn whats_new_title() -> Text {
    Text::new("What's new?")
}

fn main_screen_title(version: String) -> Text {
    let title_text = format!("Hi there, welcome to Zellij {}!", &version);
    Text::new(title_text).color_range(2, 21..=27 + version.chars().count())
}

fn main_screen_help_text(hovering_over_link: bool, menu_item_is_selected: bool) -> Text {
    if hovering_over_link {
        let help_text = format!("Help: Click or Shift-Click to open in browser");
        Text::new(help_text)
            .color_range(3, 6..=10)
            .color_range(3, 15..=25)
    } else if menu_item_is_selected {
        let help_text = format!("Help: <↓↑> - Navigate, <ENTER> - Learn More, <ESC> - Dismiss");
        Text::new(help_text)
            .color_range(1, 6..=9)
            .color_range(1, 23..=29)
            .color_range(1, 45..=49)
    } else {
        let help_text = format!("Help: <↓↑> - Navigate, <ESC> - Dismiss");
        Text::new(help_text)
            .color_range(1, 6..=9)
            .color_range(1, 23..=27)
    }
}

fn esc_go_back_plus_link_hover(hovering_over_link: bool, _menu_item_is_selected: bool) -> Text {
    if hovering_over_link {
        let help_text = format!("Help: Click or Shift-Click to open in browser");
        Text::new(help_text)
            .color_range(3, 6..=10)
            .color_range(3, 15..=25)
    } else {
        let help_text = format!("Help: <ESC> - Go back");
        Text::new(help_text).color_range(1, 6..=10)
    }
}

fn esc_to_go_back_help() -> Text {
    let help_text = format!("Help: <ESC> - Go back");
    Text::new(help_text).color_range(1, 6..=10)
}

fn main_menu_item(item_name: &str) -> Text {
    Text::new(item_name).color_range(0, ..)
}

fn support_the_developer_text() -> Text {
    let support_text = format!("Please support the Zellij developer <3: ");
    Text::new(support_text).color_range(3, ..)
}

pub enum TextOrCustomRender {
    Text(Text),
    CustomRender(
        Box<dyn Fn(usize, usize) -> usize>, // (rows, columns) -> text_len (render function)
        Box<dyn Fn() -> usize>,             // length of rendered component
    ),
}

impl TextOrCustomRender {
    pub fn len(&self) -> usize {
        match self {
            TextOrCustomRender::Text(text) => text.len(),
            TextOrCustomRender::CustomRender(_render_fn, len_fn) => len_fn(),
        }
    }
    pub fn render(&mut self, x: usize, y: usize, rows: usize, columns: usize) -> usize {
        match self {
            TextOrCustomRender::Text(text) => {
                print_text_with_coordinates(text.clone(), x, y, Some(columns), Some(rows));
                text.len()
            },
            TextOrCustomRender::CustomRender(render_fn, _len_fn) => render_fn(x, y),
        }
    }
}

impl std::fmt::Debug for TextOrCustomRender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextOrCustomRender::Text(text) => write!(f, "Text {{ {:?} }}", text),
            TextOrCustomRender::CustomRender(..) => write!(f, "CustomRender"),
        }
    }
}

enum RenderedComponent {
    HelpText(Box<dyn Fn(bool, bool) -> Text>),
    BulletinList(BulletinList),
    Paragraph(Vec<ComponentLine>),
}

impl std::fmt::Debug for RenderedComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderedComponent::HelpText(_) => write!(f, "HelpText"),
            RenderedComponent::BulletinList(bulletinlist) => write!(f, "{:?}", bulletinlist),
            RenderedComponent::Paragraph(component_list) => write!(f, "{:?}", component_list),
        }
    }
}

impl RenderedComponent {
    pub fn render(
        &mut self,
        x: usize,
        y: usize,
        rows: usize,
        columns: usize,
        hovering_over_link: bool,
        menu_item_is_selected: bool,
    ) -> usize {
        let mut rendered_rows = 0;
        match self {
            RenderedComponent::HelpText(text) => {
                rendered_rows += 1;
                print_text_with_coordinates(
                    text(hovering_over_link, menu_item_is_selected),
                    0,
                    y,
                    Some(columns),
                    Some(rows),
                );
            },
            RenderedComponent::BulletinList(bulletin_list) => {
                rendered_rows += bulletin_list.len();
                bulletin_list.render(x, y, rows, columns);
            },
            RenderedComponent::Paragraph(paragraph) => {
                let mut paragraph_rendered_rows = 0;
                for component_line in paragraph {
                    component_line.render(
                        x,
                        y + paragraph_rendered_rows,
                        rows.saturating_sub(paragraph_rendered_rows),
                        columns,
                    );
                    rendered_rows += 1;
                    paragraph_rendered_rows += 1;
                }
            },
        }
        rendered_rows
    }
}

#[derive(Debug)]
pub struct BulletinList {
    title: Text,
    items: Vec<ActiveComponent>,
}

impl BulletinList {
    pub fn new(title: Text) -> Self {
        BulletinList {
            title,
            items: vec![],
        }
    }
    pub fn with_items(mut self, items: Vec<ActiveComponent>) -> Self {
        self.items = items;
        self
    }
    pub fn len(&self) -> usize {
        self.items.len() + 1 // 1 for the title
    }
    pub fn column_count(&self) -> usize {
        let mut column_count = 0;
        for item in &self.items {
            column_count = std::cmp::max(column_count, item.column_count());
        }
        column_count
    }
    pub fn handle_left_click_at_position(&mut self, x: usize, y: usize) -> Option<Page> {
        for component in &mut self.items {
            let page_to_render = component.handle_left_click_at_position(x, y);
            if page_to_render.is_some() {
                return page_to_render;
            }
        }
        None
    }
    pub fn handle_selection(&mut self) -> Option<Page> {
        for component in &mut self.items {
            let page_to_render = component.handle_selection();
            if page_to_render.is_some() {
                return page_to_render;
            }
        }
        None
    }
    pub fn handle_hover_at_position(&mut self, x: usize, y: usize) -> bool {
        for component in &mut self.items {
            let should_render = component.handle_hover_at_position(x, y);
            if should_render {
                return should_render;
            }
        }
        false
    }
    pub fn clear_hover(&mut self) {
        for component in &mut self.items {
            component.clear_hover();
        }
    }
    pub fn active_component_position(&self) -> Option<usize> {
        self.items.iter().position(|i| i.is_active)
    }
    pub fn clear_active_bulletins(&mut self) {
        self.items.iter_mut().for_each(|i| {
            i.is_active = false;
        });
    }
    pub fn set_active_bulletin(&mut self, new_index: usize) {
        self.items.get_mut(new_index).map(|i| {
            i.is_active = true;
        });
    }
    pub fn set_last_active_bulletin(&mut self) {
        self.items.last_mut().map(|i| {
            i.is_active = true;
        });
    }
    pub fn render(&mut self, x: usize, y: usize, rows: usize, columns: usize) {
        print_text_with_coordinates(self.title.clone(), x, y, Some(columns), Some(rows));
        let mut item_bulletin = 1;
        let mut running_y = y + 1;
        for item in &mut self.items {
            let mut item_bulletin_text = Text::new(format!("{}. ", item_bulletin));
            if item.is_active {
                item_bulletin_text = item_bulletin_text.selected();
            }
            let item_bulletin_text_len = item_bulletin_text.len();
            print_text_with_coordinates(
                item_bulletin_text,
                x,
                running_y,
                Some(item_bulletin_text_len),
                Some(rows),
            );
            item.render(
                x + item_bulletin_text_len,
                running_y,
                rows,
                columns.saturating_sub(item_bulletin_text_len),
            );
            running_y += 1;
            item_bulletin += 1;
        }
    }
}

#[derive(Debug)]
pub struct ComponentLine {
    components: Vec<ActiveComponent>,
}

impl ComponentLine {
    pub fn handle_left_click_at_position(&mut self, x: usize, y: usize) -> Option<Page> {
        for active_component in &mut self.components {
            let page_to_render = active_component.handle_left_click_at_position(x, y);
            if page_to_render.is_some() {
                return page_to_render;
            }
        }
        None
    }
    pub fn handle_hover_at_position(&mut self, x: usize, y: usize) -> bool {
        for active_component in &mut self.components {
            let should_render = active_component.handle_hover_at_position(x, y);
            if should_render {
                return should_render;
            }
        }
        false
    }
    pub fn clear_hover(&mut self) {
        for active_component in &mut self.components {
            active_component.clear_hover();
        }
    }
    pub fn column_count(&self) -> usize {
        let mut column_count = 0;
        for active_component in &self.components {
            column_count += active_component.column_count()
        }
        column_count
    }
    pub fn render(&mut self, x: usize, y: usize, rows: usize, columns: usize) {
        let mut current_x = x;
        let mut columns_left = columns;
        for component in &mut self.components {
            let component_len = component.render(current_x, y, rows, columns_left);
            current_x += component_len;
            columns_left = columns_left.saturating_sub(component_len);
        }
    }
}

impl ComponentLine {
    pub fn new(components: Vec<ActiveComponent>) -> Self {
        ComponentLine { components }
    }
}
