use super::super::layout::*;
use std::convert::TryInto;

fn layout_test_dir(layout: String) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let layout_dir = root.join("src/input/unit/fixtures/layouts");
    layout_dir.join(layout)
}

fn default_layout_dir(layout: String) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let layout_dir = root.join("assets/layouts");
    layout_dir.join(layout)
}

#[test]
fn empty_layout() {
    let kdl_layout = "layout";
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout, None).unwrap();
    let expected_layout = Layout::with_one_pane();
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_one_pane() {
    let kdl_layout = r#"
        layout {
            parts direction="Horizontal" {
                layout;
            }
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout, None).unwrap();
    let expected_layout = Layout::with_one_pane();
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_multiple_panes() {
    let kdl_layout = r#"
        layout {
            parts direction="Horizontal" {
                layout;
                layout;
                layout;
            }
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout, None).unwrap();
    let expected_layout = Layout {
        direction: SplitDirection::Horizontal,
        parts: LayoutParts::Panes(vec![
            Layout::default(),
            Layout::default(),
            Layout::default()
        ]),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_nested_panes() {
    let kdl_layout = r#"
        layout {
            parts direction="Horizontal" {
                layout {
                    parts direction="Vertical" {
                        layout;
                        layout;
                    }
                }
                layout {
                    parts direction="Horizontal" {
                        layout;
                        layout;
                    }
                }
            }
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout, None).unwrap();
    let expected_layout = Layout {
        direction: SplitDirection::Horizontal,
        parts: LayoutParts::Panes(vec![
            Layout {
                direction: SplitDirection::Vertical,
                parts: LayoutParts::Panes(vec![
                    Layout::default(),
                    Layout::default(),
                ]),
                ..Default::default()
            },
            Layout {
                direction: SplitDirection::Horizontal,
                parts: LayoutParts::Panes(vec![
                    Layout::default(),
                    Layout::default(),
                ]),
                ..Default::default()
            }
        ]),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_multiple_nested_panes() {
    let kdl_layout = r#"
        layout {
            parts direction="Horizontal" {
                layout {
                    parts direction="Vertical" {
                        layout;
                        layout {
                            parts direction="Vertical" {
                                layout;
                                layout
                            }
                        }
                    }
                }
                layout;
                layout {
                    parts direction="Horizontal" {
                        layout;
                        layout;
                    }
                }
            }
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout, None).unwrap();
    let expected_layout = Layout {
        direction: SplitDirection::Horizontal,
        parts: LayoutParts::Panes(vec![
            Layout {
                direction: SplitDirection::Vertical,
                parts: LayoutParts::Panes(vec![
                    Layout::default(),
                    Layout {
                        direction: SplitDirection::Vertical,
                        parts: LayoutParts::Panes(vec![
                            Layout::default(),
                            Layout::default(),
                        ]),
                        ..Default::default()
                    }
                ]),
                ..Default::default()
            },
            Layout::default(),
            Layout {
                direction: SplitDirection::Horizontal,
                parts: LayoutParts::Panes(vec![
                    Layout::default(),
                    Layout::default(),
                ]),
                ..Default::default()
            }
        ]),
        ..Default::default()
    };
    assert_eq!(layout, expected_layout);
}

#[test]
fn layout_with_tabs() {
    let kdl_layout = r#"
        layout {
            parts direction="Horizontal" {
                tabs {
                    layout;
                }
            }
        }
    "#;
    let kdl_layout: KdlDocument = kdl_layout.parse().unwrap();
    let layout = Layout::from_kdl(&kdl_layout, None).unwrap();
    let expected_layout = Layout {
        direction: SplitDirection::Horizontal,
        parts: LayoutParts::Tabs(vec![
            (None, Layout::default()),
        ]),
        ..Default::default()
    };
    // TODO: CONTINUE HERE (17/08)
    // need to get this test to pass... there's some issue with layouts created with one pane,
    // without one pane... need to figure out the best way to go about it
    // to test: cargo test -- layout_with_tabs --nocapture

    assert_eq!(layout.parts, expected_layout.parts);
}

#[test]
fn layout_with_empty_tabs_block() {
    // TBD
}

#[test]
fn layout_with_nested_differing_tabs() {
    // TBD
}

#[test]
fn layout_with_panes_in_different_mixed_split_sizes() {
    // TBD
}

#[test]
fn layout_with_panes_in_different_split_sizes() {
    // TBD
}

#[test]
fn layout_with_command_panes() {
    // TBD
}

#[test]
fn layout_with_plugin_panes() {
    // TBD
}

#[test]
fn layout_with_borderless_panes() {
    // TBD
}

#[test]
fn layout_with_focused_panes() {
    // TBD
}

#[test]
fn layout_with_pane_names() {
    // TBD
}

#[test]
fn layout_with_tab_names() {
    // TBD
}

// TODO: CONTINUE HERE
// - write tests similar to the config that will feed KDL into Layout::from_kdl and assert stuff
// about the layout
// - then bring these tests back
// TODO: BRING THESE TESTS BACK!!
//
//
// #[test]
// fn default_layout_is_ok() {
//     let path = default_layout_dir("default.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     assert!(layout.is_ok());
// }
//
// #[test]
// fn default_layout_has_one_tab() {
//     let path = default_layout_dir("default.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     let layout_template = layout.as_ref().unwrap();
//     assert_eq!(layout_template.tabs.len(), 1);
// }
//
// #[test]
// fn default_layout_merged_correctly() {
//     let path = default_layout_dir("default.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     let layout_template = layout_from_yaml.as_ref().unwrap();
//     let tab_layout = layout_template
//         .template
//         .clone()
//         .insert_tab_layout(Some(layout_template.tabs[0].clone()));
//     let merged_layout = Layout {
//         direction: Direction::Horizontal,
//         borderless: false,
//         pane_name: None,
//         focus: None,
//         parts: vec![
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: true,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: Some(SplitSize::Fixed(1)),
//                 run: Some(Run::Plugin(RunPlugin {
//                     location: RunPluginLocation::Zellij(PluginTag::new("tab-bar")),
//                     _allow_exec_host_cmd: false,
//                 })),
//             },
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: None,
//                 run: None,
//             },
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: true,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: Some(SplitSize::Fixed(2)),
//                 run: Some(Run::Plugin(RunPlugin {
//                     location: RunPluginLocation::Zellij(PluginTag::new("status-bar")),
//                     _allow_exec_host_cmd: false,
//                 })),
//             },
//         ],
//         split_size: None,
//         run: None,
//     };
//     assert_eq!(merged_layout, tab_layout.try_into().unwrap());
// }
//
// #[test]
// fn default_layout_new_tab_correct() {
//     let path = default_layout_dir("default.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     let layout_template = layout_from_yaml.as_ref().unwrap();
//     let tab_layout = layout_template.template.clone().insert_tab_layout(None);
//     let merged_layout = Layout {
//         direction: Direction::Horizontal,
//         borderless: false,
//         pane_name: None,
//         focus: None,
//         parts: vec![
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: true,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: Some(SplitSize::Fixed(1)),
//                 run: Some(Run::Plugin(RunPlugin {
//                     location: RunPluginLocation::Zellij(PluginTag::new("tab-bar")),
//                     _allow_exec_host_cmd: false,
//                 })),
//             },
//             Layout {
//                 direction: Direction::Horizontal,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: None,
//                 run: None,
//             },
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: true,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: Some(SplitSize::Fixed(2)),
//                 run: Some(Run::Plugin(RunPlugin {
//                     location: RunPluginLocation::Zellij(PluginTag::new("status-bar")),
//                     _allow_exec_host_cmd: false,
//                 })),
//             },
//         ],
//         split_size: None,
//         run: None,
//     };
//     assert_eq!(merged_layout, tab_layout.try_into().unwrap());
// }
//
// #[test]
// fn default_strider_layout_is_ok() {
//     let path = default_layout_dir("strider.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     assert!(layout_from_yaml.is_ok());
// }
//
// #[test]
// fn default_disable_status_layout_is_ok() {
//     let path = default_layout_dir("disable-status-bar.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     assert!(layout_from_yaml.is_ok());
// }
//
// #[test]
// fn default_disable_status_layout_has_no_tab() {
//     let path = default_layout_dir("disable-status-bar.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     let layout_template = layout_from_yaml.as_ref().unwrap();
//     assert_eq!(layout_template.tabs.len(), 0);
// }
//
// #[test]
// fn three_panes_with_tab_is_ok() {
//     let path = layout_test_dir("three-panes-with-tab.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     assert!(layout.is_ok());
// }
//
// #[test]
// fn three_panes_with_tab_has_one_tab() {
//     let path = layout_test_dir("three-panes-with-tab.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     let layout_template = layout.unwrap();
//     assert_eq!(layout_template.tabs.len(), 1);
// }
//
// #[test]
// fn three_panes_with_tab_merged_correctly() {
//     let path = layout_test_dir("three-panes-with-tab.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     let layout_template = layout.as_ref().unwrap();
//     let tab_layout = layout_template
//         .template
//         .clone()
//         .insert_tab_layout(Some(layout_template.tabs[0].clone()));
//     let merged_layout = Layout {
//         direction: Direction::Horizontal,
//         borderless: false,
//         pane_name: None,
//         focus: None,
//         parts: vec![Layout {
//             direction: Direction::Vertical,
//             borderless: false,
//             pane_name: None,
//             focus: None,
//             parts: vec![
//                 Layout {
//                     direction: Direction::Horizontal,
//                     borderless: false,
//                     pane_name: None,
//                     focus: None,
//                     parts: vec![],
//                     split_size: Some(SplitSize::Percent(50)),
//                     run: None,
//                 },
//                 Layout {
//                     direction: Direction::Horizontal,
//                     borderless: false,
//                     pane_name: None,
//                     focus: None,
//                     parts: vec![
//                         Layout {
//                             direction: Direction::Vertical,
//                             borderless: false,
//                             pane_name: None,
//                             focus: None,
//                             parts: vec![],
//                             split_size: Some(SplitSize::Percent(50)),
//                             run: None,
//                         },
//                         Layout {
//                             direction: Direction::Vertical,
//                             borderless: false,
//                             pane_name: None,
//                             focus: None,
//                             parts: vec![],
//                             split_size: Some(SplitSize::Percent(50)),
//                             run: None,
//                         },
//                     ],
//                     split_size: None,
//                     run: None,
//                 },
//             ],
//             split_size: None,
//             run: None,
//         }],
//         split_size: None,
//         run: None,
//     };
//     assert_eq!(merged_layout, tab_layout.try_into().unwrap());
// }
//
// #[test]
// fn three_panes_with_tab_new_tab_is_correct() {
//     let path = layout_test_dir("three-panes-with-tab.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     let layout_template = layout.as_ref().unwrap();
//     let tab_layout = layout_template.template.clone().insert_tab_layout(None);
//     let merged_layout = Layout {
//         direction: Direction::Horizontal,
//         borderless: false,
//         pane_name: None,
//         focus: None,
//         parts: vec![Layout {
//             direction: Direction::Horizontal,
//             borderless: false,
//             pane_name: None,
//             focus: None,
//             parts: vec![],
//             split_size: None,
//             run: None,
//         }],
//         split_size: None,
//         run: None,
//     };
//     assert_eq!(merged_layout, tab_layout.try_into().unwrap());
// }
//
// #[test]
// fn three_panes_with_tab_and_default_plugins_is_ok() {
//     let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     assert!(layout.is_ok());
// }
//
// #[test]
// fn three_panes_with_tab_and_default_plugins_has_one_tab() {
//     let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     let layout_template = layout.unwrap();
//     assert_eq!(layout_template.tabs.len(), 1);
// }
//
// #[test]
// fn three_panes_with_tab_and_default_plugins_merged_correctly() {
//     let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     let layout_template = layout.as_ref().unwrap();
//     let tab_layout = layout_template
//         .template
//         .clone()
//         .insert_tab_layout(Some(layout_template.tabs[0].clone()));
//     let merged_layout = Layout {
//         direction: Direction::Horizontal,
//         borderless: false,
//         pane_name: None,
//         focus: None,
//         parts: vec![
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: Some(SplitSize::Fixed(1)),
//                 run: Some(Run::Plugin(RunPlugin {
//                     location: RunPluginLocation::Zellij(PluginTag::new("tab-bar")),
//                     _allow_exec_host_cmd: false,
//                 })),
//             },
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![
//                     Layout {
//                         direction: Direction::Horizontal,
//                         borderless: false,
//                         pane_name: None,
//                         focus: None,
//                         parts: vec![],
//                         split_size: Some(SplitSize::Percent(50)),
//                         run: None,
//                     },
//                     Layout {
//                         direction: Direction::Horizontal,
//                         borderless: false,
//                         pane_name: None,
//                         focus: None,
//                         parts: vec![
//                             Layout {
//                                 direction: Direction::Vertical,
//                                 borderless: false,
//                                 pane_name: None,
//                                 focus: None,
//                                 parts: vec![],
//                                 split_size: Some(SplitSize::Percent(50)),
//                                 run: None,
//                             },
//                             Layout {
//                                 direction: Direction::Vertical,
//                                 borderless: false,
//                                 pane_name: None,
//                                 focus: None,
//                                 parts: vec![],
//                                 split_size: Some(SplitSize::Percent(50)),
//                                 run: None,
//                             },
//                         ],
//                         split_size: None,
//                         run: None,
//                     },
//                 ],
//                 split_size: None,
//                 run: None,
//             },
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: Some(SplitSize::Fixed(2)),
//                 run: Some(Run::Plugin(RunPlugin {
//                     location: RunPluginLocation::Zellij(PluginTag::new("status-bar")),
//                     _allow_exec_host_cmd: false,
//                 })),
//             },
//         ],
//         split_size: None,
//         run: None,
//     };
//     assert_eq!(merged_layout, tab_layout.try_into().unwrap());
// }
//
// #[test]
// fn three_panes_with_tab_and_default_plugins_new_tab_is_correct() {
//     let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     let layout_template = layout.as_ref().unwrap();
//     let tab_layout = layout_template.template.clone().insert_tab_layout(None);
//     let merged_layout = Layout {
//         direction: Direction::Horizontal,
//         borderless: false,
//         pane_name: None,
//         focus: None,
//         parts: vec![
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: Some(SplitSize::Fixed(1)),
//                 run: Some(Run::Plugin(RunPlugin {
//                     location: RunPluginLocation::Zellij(PluginTag::new("tab-bar")),
//                     _allow_exec_host_cmd: false,
//                 })),
//             },
//             Layout {
//                 direction: Direction::Horizontal,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: None,
//                 run: None,
//             },
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: Some(SplitSize::Fixed(2)),
//                 run: Some(Run::Plugin(RunPlugin {
//                     location: RunPluginLocation::Zellij(PluginTag::new("status-bar")),
//                     _allow_exec_host_cmd: false,
//                 })),
//             },
//         ],
//         split_size: None,
//         run: None,
//     };
//     assert_eq!(merged_layout, tab_layout.try_into().unwrap());
// }
//
// #[test]
// fn deeply_nested_tab_is_ok() {
//     let path = layout_test_dir("deeply-nested-tab-layout.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     assert!(layout.is_ok());
// }
//
// #[test]
// fn deeply_nested_tab_has_one_tab() {
//     let path = layout_test_dir("deeply-nested-tab-layout.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     let layout_template = layout.unwrap();
//     assert_eq!(layout_template.tabs.len(), 1);
// }
//
// #[test]
// fn deeply_nested_tab_merged_correctly() {
//     let path = layout_test_dir("deeply-nested-tab-layout.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     let layout_template = layout.as_ref().unwrap();
//     let tab_layout = layout_template
//         .template
//         .clone()
//         .insert_tab_layout(Some(layout_template.tabs[0].clone()));
//     let merged_layout = Layout {
//         direction: Direction::Horizontal,
//         borderless: false,
//         pane_name: None,
//         focus: None,
//         parts: vec![
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![
//                     Layout {
//                         direction: Direction::Horizontal,
//                         borderless: false,
//                         pane_name: None,
//                         focus: None,
//                         parts: vec![],
//                         split_size: Some(SplitSize::Percent(21)),
//                         run: None,
//                     },
//                     Layout {
//                         direction: Direction::Vertical,
//                         borderless: false,
//                         pane_name: None,
//                         focus: None,
//                         parts: vec![
//                             Layout {
//                                 direction: Direction::Horizontal,
//                                 borderless: false,
//                                 pane_name: None,
//                                 focus: None,
//                                 parts: vec![],
//                                 split_size: Some(SplitSize::Percent(22)),
//                                 run: None,
//                             },
//                             Layout {
//                                 direction: Direction::Horizontal,
//                                 borderless: false,
//                                 pane_name: None,
//                                 focus: None,
//                                 parts: vec![
//                                     Layout {
//                                         direction: Direction::Horizontal,
//                                         borderless: false,
//                                         pane_name: None,
//                                         focus: None,
//                                         parts: vec![],
//                                         split_size: Some(SplitSize::Percent(23)),
//                                         run: None,
//                                     },
//                                     Layout {
//                                         direction: Direction::Horizontal,
//                                         borderless: false,
//                                         pane_name: None,
//                                         focus: None,
//                                         parts: vec![],
//                                         split_size: Some(SplitSize::Percent(24)),
//                                         run: None,
//                                     },
//                                 ],
//                                 split_size: Some(SplitSize::Percent(78)),
//                                 run: None,
//                             },
//                         ],
//                         split_size: Some(SplitSize::Percent(79)),
//                         run: None,
//                     },
//                 ],
//                 split_size: Some(SplitSize::Percent(90)),
//                 run: None,
//             },
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: Some(SplitSize::Percent(15)),
//                 run: None,
//             },
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: Some(SplitSize::Percent(15)),
//                 run: None,
//             },
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: Some(SplitSize::Percent(15)),
//                 run: None,
//             },
//         ],
//         split_size: None,
//         run: None,
//     };
//     assert_eq!(merged_layout, tab_layout.try_into().unwrap());
// }
//
// #[test]
// fn three_tabs_is_ok() {
//     let path = layout_test_dir("three-tabs-merged-correctly.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     assert!(layout_from_yaml.is_ok());
// }
//
// #[test]
// fn three_tabs_has_three_tabs() {
//     let path = layout_test_dir("three-tabs-merged-correctly.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     let layout_template = layout_from_yaml.unwrap();
//     assert_eq!(layout_template.tabs.len(), 3);
// }
//
// #[test]
// fn three_tabs_tab_one_merged_correctly() {
//     let path = layout_test_dir("three-tabs-merged-correctly.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     let layout_template = layout_from_yaml.as_ref().unwrap();
//     let tab_layout = layout_template
//         .template
//         .clone()
//         .insert_tab_layout(Some(layout_template.tabs[0].clone()));
//     let merged_layout = Layout {
//         direction: Direction::Vertical,
//         borderless: false,
//         pane_name: None,
//         focus: None,
//         parts: vec![
//             Layout {
//                 direction: Direction::Horizontal,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: Some(SplitSize::Percent(50)),
//                 run: None,
//             },
//             Layout {
//                 direction: Direction::Horizontal,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: None,
//                 run: None,
//             },
//         ],
//         split_size: None,
//         run: None,
//     };
//
//     assert_eq!(merged_layout, tab_layout.try_into().unwrap());
// }
//
// #[test]
// fn three_tabs_tab_two_merged_correctly() {
//     let path = layout_test_dir("three-tabs-merged-correctly.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     let layout_template = layout_from_yaml.as_ref().unwrap();
//     let tab_layout = layout_template
//         .template
//         .clone()
//         .insert_tab_layout(Some(layout_template.tabs[1].clone()));
//     let merged_layout = Layout {
//         direction: Direction::Vertical,
//         borderless: false,
//         pane_name: None,
//         focus: None,
//         parts: vec![
//             Layout {
//                 direction: Direction::Horizontal,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![
//                     Layout {
//                         direction: Direction::Horizontal,
//                         borderless: false,
//                         pane_name: None,
//                         focus: None,
//                         parts: vec![],
//                         split_size: Some(SplitSize::Percent(50)),
//                         run: None,
//                     },
//                     Layout {
//                         direction: Direction::Horizontal,
//                         borderless: false,
//                         pane_name: None,
//                         focus: None,
//                         parts: vec![],
//                         split_size: None,
//                         run: None,
//                     },
//                 ],
//                 split_size: Some(SplitSize::Percent(50)),
//                 run: None,
//             },
//             Layout {
//                 direction: Direction::Horizontal,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: None,
//                 run: None,
//             },
//         ],
//         split_size: None,
//         run: None,
//     };
//
//     assert_eq!(merged_layout, tab_layout.try_into().unwrap());
// }
//
// #[test]
// fn three_tabs_tab_three_merged_correctly() {
//     let path = layout_test_dir("three-tabs-merged-correctly.yaml".into());
//     let layout = LayoutFromYaml::new(&path);
//     let layout_template = layout.as_ref().unwrap();
//     let tab_layout = layout_template
//         .template
//         .clone()
//         .insert_tab_layout(Some(layout_template.tabs[2].clone()));
//     let merged_layout = Layout {
//         direction: Direction::Vertical,
//         borderless: false,
//         pane_name: None,
//         focus: None,
//         parts: vec![
//             Layout {
//                 direction: Direction::Vertical,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![
//                     Layout {
//                         direction: Direction::Vertical,
//                         borderless: false,
//                         pane_name: None,
//                         focus: None,
//                         parts: vec![],
//                         split_size: Some(SplitSize::Percent(50)),
//                         run: None,
//                     },
//                     Layout {
//                         direction: Direction::Horizontal,
//                         borderless: false,
//                         pane_name: None,
//                         focus: None,
//                         parts: vec![],
//                         split_size: None,
//                         run: None,
//                     },
//                 ],
//                 split_size: Some(SplitSize::Percent(50)),
//                 run: None,
//             },
//             Layout {
//                 direction: Direction::Horizontal,
//                 borderless: false,
//                 pane_name: None,
//                 focus: None,
//                 parts: vec![],
//                 split_size: None,
//                 run: None,
//             },
//         ],
//         split_size: None,
//         run: None,
//     };
//     assert_eq!(merged_layout, tab_layout.try_into().unwrap());
// }
//
// #[test]
// fn no_tabs_is_ok() {
//     let path = layout_test_dir("no-tab-section-specified.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     assert!(layout_from_yaml.is_ok());
// }
//
// #[test]
// fn no_tabs_has_no_tabs() {
//     let path = layout_test_dir("no-tab-section-specified.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     let layout_template = layout_from_yaml.unwrap();
//     assert_eq!(layout_template.tabs.len(), 0);
// }
//
// #[test]
// fn no_tabs_merged_correctly() {
//     let path = layout_test_dir("no-tab-section-specified.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     let layout_template = layout_from_yaml.as_ref().unwrap();
//     let tab_layout = layout_template.template.clone().insert_tab_layout(None);
//     let merged_layout = Layout {
//         direction: Direction::Horizontal,
//         borderless: false,
//         pane_name: None,
//         focus: None,
//         parts: vec![Layout {
//             direction: Direction::Horizontal,
//             borderless: false,
//             pane_name: None,
//             focus: None,
//             parts: vec![],
//             split_size: None,
//             run: None,
//         }],
//         split_size: None,
//         run: None,
//     };
//
//     assert_eq!(merged_layout, tab_layout.try_into().unwrap());
// }
//
// #[test]
// fn no_layout_template_specified_is_ok() {
//     let path = layout_test_dir("no-layout-template-specified.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     assert!(layout_from_yaml.is_ok());
// }
//
// #[test]
// fn no_layout_template_has_one_tab() {
//     let path = layout_test_dir("no-layout-template-specified.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     let layout_template = layout_from_yaml.unwrap();
//     assert_eq!(layout_template.tabs.len(), 1);
// }
//
// #[test]
// fn no_layout_template_merged_correctly() {
//     let path = layout_test_dir("no-layout-template-specified.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     let layout_template = layout_from_yaml.as_ref().unwrap();
//     let tab_layout = layout_template
//         .template
//         .clone()
//         .insert_tab_layout(Some(layout_template.tabs[0].clone()));
//     let merged_layout = Layout {
//         direction: Direction::Horizontal,
//         parts: vec![Layout {
//             direction: Direction::Vertical,
//             parts: vec![
//                 Layout {
//                     direction: Direction::Horizontal,
//                     parts: vec![],
//                     split_size: None,
//                     run: None,
//                     borderless: false,
//                     pane_name: None,
//                     focus: None,
//                 },
//                 Layout {
//                     direction: Direction::Horizontal,
//                     parts: vec![],
//                     split_size: None,
//                     run: None,
//                     borderless: false,
//                     pane_name: None,
//                     focus: None,
//                 },
//             ],
//             split_size: None,
//             run: None,
//             borderless: false,
//             pane_name: None,
//             focus: None,
//         }],
//         split_size: None,
//         run: None,
//         borderless: false,
//         pane_name: None,
//         focus: None,
//     };
//
//     assert_eq!(merged_layout, tab_layout.try_into().unwrap());
// }
//
// #[test]
// fn session_name_to_layout_is_ok() {
//     let path = layout_test_dir("session-name-to-layout.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     assert!(layout_from_yaml.is_ok());
// }
//
// #[test]
// fn session_name_to_layout_has_name() {
//     let path = layout_test_dir("session-name-to-layout.yaml".into());
//     let layout_from_yaml = LayoutFromYaml::new(&path);
//     let layout_template = layout_from_yaml.unwrap();
//     let session_layout = layout_template.session;
//
//     let expected_session = SessionFromYaml {
//         name: Some(String::from("zellij-session")),
//         attach: Some(true),
//     };
//
//     assert_eq!(expected_session, session_layout);
// }
