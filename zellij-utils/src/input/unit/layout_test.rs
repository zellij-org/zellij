use super::super::layout::*;
use insta::assert_snapshot;

#[test]
fn empty_layout() {
    let kdl_layout = "layout";
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((TiledPaneLayout::default(), vec![])),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![TiledPaneLayout::default()],
                ..Default::default()
            },
            vec![],
        )),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![
                    TiledPaneLayout::default(),
                    TiledPaneLayout::default(),
                    TiledPaneLayout::default(),
                ],
                ..Default::default()
            },
            vec![],
        )),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![
                    TiledPaneLayout {
                        children_split_direction: SplitDirection::Vertical,
                        children: vec![TiledPaneLayout::default(), TiledPaneLayout::default()],
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        children: vec![TiledPaneLayout::default(), TiledPaneLayout::default()],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            vec![],
        )),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_floating_panes() {
    let kdl_layout = r#"
        layout {
            floating_panes {
                pane
                pane {
                    x 10
                    y "10%"
                    width 10
                    height "10%"
                }
                pane x=10 y="10%"
                pane command="htop"
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout::default(),
            vec![
                FloatingPaneLayout::default(),
                FloatingPaneLayout {
                    x: Some(PercentOrFixed::Fixed(10)),
                    y: Some(PercentOrFixed::Percent(10)),
                    width: Some(PercentOrFixed::Fixed(10)),
                    height: Some(PercentOrFixed::Percent(10)),
                    ..Default::default()
                },
                FloatingPaneLayout {
                    x: Some(PercentOrFixed::Fixed(10)),
                    y: Some(PercentOrFixed::Percent(10)),
                    ..Default::default()
                },
                FloatingPaneLayout {
                    run: Some(Run::Command(RunCommand {
                        command: PathBuf::from("htop"),
                        hold_on_close: true,
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            ],
        )),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_mixed_panes_and_floating_panes() {
    let kdl_layout = r#"
        layout {
            pane
            pane
            floating_panes {
                pane
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![TiledPaneLayout::default(), TiledPaneLayout::default()],
                ..Default::default()
            },
            vec![FloatingPaneLayout::default()],
        )),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_hidden_floating_panes() {
    let kdl_layout = r#"
        layout {
            tab hide_floating_panes=true {
                pane
                pane
                floating_panes {
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        tabs: vec![(
            None,
            TiledPaneLayout {
                children: vec![TiledPaneLayout::default(), TiledPaneLayout::default()],
                hide_floating_panes: true,
                ..Default::default()
            },
            vec![FloatingPaneLayout::default()],
        )],
        template: Some((TiledPaneLayout::default(), vec![])),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_floating_panes_template() {
    let kdl_layout = r#"
        layout {
            pane_template name="my_cool_template" {
                x 10
                y "10%"
            }
            pane
            floating_panes {
                pane
                my_cool_template
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![TiledPaneLayout::default()],
                ..Default::default()
            },
            vec![
                FloatingPaneLayout::default(),
                FloatingPaneLayout {
                    x: Some(PercentOrFixed::Fixed(10)),
                    y: Some(PercentOrFixed::Percent(10)),
                    ..Default::default()
                },
            ],
        )),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_shared_tiled_and_floating_panes_template() {
    let kdl_layout = r#"
        layout {
            pane_template name="htop" {
                command "htop"
            }
            htop
            floating_panes {
                pane
                htop
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![TiledPaneLayout {
                    run: Some(Run::Command(RunCommand {
                        command: PathBuf::from("htop"),
                        hold_on_close: true,
                        ..Default::default()
                    })),
                    ..Default::default()
                }],
                ..Default::default()
            },
            vec![
                FloatingPaneLayout::default(),
                FloatingPaneLayout {
                    run: Some(Run::Command(RunCommand {
                        command: PathBuf::from("htop"),
                        hold_on_close: true,
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            ],
        )),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_tabs_and_floating_panes() {
    let kdl_layout = r#"
        layout {
            tab {
                floating_panes {
                    pane
                }
            }
            tab {
                floating_panes {
                    pane
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn layout_with_tabs() {
    let kdl_layout = r#"
        layout {
            tab
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        tabs: vec![(None, TiledPaneLayout::default(), vec![])],
        template: Some((TiledPaneLayout::default(), vec![])),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        tabs: vec![
            (
                None,
                TiledPaneLayout {
                    children_split_direction: SplitDirection::Vertical,
                    children: vec![
                        TiledPaneLayout::default(),
                        TiledPaneLayout::default(),
                        TiledPaneLayout::default(),
                    ],
                    ..Default::default()
                },
                vec![], // floating panes
            ),
            (
                None,
                TiledPaneLayout {
                    children_split_direction: SplitDirection::Horizontal,
                    children: vec![TiledPaneLayout::default(), TiledPaneLayout::default()],
                    ..Default::default()
                },
                vec![], // floating panes
            ),
        ],
        template: Some((TiledPaneLayout::default(), vec![])),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![
                    TiledPaneLayout {
                        split_size: Some(SplitSize::Fixed(1)),
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        split_size: Some(SplitSize::Percent(10)),
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        split_size: None,
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        split_size: Some(SplitSize::Fixed(2)),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            vec![],
        )),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![TiledPaneLayout {
                    run: Some(Run::Command(RunCommand {
                        command: PathBuf::from("htop"),
                        hold_on_close: true,
                        ..Default::default()
                    })),
                    ..Default::default()
                }],
                ..Default::default()
            },
            vec![],
        )),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![TiledPaneLayout {
                    run: Some(Run::Command(RunCommand {
                        command: PathBuf::from("htop"),
                        cwd: Some(PathBuf::from("/path/to/my/cwd")),
                        hold_on_close: true,
                        ..Default::default()
                    })),
                    ..Default::default()
                }],
                ..Default::default()
            },
            vec![],
        )),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![TiledPaneLayout {
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
            },
            vec![],
        )),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_command_panes_and_close_on_exit() {
    let kdl_layout = r#"
        layout {
            pane command="htop" {
                close_on_exit true
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn layout_with_command_panes_and_start_suspended() {
    let kdl_layout = r#"
        layout {
            pane command="htop" {
                start_suspended true
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
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
            pane {
                plugin location="zellij:status-bar" {
                    config_key_1 "config_value_1"
                    "2" true
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let mut expected_plugin_configuration = BTreeMap::new();
    expected_plugin_configuration.insert("config_key_1".to_owned(), "config_value_1".to_owned());
    expected_plugin_configuration.insert("2".to_owned(), "true".to_owned());
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![
                    TiledPaneLayout {
                        run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                            location: RunPluginLocation::Zellij(PluginTag::new("tab-bar")),
                            _allow_exec_host_cmd: false,
                            configuration: Default::default(),
                            ..Default::default()
                        }))),
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                            location: RunPluginLocation::File(PathBuf::from(
                                "/path/to/my/plugin.wasm",
                            )),
                            _allow_exec_host_cmd: false,
                            configuration: Default::default(),
                            ..Default::default()
                        }))),
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                            location: RunPluginLocation::Zellij(PluginTag::new("status-bar")),
                            _allow_exec_host_cmd: false,
                            configuration: PluginUserConfiguration(expected_plugin_configuration),
                            ..Default::default()
                        }))),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            vec![],
        )),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![TiledPaneLayout {
                    borderless: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
            vec![],
        )),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![TiledPaneLayout {
                    focus: Some(true),
                    ..Default::default()
                }],
                ..Default::default()
            },
            vec![],
        )),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![TiledPaneLayout {
                    name: Some("my awesome pane".into()),
                    ..Default::default()
                }],
                ..Default::default()
            },
            vec![],
        )),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        tabs: vec![
            (
                Some("my cool tab name 1".into()),
                TiledPaneLayout {
                    children: vec![],
                    ..Default::default()
                },
                vec![], // floating panes
            ),
            (
                Some("my cool tab name 2".into()),
                TiledPaneLayout {
                    children: vec![],
                    ..Default::default()
                },
                vec![], // floating panes
            ),
        ],
        template: Some((TiledPaneLayout::default(), vec![])),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        tabs: vec![
            (None, TiledPaneLayout::default(), vec![]),
            (None, TiledPaneLayout::default(), vec![]),
            (None, TiledPaneLayout::default(), vec![]),
        ],
        template: Some((TiledPaneLayout::default(), vec![])),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        tabs: vec![
            (
                Some("my first tab".into()),
                TiledPaneLayout {
                    children_split_direction: SplitDirection::Horizontal,
                    children: vec![
                        TiledPaneLayout::default(),
                        TiledPaneLayout {
                            children_split_direction: SplitDirection::Vertical,
                            children: vec![TiledPaneLayout::default(), TiledPaneLayout::default()],
                            ..Default::default()
                        },
                        TiledPaneLayout::default(),
                    ],
                    ..Default::default()
                },
                vec![], // floating panes
            ),
            (
                Some("my second tab".into()),
                TiledPaneLayout {
                    children_split_direction: SplitDirection::Horizontal,
                    children: vec![
                        TiledPaneLayout::default(),
                        TiledPaneLayout {
                            children_split_direction: SplitDirection::Horizontal,
                            children: vec![TiledPaneLayout::default(), TiledPaneLayout::default()],
                            ..Default::default()
                        },
                        TiledPaneLayout::default(),
                    ],
                    ..Default::default()
                },
                vec![], // floating panes
            ),
            (
                None,
                TiledPaneLayout {
                    children_split_direction: SplitDirection::Horizontal,
                    children: vec![
                        TiledPaneLayout::default(),
                        TiledPaneLayout::default(),
                        TiledPaneLayout::default(),
                    ],
                    ..Default::default()
                },
                vec![], // floating panes
            ),
        ],
        template: Some((TiledPaneLayout::default(), vec![])),
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn layout_with_new_tab_template() {
    let kdl_layout = r#"
        layout {
            new_tab_template {
                pane split_direction="vertical" {
                    pane
                    pane
                }
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn layout_with_pane_excluded_from_sync() {
    let kdl_layout = r#"
        layout {
            pane exclude_from_sync=true
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
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
        let layout = Layout::from_kdl(&kdl_layout, Some("layout_file_name".into()), None, None);
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
        let layout = Layout::from_kdl(&kdl_layout, Some("layout_file_name".into()), None, None);
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
    let layout_error =
        Layout::from_kdl(&kdl_layout, Some("layout_file_name".into()), None, None).unwrap_err();
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
    let layout_error =
        Layout::from_kdl(&kdl_layout, Some("layout_file_name".into()), None, None).unwrap_err();
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
    let layout_error =
        Layout::from_kdl(&kdl_layout, Some("layout_file_name".into()), None, None).unwrap_err();
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
    let layout_error =
        Layout::from_kdl(&kdl_layout, Some("layout_file_name".into()), None, None).unwrap_err();
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
    let layout_error =
        Layout::from_kdl(&kdl_layout, Some("layout_file_name".into()), None, None).unwrap_err();
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
    let layout_error =
        Layout::from_kdl(&kdl_layout, Some("layout_file_name".into()), None, None).unwrap_err();
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
    let layout_error =
        Layout::from_kdl(&kdl_layout, Some("layout_file_name".into()), None, None).unwrap_err();
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
    let layout_error =
        Layout::from_kdl(&kdl_layout, Some("layout_file_name".into()), None, None).unwrap_err();
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
    let layout_error =
        Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap_err();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn close_on_exit_overrides_close_on_exit_in_template() {
    let kdl_layout = r#"
        layout {
            pane_template name="tail" {
                command "tail"
                close_on_exit false
            }
            tail
            tail {
                close_on_exit true
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn close_on_exit_added_to_close_on_exit_in_template() {
    let kdl_layout = r#"
        layout {
            pane_template name="tail" {
                command "tail"
            }
            tail
            tail {
                close_on_exit true
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
    assert!(layout.is_err(), "error provided");
}

#[test]
fn error_on_bare_close_on_exit_without_command() {
    let kdl_layout = r#"
        layout {
            pane {
                close_on_exit true
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
    assert!(layout.is_err(), "error provided");
}

#[test]
fn error_on_bare_close_on_exit_in_template_without_command() {
    let kdl_layout = r#"
        layout {
            pane_template name="my_template"
            my_template {
                close_on_exit true
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
        Some("layout_file_name".into()),
        None,
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
        Some("layout_file_name".into()),
        None,
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
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
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn can_load_swap_layouts_from_a_different_file() {
    let kdl_layout = r#"
        layout {
            // here we define a tab_template in the main layout and later make sure we can sue it
            // in the swap layouts
            tab_template name="ui" {
               pane size=1 borderless=true {
                   plugin location="zellij:tab-bar"
               }
               children
               pane size=2 borderless=true {
                   plugin location="zellij:status-bar"
               }
            }
            pane
        }
    "#;
    let kdl_swap_layout = r#"
        swap_tiled_layout name="vertical" {
            ui max_panes=5 {
                pane split_direction="vertical" {
                    pane
                    pane { children; }
                }
            }
            ui max_panes=8 {
                pane split_direction="vertical" {
                    pane { children; }
                    pane { pane; pane; pane; pane; }
                }
            }
            ui max_panes=12 {
                pane split_direction="vertical" {
                    pane { children; }
                    pane { pane; pane; pane; pane; }
                    pane { pane; pane; pane; pane; }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(
        kdl_layout,
        Some("layout_file_name".into()),
        Some(("swap_layout_file_name".into(), kdl_swap_layout)),
        None,
    )
    .unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn can_define_stacked_children_for_pane_node() {
    let kdl_layout = r#"
        layout {
           pane stacked=true {
               pane
               pane
           }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn can_define_stacked_children_for_pane_template() {
    let kdl_layout = r#"
        layout {
           pane_template name="stack" stacked=true {
               children
           }
           stack {
               pane
               pane
           }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn can_define_a_stack_with_an_expanded_pane() {
    let kdl_layout = r#"
        layout {
           pane stacked=true {
               pane
               pane expanded=true
               pane
           }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    assert_snapshot!(format!("{:#?}", layout));
}

#[test]
fn cannot_define_stacked_panes_for_bare_node() {
    let kdl_layout = r#"
        layout {
           pane stacked=true
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
    assert!(layout.is_err(), "error provided for tab name with space");
}

#[test]
fn cannot_define_an_expanded_pane_outside_of_a_stack() {
    let kdl_layout = r#"
        layout {
            pane {
                pane
                pane expanded=true
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
    assert!(layout.is_err(), "error provided for tab name with space");
}

#[test]
fn cannot_define_stacked_panes_with_vertical_split_direction() {
    let kdl_layout = r#"
        layout {
           pane stacked=true split_direction="vertical" {
               pane
               pane
           }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
    assert!(layout.is_err(), "error provided for tab name with space");
}

#[test]
fn cannot_define_stacked_panes_with_grandchildren() {
    let kdl_layout = r#"
        layout {
           pane stacked=true {
               pane {
                   pane
                   pane
               }
               pane
           }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
    assert!(layout.is_err(), "error provided for tab name with space");
}

#[test]
fn cannot_define_stacked_panes_with_grandchildren_in_pane_template() {
    let kdl_layout = r#"
        layout {
           pane_template name="stack" stacked=true {
               children
           }
           stack {
               pane
               pane {
                   pane
                   pane
               }
           }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
    assert!(layout.is_err(), "error provided for tab name with space");
}

#[test]
fn run_plugin_location_parsing() {
    let kdl_layout = r#"
        layout {
            pane {
                plugin location="zellij:tab-bar"
            }
            pane {
                plugin location="file:/path/to/my/plugin.wasm"
            }
            pane {
                plugin location="file:plugin.wasm"
            }
            pane {
                plugin location="file:relative/with space/plugin.wasm"
            }
            pane {
                plugin location="file:///absolute/with space/plugin.wasm"
            }
            pane {
                plugin location="file:c:/absolute/windows/plugin.wasm"
            }
            pane {
                plugin location="filepicker"
            }
            pane {
                plugin location="https://example.com/plugin.wasm"
            }
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None).unwrap();
    let expected_layout = Layout {
        template: Some((
            TiledPaneLayout {
                children: vec![
                    TiledPaneLayout {
                        run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                            _allow_exec_host_cmd: false,
                            location: RunPluginLocation::Zellij(PluginTag::new("tab-bar")),
                            configuration: Default::default(),
                            ..Default::default()
                        }))),
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                            _allow_exec_host_cmd: false,
                            location: RunPluginLocation::File(PathBuf::from(
                                "/path/to/my/plugin.wasm",
                            )),
                            configuration: Default::default(),
                            ..Default::default()
                        }))),
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                            _allow_exec_host_cmd: false,
                            location: RunPluginLocation::File(PathBuf::from("plugin.wasm")),
                            configuration: Default::default(),
                            ..Default::default()
                        }))),
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                            _allow_exec_host_cmd: false,
                            location: RunPluginLocation::File(PathBuf::from(
                                "relative/with space/plugin.wasm",
                            )),
                            configuration: Default::default(),
                            ..Default::default()
                        }))),
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                            _allow_exec_host_cmd: false,
                            location: RunPluginLocation::File(PathBuf::from(
                                "/absolute/with space/plugin.wasm",
                            )),
                            configuration: Default::default(),
                            ..Default::default()
                        }))),
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                            _allow_exec_host_cmd: false,
                            location: RunPluginLocation::File(PathBuf::from(
                                "c:/absolute/windows/plugin.wasm",
                            )),
                            configuration: Default::default(),
                            ..Default::default()
                        }))),
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        run: Some(Run::Plugin(RunPluginOrAlias::Alias(PluginAlias {
                            name: "filepicker".to_owned(),
                            configuration: Some(PluginUserConfiguration::default()),
                            ..Default::default()
                        }))),
                        ..Default::default()
                    },
                    TiledPaneLayout {
                        run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
                            _allow_exec_host_cmd: false,
                            location: RunPluginLocation::Remote(String::from(
                                "https://example.com/plugin.wasm",
                            )),
                            configuration: Default::default(),
                            ..Default::default()
                        }))),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            vec![],
        )),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn env_var_expansion() {
    let raw_layout = r#"
        layout {
            // cwd tests + composition
            cwd "$TEST_GLOBAL_CWD"
            pane cwd="relative"  // -> /abs/path/relative
            pane cwd="/another/abs"  // -> /another/abs
            pane cwd="$TEST_LOCAL_CWD"  // -> /another/abs
            pane cwd="$TEST_RELATIVE"  // -> /abs/path/relative
            pane command="ls" cwd="$TEST_ABSOLUTE"  // -> /somewhere
            pane edit="file.rs" cwd="$TEST_ABSOLUTE"  // -> /somewhere/file.rs
            pane edit="file.rs" cwd="~/backup"  // -> /home/aram/backup/file.rs

            // other paths
            pane command="~/backup/executable"  // -> /home/aram/backup/executable
            pane edit="~/backup/foo.txt"  // -> /home/aram/backup/foo.txt
        }
    "#;
    let env_vars = [
        ("TEST_GLOBAL_CWD", "/abs/path"),
        ("TEST_LOCAL_CWD", "/another/abs"),
        ("TEST_RELATIVE", "relative"),
        ("TEST_ABSOLUTE", "/somewhere"),
        ("HOME", "/home/aram"),
    ];
    let mut old_vars = Vec::new();
    // set environment variables for test, keeping track of existing values.
    for (key, value) in env_vars {
        old_vars.push((key, std::env::var(key).ok()));
        std::env::set_var(key, value);
    }
    let layout = Layout::from_kdl(raw_layout, Some("layout_file_name".into()), None, None);
    // restore environment.
    for (key, opt) in old_vars {
        match opt {
            Some(value) => std::env::set_var(key, &value),
            None => std::env::remove_var(key),
        }
    }
    let layout = layout.unwrap();
    assert_snapshot!(format!("{layout:#?}"));
}

#[test]
fn env_var_missing() {
    std::env::remove_var("SOME_UNIQUE_VALUE");
    let kdl_layout = r#"
        layout {
            cwd "$SOME_UNIQUE_VALUE"
            pane cwd="relative"
        }
    "#;
    let layout = Layout::from_kdl(kdl_layout, Some("layout_file_name".into()), None, None);
    assert!(layout.is_err(), "invalid env var lookup should fail");
}
