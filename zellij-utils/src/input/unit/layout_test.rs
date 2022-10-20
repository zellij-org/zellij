use super::super::layout::*;
use insta::assert_snapshot;

#[test]
fn empty_layout() {
    let kdl_layout = "layout";
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![
                PaneLayout {
                    children_split_direction: SplitDirection::Vertical,
                    children: vec![PaneLayout::default(), PaneLayout::default()],
                    ..Default::default()
                },
                PaneLayout {
                    children: vec![PaneLayout::default(), PaneLayout::default()],
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
fn layout_with_tabs() {
    let kdl_layout = r#"
        layout {
            tab
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    let expected_layout = Layout {
        tabs: vec![(None, PaneLayout::default())],
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    let expected_layout = Layout {
        tabs: vec![
            (
                None,
                PaneLayout {
                    children_split_direction: SplitDirection::Vertical,
                    children: vec![
                        PaneLayout::default(),
                        PaneLayout::default(),
                        PaneLayout::default(),
                    ],
                    ..Default::default()
                },
            ),
            (
                None,
                PaneLayout {
                    children_split_direction: SplitDirection::Horizontal,
                    children: vec![PaneLayout::default(), PaneLayout::default()],
                    ..Default::default()
                },
            ),
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![PaneLayout {
                run: Some(Run::Command(RunCommand {
                    command: PathBuf::from("htop"),
                    hold_on_close: true,
                    ..Default::default()
                })),
                ..Default::default()
            }],
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![PaneLayout {
                run: Some(Run::Command(RunCommand {
                    command: PathBuf::from("htop"),
                    cwd: Some(PathBuf::from("/path/to/my/cwd")),
                    hold_on_close: true,
                    ..Default::default()
                })),
                ..Default::default()
            }],
            ..Default::default()
        }),
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![PaneLayout {
                run: Some(Run::Command(RunCommand {
                    command: PathBuf::from("htop"),
                    cwd: Some(PathBuf::from("/path/to/my/cwd")),
                    args: vec![String::from("-h"), String::from("-v")],
                    hold_on_close: true,
                    ..Default::default()
                })),
                ..Default::default()
            }],
            ..Default::default()
        }),
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![PaneLayout {
                borderless: true,
                ..Default::default()
            }],
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![PaneLayout {
                focus: Some(true),
                ..Default::default()
            }],
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    let expected_layout = Layout {
        template: Some(PaneLayout {
            children: vec![PaneLayout {
                name: Some("my awesome pane".into()),
                ..Default::default()
            }],
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    let expected_layout = Layout {
        tabs: vec![
            (
                Some("my cool tab name 1".into()),
                PaneLayout {
                    children: vec![],
                    ..Default::default()
                },
            ),
            (
                Some("my cool tab name 2".into()),
                PaneLayout {
                    children: vec![],
                    ..Default::default()
                },
            ),
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    let expected_layout = Layout {
        tabs: vec![
            (
                Some("my first tab".into()),
                PaneLayout {
                    children_split_direction: SplitDirection::Horizontal,
                    children: vec![
                        PaneLayout::default(),
                        PaneLayout {
                            children_split_direction: SplitDirection::Vertical,
                            children: vec![PaneLayout::default(), PaneLayout::default()],
                            ..Default::default()
                        },
                        PaneLayout::default(),
                    ],
                    ..Default::default()
                },
            ),
            (
                Some("my second tab".into()),
                PaneLayout {
                    children_split_direction: SplitDirection::Horizontal,
                    children: vec![
                        PaneLayout::default(),
                        PaneLayout {
                            children_split_direction: SplitDirection::Horizontal,
                            children: vec![PaneLayout::default(), PaneLayout::default()],
                            ..Default::default()
                        },
                        PaneLayout::default(),
                    ],
                    ..Default::default()
                },
            ),
            (
                None,
                PaneLayout {
                    children_split_direction: SplitDirection::Horizontal,
                    children: vec![
                        PaneLayout::default(),
                        PaneLayout::default(),
                        PaneLayout::default(),
                    ],
                    ..Default::default()
                },
            ),
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None);
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None);
    assert!(
        layout.is_err(),
        "error provided for more than one children block"
    );
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
            horizontal-with-vertical-top name="my pane" {
                pane
                pane
            }
            horizontal-with-vertical-top
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None);
    assert!(
        layout.is_err(),
        "error provided for more than one children block"
    );
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None);
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None);
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
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None);
    assert!(
        layout.is_err(),
        "error provided for tab and pane on the same level"
    );
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
        "tab",
    ];
    for keyword in keywords {
        let kdl_layout = format!(
            "
            layout {{
                tab_template name=\"{}\" {{
                    pane
                    children
                    pane
                }}
                pane
            }}
        ",
            keyword
        );
        let layout = Layout::from_kdl(&kdl_layout, "layout_file_name".into(), None);
        assert!(
            layout.is_err(),
            "{}",
            format!(
                "error provided for tab template name with keyword: {}",
                keyword
            )
        );
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
        "tab",
    ];
    for keyword in keywords {
        let kdl_layout = format!(
            "
            layout {{
                pane_template name=\"{}\" {{
                    pane
                    children
                    pane
                }}
                pane
            }}
        ",
            keyword
        );
        let layout = Layout::from_kdl(&kdl_layout, "layout_file_name".into(), None);
        assert!(
            layout.is_err(),
            "{}",
            format!(
                "error provided for pane template name with keyword: {}",
                keyword
            )
        );
    }
}

#[test]
fn error_on_multiple_layout_nodes_in_file() {
    let kdl_layout = format!(
        "
        layout
        layout
    "
    );
    let layout_error = Layout::from_kdl(&kdl_layout, "layout_file_name".into(), None).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_unknown_layout_node() {
    let kdl_layout = format!(
        "
        layout {{
            pane
            i_am_not_a_proper_node
            pane
        }}
    "
    );
    let layout_error = Layout::from_kdl(&kdl_layout, "layout_file_name".into(), None).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_unknown_layout_pane_property() {
    let kdl_layout = format!(
        "
        layout {{
            pane spit_size=1
        }}
    "
    );
    let layout_error = Layout::from_kdl(&kdl_layout, "layout_file_name".into(), None).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_unknown_layout_pane_template_property() {
    let kdl_layout = format!(
        "
        layout {{
            pane_template name=\"my_cool_template\" spit_size=1
        }}
    "
    );
    let layout_error = Layout::from_kdl(&kdl_layout, "layout_file_name".into(), None).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_unknown_layout_tab_property() {
    let kdl_layout = format!(
        "
        layout {{
            tab spit_size=1
        }}
    "
    );
    let layout_error = Layout::from_kdl(&kdl_layout, "layout_file_name".into(), None).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_unknown_layout_tab_template_property() {
    let kdl_layout = format!(
        "
        layout {{
            tab_template name=\"my_cool_template\" spit_size=1
        }}
    "
    );
    let layout_error = Layout::from_kdl(&kdl_layout, "layout_file_name".into(), None).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_pane_templates_without_a_name() {
    let kdl_layout = format!(
        "
        layout {{
            pane_template {{
                pane
                children
                pane
            }}
        }}
    "
    );
    let layout_error = Layout::from_kdl(&kdl_layout, "layout_file_name".into(), None).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn error_on_tab_templates_without_a_name() {
    let kdl_layout = format!(
        "
        layout {{
            tab_template {{
                pane
                children
                pane
            }}
        }}
    "
    );
    let layout_error = Layout::from_kdl(&kdl_layout, "layout_file_name".into(), None).unwrap_err();
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
    let layout_error = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap_err();
    assert_snapshot!(format!("{:?}", layout_error));
}

#[test]
fn args_override_args_in_template() {
    let kdl_layout = r#"
        layout {
            pane_template name="tail" {
                command "tail"
                args "-f" "/tmp/foo"
            }
            tail
            tail {
                args "-f" "/tmp/bar"
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn args_added_to_args_in_template() {
    let kdl_layout = r#"
        layout {
            pane_template name="tail" {
                command "tail"
            }
            tail
            tail {
                args "-f" "/tmp/bar"
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn cwd_override_cwd_in_template() {
    let kdl_layout = r#"
        layout {
            pane_template name="tail" {
                command "tail"
                cwd "/tmp"
            }
            tail
            tail {
                cwd "/"
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn cwd_added_to_cwd_in_template() {
    let kdl_layout = r#"
        layout {
            pane_template name="tail" {
                command "tail"
            }
            tail
            tail {
                cwd "/home"
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn error_on_mixed_command_and_child_panes() {
    let kdl_layout = r#"
        layout {
            pane command="tail" {
                pane
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None);
    assert!(layout.is_err(), "error provided");
}

#[test]
fn error_on_mixed_cwd_and_child_panes() {
    let kdl_layout = r#"
        layout {
            pane cwd="/tmp" {
                pane
                pane
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None);
    assert!(layout.is_err(), "error provided");
}

#[test]
fn error_on_bare_args_without_command() {
    let kdl_layout = r#"
        layout {
            pane {
                args "-f"
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None);
    assert!(layout.is_err(), "error provided");
}

#[test]
fn error_on_bare_args_in_template_without_command() {
    let kdl_layout = r#"
        layout {
            pane_template name="my_template"
            my_template {
                args "--help"
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None);
    assert!(layout.is_err(), "error provided");
}

#[test]
fn pane_template_command_with_cwd_overriden_by_its_consumers_command_cwd() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            pane_template name="tail" {
                command "tail"
                cwd "bar"
            }
            tail command="pwd" {
                cwd "foo"
            }
            // pane should have /tmp/foo and not /tmp/bar as cwd
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn pane_template_command_with_cwd_remains_when_its_consumer_command_does_not_have_a_cwd() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            pane_template name="tail" {
                command "tail"
                cwd "bar"
            }
            tail command="pwd"
            // pane should have /tmp/bar as its cwd with the pwd command
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn pane_template_command_without_cwd_is_overriden_by_its_consumers_cwd() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            pane_template name="tail" {
                command "tail"
            }
            tail command="pwd" {
                cwd "bar"
            }
            // pane should have /tmp/bar as its cwd with the pwd command
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn pane_template_command_with_cwd_is_overriden_by_its_consumers_bare_cwd() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            pane_template name="tail" {
                command "tail"
                cwd "foo"
            }
            tail {
                cwd "bar"
            }
            // pane should have /tmp/bar as its cwd with the tail command
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn pane_template_command_without_cwd_receives_its_consumers_bare_cwd() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            pane_template name="tail" {
                command "tail"
            }
            tail {
                cwd "bar"
            }
            // pane should have /tmp/bar as its cwd with the tail command
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn pane_template_with_bare_cwd_overriden_by_its_consumers_bare_cwd() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            pane_template name="tail" {
                cwd "foo"
            }
            tail {
                cwd "bar"
            }
            // pane should have /tmp/foo without a command
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn pane_template_with_bare_propagated_to_its_consumer_command_without_cwd() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            pane_template name="tail" {
                cwd "foo"
            }
            tail command="tail"
            // pane should have /tmp/foo with the tail command
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn pane_template_with_bare_propagated_to_its_consumer_command_with_cwd() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            pane_template name="tail" {
                cwd "foo"
            }
            tail command="tail" {
                cwd "bar"
            }
            // pane should have /tmp/bar with the tail command
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn pane_template_with_bare_propagated_to_its_consumer_edit() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            pane_template name="tail" {
                cwd "foo"
            }
            tail edit="bar"
            // pane should have /tmp/foo/bar with the edit file variant
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn pane_template_with_command_propagated_to_its_consumer_edit() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            pane_template name="tail" command="not-vim" {
                cwd "foo"
            }
            tail edit="bar"
            // pane should have /tmp/foo/bar with the edit file variant
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn global_cwd_given_to_panes_without_cwd() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            pane
            pane command="tail"
            // both should have the /tmp cwd
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn global_cwd_prepended_to_panes_with_cwd() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            pane cwd="foo" // should be /tmp/foo
            pane command="tail" cwd="/home/foo" // should be /home/foo because its an absolute path
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn global_cwd_passed_from_layout_constructor() {
    // this is used by the new-tab cli action with --cwd
    let kdl_layout = r#"
        layout {
            pane
            pane command="tail"
            // both should have the /tmp cwd
        }
    "#;
    let layout = Layout::from_kdl(
        kdl_layout,
        "layout_file_name".into(),
        Some(PathBuf::from("/tmp")),
    )
    .unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn global_cwd_passed_from_layout_constructor_overrides_global_cwd_in_layout_file() {
    // this is used by the new-tab cli action with --cwd
    let kdl_layout = r#"
        layout {
            cwd "/home"
            pane
            pane command="tail"
            // both should have the /tmp cwd
        }
    "#;
    let layout = Layout::from_kdl(
        kdl_layout,
        "layout_file_name".into(),
        Some(PathBuf::from("/tmp")),
    )
    .unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn global_cwd_with_tab_cwd_given_to_panes_without_cwd() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            tab cwd="./foo" {
                pane
                pane command="tail"
            }
            // both should have the /tmp/foo cwd
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn tab_cwd_given_to_panes_without_cwd() {
    let kdl_layout = r#"
        layout {
            tab cwd="/tmp" {
                pane
                pane command="tail"
            }
            // both should have the /tmp cwd
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn tab_cwd_prepended_to_panes_with_cwd() {
    let kdl_layout = r#"
        layout {
            tab cwd="/tmp" {
                pane cwd="./foo"
                pane command="tail" cwd="./foo"
            }
            // both should have the /tmp/foo cwd
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn global_cwd_and_tab_cwd_prepended_to_panes_with_and_without_cwd() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            tab cwd="./foo" {
                pane // should have /tmp/foo
                pane command="tail" cwd="./bar" // should have /tmp/foo/bar
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn global_cwd_and_tab_cwd_prepended_to_panes_with_and_without_cwd_in_pane_templates() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            pane_template name="my_pane_template" {
                pane // should have /tmp/foo
                pane command="tail" cwd="./bar" // should have /tmp/foo/bar
                children
            }
            tab cwd="./foo" {
                my_pane_template {
                    pane // should have /tmp/foo
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn global_cwd_and_tab_cwd_prepended_to_panes_with_and_without_cwd_in_tab_templates() {
    let kdl_layout = r#"
        layout {
            cwd "/tmp"
            tab_template name="my_tab_template" {
                pane // should have /tmp/foo
                pane command="tail" cwd="./bar" // should have /tmp/foo/bar
                children
            }
            my_tab_template cwd="./foo" {
                pane // should have /tmp/foo
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, "layout_file_name".into(), None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}
