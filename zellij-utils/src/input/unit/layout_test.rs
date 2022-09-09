use crate::input::config::ConfigError;
use super::super::layout::*;
use std::convert::TryInto;
use insta::assert_snapshot;

#[test]
fn empty_layout() {
    let kdl_layout = "layout";
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout::default()),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_one_pane() {
    let kdl_layout = r#"
        layout {
            pane
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![PaneLayout::default()],
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_multiple_panes() {
    let kdl_layout = r#"
        layout {
            pane
            pane
            pane
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![
                PaneLayout::default(),
                PaneLayout::default(),
                PaneLayout::default(),
            ],
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_nested_panes() {
    let kdl_layout = r#"
        layout {
            pane split_direction="Vertical" {
                pane
                pane
            }
            pane {
                pane
                pane
            }
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![
                PaneLayout {
                    children_split_direction: SplitDirection::Vertical,
                    children: vec![
                        PaneLayout::default(),
                        PaneLayout::default(),
                    ],
                    ..Default::default()
                },
                PaneLayout {
                    children: vec![
                        PaneLayout::default(),
                        PaneLayout::default(),
                    ],
                    ..Default::default()
                }
            ],
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_tabs() {
    let kdl_layout = r#"
        layout {
            tab
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        tabs: vec![
            (None, PaneLayout::default()),
        ],
        template: Some(PaneLayout::default()),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_nested_differing_tabs() {
    let kdl_layout = r#"
        layout {
            tab split_direction="Vertical" {
                pane
                pane
                pane
            }
            tab {
                pane
                pane
            }
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        tabs: vec![
            (None, PaneLayout {
                children_split_direction: SplitDirection::Vertical,
                children: vec![
                    PaneLayout::default(),
                    PaneLayout::default(),
                    PaneLayout::default(),
                ],
                ..Default::default()
            }),
            (None, PaneLayout {
                children_split_direction: SplitDirection::Horizontal,
                children: vec![
                    PaneLayout::default(),
                    PaneLayout::default(),
                ],
                ..Default::default()
            }),
        ],
        template: Some(PaneLayout::default()),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_panes_in_different_mixed_split_sizes() {
    let kdl_layout = r#"
        layout {
            pane size=1;
            pane size="10%";
            pane;
            pane size=2;
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![
                PaneLayout {
                    split_size: Some(SplitSize::Fixed(1)),
                    ..Default::default()
                },
                PaneLayout {
                    split_size: Some(SplitSize::Percent(10)),
                    ..Default::default()
                },
                PaneLayout {
                    split_size: None,
                    ..Default::default()
                },
                PaneLayout {
                    split_size: Some(SplitSize::Fixed(2)),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_command_panes() {
    let kdl_layout = r#"
        layout {
            pane command="htop"
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![
                PaneLayout {
                    run: Some(Run::Command(RunCommand {
                        command: PathBuf::from("htop"),
                        ..Default::default()
                    })),
                    ..Default::default()
                }
            ],
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_command_panes_and_cwd() {
    let kdl_layout = r#"
        layout {
            pane command="htop" cwd="/path/to/my/cwd"
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        template: Some(
            PaneLayout {
                children: vec![
                    PaneLayout {
                        run: Some(Run::Command(RunCommand {
                            command: PathBuf::from("htop"),
                            cwd: Some(PathBuf::from("/path/to/my/cwd")),
                            ..Default::default()
                        })),
                        ..Default::default()
                    }
                ],
                ..Default::default()
            },
        ),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_command_panes_and_cwd_and_args() {
    let kdl_layout = r#"
        layout {
            pane command="htop" cwd="/path/to/my/cwd" {
                args "-h" "-v"
            }
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        template: Some(
            PaneLayout {
                children: vec![
                    PaneLayout {
                        run: Some(Run::Command(RunCommand {
                            command: PathBuf::from("htop"),
                            cwd: Some(PathBuf::from("/path/to/my/cwd")),
                            args: vec![String::from("-h"), String::from("-v")],
                            ..Default::default()
                        })),
                        ..Default::default()
                    }
                ],
                ..Default::default()
            },
        ),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_plugin_panes() {
    let kdl_layout = r#"
        layout {
            pane {
                plugin location="zellij:tab-bar"
            }
            pane {
                plugin location="file:/path/to/my/plugin.wasm"
            }
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![
                PaneLayout {
                    run: Some(Run::Plugin(RunPlugin {
                        location: RunPluginLocation::Zellij(PluginTag::new("tab-bar")),
                        _allow_exec_host_cmd: false,
                    })),
                    ..Default::default()
                },
                PaneLayout {
                    run: Some(Run::Plugin(RunPlugin {
                        location: RunPluginLocation::File(PathBuf::from("/path/to/my/plugin.wasm")),
                        _allow_exec_host_cmd: false,
                    })),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_borderless_panes() {
    let kdl_layout = r#"
        layout {
            pane borderless=true
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![
                PaneLayout {
                    borderless: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_focused_panes() {
    let kdl_layout = r#"
        layout {
            pane focus=true
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![
                PaneLayout {
                    focus: Some(true),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_pane_names() {
    let kdl_layout = r#"
        layout {
            pane name="my awesome pane"
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![
                PaneLayout {
                    name: Some("my awesome pane".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_tab_names() {
    let kdl_layout = r#"
        layout {
            tab name="my cool tab name 1"
            tab name="my cool tab name 2"
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        tabs: vec![
            (Some("my cool tab name 1".into()), PaneLayout {
                children: vec![],
                ..Default::default()
            }),
            (Some("my cool tab name 2".into()), PaneLayout {
                children: vec![],
                ..Default::default()
            }),
        ],
        template: Some(PaneLayout::default()),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_focused_tab() {
    let kdl_layout = r#"
        layout {
            tab
            tab focus=true
            tab
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        tabs: vec![
            (None, PaneLayout::default()),
            (None, PaneLayout::default()),
            (None, PaneLayout::default()),
        ],
        template: Some(PaneLayout::default()),
        focused_tab_index: Some(1),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_tab_templates() {
    let kdl_layout = r#"
        layout {
            tab_template name="one-above-one-below" {
                pane
                children
                pane
            }
            one-above-one-below name="my first tab" split_direction="Vertical" {
                pane
                pane
            }
            one-above-one-below name="my second tab" {
                pane
                pane
            }
            one-above-one-below
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        tabs: vec![
            (Some("my first tab".into()), PaneLayout {
                children_split_direction: SplitDirection::Horizontal,
                children: vec![
                    PaneLayout::default(),
                    PaneLayout {
                        children_split_direction: SplitDirection::Vertical,
                        children: vec![
                            PaneLayout::default(),
                            PaneLayout::default(),
                        ],
                        ..Default::default()
                    },
                    PaneLayout::default(),
                ],
                ..Default::default()
            }),
            (Some("my second tab".into()), PaneLayout {
                children_split_direction: SplitDirection::Horizontal,
                children: vec![
                    PaneLayout::default(),
                    PaneLayout {
                        children_split_direction: SplitDirection::Horizontal,
                        children: vec![
                            PaneLayout::default(),
                            PaneLayout::default(),
                        ],
                        ..Default::default()
                    },
                    PaneLayout::default(),
                ],
                ..Default::default()
            }),
            (None, PaneLayout {
                children_split_direction: SplitDirection::Horizontal,
                children: vec![
                    PaneLayout::default(),
                    PaneLayout::default(),
                    PaneLayout::default(),
                ],
                ..Default::default()
            }),
        ],
        template: Some(PaneLayout::default()),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_default_tab_template() {
    let kdl_layout = r#"
        layout {
            default_tab_template {
                pane
                children
                pane
            }
            tab name="my first tab" split_direction="Vertical" {
                pane
                pane
            }
            tab name="my second tab" {
                pane
                pane
            }
            tab
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    let expected_layout = Layout {
        tabs: vec![
            (Some("my first tab".into()), PaneLayout {
                children_split_direction: SplitDirection::Horizontal,
                children: vec![
                    PaneLayout::default(),
                    PaneLayout {
                        children_split_direction: SplitDirection::Vertical,
                        children: vec![
                            PaneLayout::default(),
                            PaneLayout::default(),
                        ],
                        ..Default::default()
                    },
                    PaneLayout::default(),
                ],
                ..Default::default()
            }),
            (Some("my second tab".into()), PaneLayout {
                children_split_direction: SplitDirection::Horizontal,
                children: vec![
                    PaneLayout::default(),
                    PaneLayout {
                        children_split_direction: SplitDirection::Horizontal,
                        children: vec![
                            PaneLayout::default(),
                            PaneLayout::default(),
                        ],
                        ..Default::default()
                    },
                    PaneLayout::default(),
                ],
                ..Default::default()
            }),
            (None, PaneLayout {
                children_split_direction: SplitDirection::Horizontal,
                children: vec![
                    PaneLayout::default(),
                    PaneLayout::default(),
                    PaneLayout::default(),
                ],
                ..Default::default()
            }),
        ],
        template: Some(PaneLayout {
            children_split_direction: SplitDirection::Horizontal,
            children: vec![
                PaneLayout::default(),
                PaneLayout::default(),
                PaneLayout::default(),
            ],
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_pane_templates() {
    let kdl_layout = r#"
        layout {
            pane_template name="left-and-right" split_direction="Vertical" {
                pane
                children
                pane
            }
            left-and-right {
                pane
            }
            left-and-right {
                pane
                pane
            }
            left-and-right split_direction="Vertical" {
                pane
                pane
            }
            left-and-right
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn layout_with_tab_and_pane_templates() {
    let kdl_layout = r#"
        layout {
            tab_template name="left-right-and-htop" {
                left-and-right {
                    pane command="htop"
                }
            }
            pane_template name="left-and-right" split_direction="Vertical" {
                pane
                children
                pane
            }
            left-right-and-htop
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn layout_with_nested_pane_templates() {
    let kdl_layout = r#"
        layout {
            pane_template name="left-and-right" split_direction="Vertical" {
                pane
                children
                up-and-down
                pane
            }
            pane_template name="up-and-down" split_direction="Horizontal" {
                pane
                pane
            }
            left-and-right
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn layout_with_nested_branched_pane_templates() {
    let kdl_layout = r#"
        layout {
            pane_template name="left-and-right" split_direction="Vertical" {
                pane
                children
                up-and-down
                three-horizontal-panes
            }
            pane_template name="three-horizontal-panes" split_direction="Horizontal" {
                pane
                pane
                pane
            }
            pane_template name="up-and-down" split_direction="Horizontal" {
                pane
                pane
            }
            left-and-right
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn circular_dependency_pane_templates_error() {
    let kdl_layout = r#"
        layout {
            pane_template name="one" split_direction="Vertical" {
                pane
                children
                two
            }
            pane_template name="two" split_direction="Horizontal" {
                pane
                three
                pane
            }
            pane_template name="three" split_direction="Horizontal" {
                one
            }
            one
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout);
    assert!(layout.is_err(), "circular dependency detected");
}

#[test]
fn children_not_as_first_child_of_tab_template() {
    let kdl_layout = r#"
        layout {
            tab_template name="horizontal-with-vertical-top" {
                pane split_direction="Vertical" {
                    pane
                    children
                }
                pane
            }
            horizontal-with-vertical-top name="my tab" {
                pane
                pane
            }
            horizontal-with-vertical-top
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn error_on_more_than_one_children_block_in_tab_template() {
    let kdl_layout = r#"
        layout {
            tab_template name="horizontal-with-vertical-top" {
                pane split_direction="Vertical" {
                    pane
                    children
                }
                children
                pane
            }
            horizontal-with-vertical-top name="my tab" {
                pane
                pane
            }
            horizontal-with-vertical-top
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout);
    assert!(layout.is_err(), "error provided for more than one children block");
}

#[test]
fn children_not_as_first_child_of_pane_template() {
    let kdl_layout = r#"
        layout {
            pane_template name="horizontal-with-vertical-top" {
                pane split_direction="Vertical" {
                    pane
                    children
                }
                pane
            }
            horizontal-with-vertical-top name="my tab" {
                pane
                pane
            }
            horizontal-with-vertical-top
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn error_on_more_than_one_children_block_in_pane_template() {
    let kdl_layout = r#"
        layout {
            pane_template name="horizontal-with-vertical-top" {
                pane split_direction="Vertical" {
                    pane
                    children
                }
                children
                pane
            }
            horizontal-with-vertical-top name="my tab" {
                pane
                pane
            }
            horizontal-with-vertical-top
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout);
    assert!(layout.is_err(), "error provided for more than one children block");
}

#[test]
fn combined_tab_and_pane_template_both_with_children() {
    let kdl_layout = r#"
        layout {
            tab_template name="horizontal-with-vertical-top" {
                vertical-sandwich {
                    pane name="middle"
                }
                children
            }
            pane_template name="vertical-sandwich" split_direction="Vertical" {
                pane
                children
                pane
            }
            horizontal-with-vertical-top name="my tab" {
                pane
                pane
            }
            horizontal-with-vertical-top
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn cannot_define_tab_template_name_with_space() {
    let kdl_layout = r#"
        layout {
            tab_template name="with space" {
                pane
                children
                pane
            }
            pane
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout);
    assert!(layout.is_err(), "error provided for tab name with space");
}

#[test]
fn cannot_define_pane_template_name_with_space() {
    let kdl_layout = r#"
        layout {
            pane_template name="with space" {
                pane
                children
                pane
            }
            pane
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout);
    assert!(layout.is_err(), "error provided for tab name with space");
}

#[test]
fn cannot_define_panes_and_tabs_on_same_level() {
    let kdl_layout = r#"
        layout {
            pane
            tab {
                pane
            }
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout);
    assert!(layout.is_err(), "error provided for tab and pane on the same level");
}

#[test]
fn cannot_define_tab_template_names_as_keywords() {
    let keywords = vec![
        "pane",
        "layout",
        "pane_template",
        "tab_template",
        "default_tab_template",
        "command",
        "plugin",
        "children",
        "tab"
    ];
    for keyword in keywords {
        let kdl_layout = format!("
            layout {{
                tab_template name=\"{}\" {{
                    pane
                    children
                    pane
                }}
                pane
            }}
        ", keyword);
        let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
        let layout = Layout::from_kdl(&kdl_layout);
        assert!(layout.is_err(), "{}", format!("error provided for tab template name with keyword: {}", keyword));
    }
}

#[test]
fn cannot_define_pane_template_names_as_keywords() {
    let keywords = vec![
        "pane",
        "layout",
        "pane_template",
        "tab_template",
        "command",
        "plugin",
        "children",
        "tab"
    ];
    for keyword in keywords {
        let kdl_layout = format!("
            layout {{
                pane_template name=\"{}\" {{
                    pane
                    children
                    pane
                }}
                pane
            }}
        ", keyword);
        let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
        let layout = Layout::from_kdl(&kdl_layout);
        assert!(layout.is_err(), "{}", format!("error provided for pane template name with keyword: {}", keyword));
    }
}

#[test]
fn error_on_multiple_layout_nodes_in_file() {
    let kdl_layout = format!("
        layout
        layout
    ");
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout_error = Layout::from_kdl(&kdl_layout).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_unknown_layout_node() {
    let kdl_layout = format!("
        layout {{
            pane
            i_am_not_a_proper_node
            pane
        }}
    ");
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout_error = Layout::from_kdl(&kdl_layout).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_unknown_layout_pane_property() {
    let kdl_layout = format!("
        layout {{
            pane spit_size=1
        }}
    ");
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout_error = Layout::from_kdl(&kdl_layout).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_unknown_layout_pane_template_property() {
    let kdl_layout = format!("
        layout {{
            pane_template name=\"my_cool_template\" spit_size=1
        }}
    ");
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout_error = Layout::from_kdl(&kdl_layout).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_unknown_layout_tab_property() {
    let kdl_layout = format!("
        layout {{
            tab spit_size=1
        }}
    ");
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout_error = Layout::from_kdl(&kdl_layout).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_unknown_layout_tab_template_property() {
    let kdl_layout = format!("
        layout {{
            tab_template name=\"my_cool_template\" spit_size=1
        }}
    ");
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout_error = Layout::from_kdl(&kdl_layout).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_pane_templates_without_a_name() {
    let kdl_layout = format!("
        layout {{
            pane_template {{
                pane
                children
                pane
            }}
        }}
    ");
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout_error = Layout::from_kdl(&kdl_layout).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_tab_templates_without_a_name() {
    let kdl_layout = format!("
        layout {{
            tab_template {{
                pane
                children
                pane
            }}
        }}
    ");
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout_error = Layout::from_kdl(&kdl_layout).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_more_than_one_focused_tab() {
    let kdl_layout = r#"
        layout {
            tab focus=true
            tab focus=true
            tab
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout_error = Layout::from_kdl(&kdl_layout).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}
