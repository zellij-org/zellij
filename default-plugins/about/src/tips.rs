use zellij_tile::prelude::*;

use std::cell::RefCell;
use std::rc::Rc;

use crate::active_component::{ActiveComponent, ClickAction};
use crate::pages::{BulletinList, ComponentLine, Page, TextOrCustomRender};

pub const MAX_TIP_INDEX: usize = 11;

impl Page {
    pub fn new_tip_screen(
        link_executable: Rc<RefCell<String>>,
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
            Page::tip_7(link_executable)
        } else if tip_index == 7 {
            Page::tip_8(link_executable)
        } else if tip_index == 8 {
            Page::tip_9(link_executable)
        } else if tip_index == 9 {
            Page::tip_10(link_executable, base_mode)
        } else if tip_index == 10 {
            Page::tip_11(link_executable)
        } else if tip_index == 11 {
            Page::tip_12(link_executable, base_mode)
        } else {
            Page::tip_1(link_executable)
        }
    }
    pub fn tip_1(link_executable: Rc<RefCell<String>>) -> Self {
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
    pub fn tip_2(link_executable: Rc<RefCell<String>>, base_mode: Rc<RefCell<InputMode>>) -> Self {
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
    pub fn tip_3(link_executable: Rc<RefCell<String>>) -> Self {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij Tip #3").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new("Want to make your floating pane bigger?"),
                ))]),
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new(
                        "You can switch to the ENLARGED layout with Alt ] while focused on it.",
                    )
                    .color_range(2, 22..=29)
                    .color_range(0, 43..=47),
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
    fn tip_4(link_executable: Rc<RefCell<String>>, base_mode: Rc<RefCell<InputMode>>) -> Page {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij tip #4").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![ActiveComponent::new(TextOrCustomRender::Text(
                    Text::new("It's possible to \"pin\" a floating pane so that it will always"),
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
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://zellij.dev/tutorials/stacked-resize")))
                        .with_hover(TextOrCustomRender::CustomRender(Box::new(stacked_resize_screencast_link_selected), Box::new(stacked_resize_screencast_link_selected_len)))
                        .with_left_click_action(ClickAction::new_open_link("https://zellij.dev/tutorials/stacked-resize".to_owned(), link_executable.clone()))
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
                            Text::new("Press TAB to go to Change Mode Behavior")
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
    pub fn tip_7(link_executable: Rc<RefCell<String>>) -> Page {
        Page::new()
            .main_screen()
            .with_title(Text::new("Zellij Tip #7").color_range(0, ..))
            .with_paragraph(vec![ComponentLine::new(vec![ActiveComponent::new(
                TextOrCustomRender::Text(Text::new(
                    "Want to customize the appearance and colors of Zellij?",
                )),
            )])])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Check out the built-in themes: ").color_range(2, ..),
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new(
                        "https://zellij.dev/documentation/theme-list",
                    )))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(theme_list_selected),
                        Box::new(theme_list_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://zellij.dev/documentation/theme-list".to_owned(),
                        link_executable.clone(),
                    )),
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Or create your own theme: ").color_range(2, ..),
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new(
                        "https://zellij.dev/documentation/themes",
                    )))
                    .with_hover(TextOrCustomRender::CustomRender(
                        Box::new(theme_link_selected),
                        Box::new(theme_link_selected_len),
                    ))
                    .with_left_click_action(ClickAction::new_open_link(
                        "https://zellij.dev/documentation/themes".to_owned(),
                        link_executable.clone(),
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
    pub fn tip_8(link_executable: Rc<RefCell<String>>) -> Page {
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
                            .color_range(2, 56..=61)
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
    pub fn tip_9(link_executable: Rc<RefCell<String>>) -> Page {
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
            .with_bulletin_list(
                BulletinList::new(
                    Text::new("The Zellij session-manager can:").color_range(2, 11..=25),
                )
                .with_items(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new(
                        "Create new sessions",
                    ))),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new(
                        "Switch between existing sessions",
                    ))),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new(
                        "Resurrect exited sessions",
                    ))),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new(
                        "Change the session name",
                    ))),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new(
                        "Disconnect other users from the current session",
                    ))),
                ]),
            )
            .with_paragraph(vec![ComponentLine::new(vec![ActiveComponent::new(
                TextOrCustomRender::Text(match *base_mode.borrow() {
                    InputMode::Locked => Text::new("Check it out with with: Ctrl g + o + w")
                        .color_range(3, 24..=29)
                        .color_indices(3, vec![33, 37]),
                    _ => Text::new("Check it out with with: Ctrl o + w")
                        .color_range(3, 24..=29)
                        .color_indices(3, vec![33]),
                }),
            )])])
            .with_paragraph(vec![ComponentLine::new(vec![ActiveComponent::new(
                TextOrCustomRender::Text(
                    Text::new("You can also use it as a welcome screen with: zellij -l welcome")
                        .color_range(0, 46..=62),
                ),
            )])])
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
    pub fn tip_11(link_executable: Rc<RefCell<String>>) -> Page {
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
        "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://zellij.dev/tutorials/stacked-resize",
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

fn support_the_developer_text() -> Text {
    let support_text = format!("Please support the Zellij developer <3: ");
    Text::new(support_text).color_range(3, ..)
}

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
        let help_text = format!(
            "Help: <ESC> - Dismiss, <↓↑> - Browse tips, <Ctrl c> - Don't show tips on startup"
        );
        Text::new(help_text)
            .color_range(1, 6..=10)
            .color_range(1, 23..=26)
            .color_range(1, 43..=50)
    }
}
