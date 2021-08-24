use super::super::layout::*;

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
fn default_layout_is_ok() {
    let path = default_layout_dir("default.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    assert!(layout.is_ok());
}

#[test]
fn default_layout_has_one_tab() {
    let path = default_layout_dir("default.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.as_ref().unwrap().construct_layout_template();
    assert_eq!(layout_template.tabs.len(), 1);
}

#[test]
fn default_layout_has_one_pre_tab() {
    let path = default_layout_dir("default.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.as_ref().unwrap().construct_layout_template();
    assert_eq!(layout_template.pre_tab.parts.len(), 1);
}

#[test]
fn default_layout_has_one_post_tab() {
    let path = default_layout_dir("default.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.as_ref().unwrap().construct_layout_template();
    assert_eq!(layout_template.post_tab.len(), 1);
}

#[test]
fn default_layout_merged_correctly() {
    let path = default_layout_dir("default.yaml".into());
    let layout_from_yaml = LayoutFromYaml::new(&path);
    let layout_template = layout_from_yaml
        .as_ref()
        .unwrap()
        .construct_layout_template();
    let tab_layout = layout_template.construct_tab_layout(Some(layout_template.tabs[0].clone()));
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        borderless: false,
        parts: vec![
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Fixed(1)),
                run: Some(Run::Plugin(Some("tab-bar".into()))),
            },
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: None,
                run: None,
            },
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Fixed(2)),
                run: Some(Run::Plugin(Some("status-bar".into()))),
            },
        ],
        tabs: vec![],
        split_size: None,
        run: None,
    };
    assert_eq!(merged_layout, tab_layout);
}

#[test]
fn default_layout_new_tab_correct() {
    let path = default_layout_dir("default.yaml".into());
    let layout_from_yaml = LayoutFromYaml::new(&path);
    let layout_template = layout_from_yaml
        .as_ref()
        .unwrap()
        .construct_layout_template();
    let tab_layout = layout_template.construct_tab_layout(None);
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        borderless: false,
        parts: vec![
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Fixed(1)),
                run: Some(Run::Plugin(Some("tab-bar".into()))),
            },
            Layout {
                direction: Direction::Horizontal,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: None,
                run: None,
            },
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Fixed(2)),
                run: Some(Run::Plugin(Some("status-bar".into()))),
            },
        ],
        tabs: vec![],
        split_size: None,
        run: None,
    };
    assert_eq!(merged_layout, tab_layout);
}

#[test]
fn default_strider_layout_is_ok() {
    let path = default_layout_dir("strider.yaml".into());
    let layout_from_yaml = LayoutFromYaml::new(&path);
    assert!(layout_from_yaml.is_ok());
}

#[test]
fn default_disable_status_layout_is_ok() {
    let path = default_layout_dir("disable-status-bar.yaml".into());
    let layout_from_yaml = LayoutFromYaml::new(&path);
    assert!(layout_from_yaml.is_ok());
}

#[test]
fn default_disable_status_layout_has_no_tab() {
    let path = default_layout_dir("disable-status-bar.yaml".into());
    let layout_from_yaml = LayoutFromYaml::new(&path);
    let layout_template = layout_from_yaml
        .as_ref()
        .unwrap()
        .construct_layout_template();
    assert_eq!(layout_template.tabs.len(), 0);
}

#[test]
fn default_disable_status_layout_has_one_pre_tab() {
    let path = default_layout_dir("disable-status-bar.yaml".into());
    let layout_from_yaml = LayoutFromYaml::new(&path);
    let layout_template = layout_from_yaml
        .as_ref()
        .unwrap()
        .construct_layout_template();
    assert_eq!(layout_template.pre_tab.parts.len(), 1);
}

#[test]
fn default_disable_status_layout_has_no_post_tab() {
    let path = default_layout_dir("disable-status-bar.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.as_ref().unwrap().construct_layout_template();
    assert!(layout_template.post_tab.is_empty());
}

#[test]
fn three_panes_with_tab_is_ok() {
    let path = layout_test_dir("three-panes-with-tab.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    assert!(layout.is_ok());
}

#[test]
fn three_panes_with_tab_has_one_tab() {
    let path = layout_test_dir("three-panes-with-tab.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.unwrap().construct_layout_template();
    assert_eq!(layout_template.tabs.len(), 1);
}

#[test]
fn three_panes_with_tab_no_post_tab() {
    let path = layout_test_dir("three-panes-with-tab.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.unwrap().construct_layout_template();
    assert!(layout_template.post_tab.is_empty());
}

#[test]
fn three_panes_with_tab_no_pre_tab() {
    let path = layout_test_dir("three-panes-with-tab.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.unwrap().construct_layout_template();
    assert!(layout_template.pre_tab.parts.is_empty());
}

#[test]
fn three_panes_with_tab_merged_correctly() {
    let path = layout_test_dir("three-panes-with-tab.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.as_ref().unwrap().construct_layout_template();
    let tab_layout = layout_template.construct_tab_layout(Some(layout_template.tabs[0].clone()));
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        borderless: false,
        parts: vec![Layout {
            direction: Direction::Vertical,
            borderless: false,
            parts: vec![
                Layout {
                    direction: Direction::Horizontal,
                    borderless: false,
                    parts: vec![],
                    tabs: vec![],
                    split_size: Some(SplitSize::Percent(50)),
                    run: None,
                },
                Layout {
                    direction: Direction::Horizontal,
                    borderless: false,
                    parts: vec![
                        Layout {
                            direction: Direction::Vertical,
                            borderless: false,
                            parts: vec![],
                            tabs: vec![],
                            split_size: Some(SplitSize::Percent(50)),
                            run: None,
                        },
                        Layout {
                            direction: Direction::Vertical,
                            borderless: false,
                            parts: vec![],
                            tabs: vec![],
                            split_size: Some(SplitSize::Percent(50)),
                            run: None,
                        },
                    ],
                    tabs: vec![],
                    split_size: None,
                    run: None,
                },
            ],
            tabs: vec![],
            split_size: None,
            run: None,
        }],
        tabs: vec![],
        split_size: None,
        run: None,
    };
    assert_eq!(merged_layout, tab_layout);
}

#[test]
fn three_panes_with_tab_new_tab_is_correct() {
    let path = layout_test_dir("three-panes-with-tab.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.as_ref().unwrap().construct_layout_template();
    let tab_layout = layout_template.construct_tab_layout(None);
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        borderless: false,
        parts: vec![Layout {
            direction: Direction::Horizontal,
            borderless: false,
            parts: vec![],
            tabs: vec![],
            split_size: None,
            run: None,
        }],
        tabs: vec![],
        split_size: None,
        run: None,
    };
    assert_eq!(merged_layout, tab_layout);
}

#[test]
fn three_panes_with_tab_and_default_plugins_is_ok() {
    let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    assert!(layout.is_ok());
}

#[test]
fn three_panes_with_tab_and_default_plugins_has_one_tab() {
    let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.unwrap().construct_layout_template();
    assert_eq!(layout_template.tabs.len(), 1);
}

#[test]
fn three_panes_with_tab_and_default_plugins_one_post_tab() {
    let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.unwrap().construct_layout_template();
    assert_eq!(layout_template.post_tab.len(), 1);
}

#[test]
fn three_panes_with_tab_and_default_plugins_has_pre_tab() {
    let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.unwrap().construct_layout_template();
    assert!(!layout_template.pre_tab.parts.is_empty());
}

#[test]
fn three_panes_with_tab_and_default_plugins_merged_correctly() {
    let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.as_ref().unwrap().construct_layout_template();
    let tab_layout = layout_template.construct_tab_layout(Some(layout_template.tabs[0].clone()));
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        borderless: false,
        parts: vec![
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Fixed(1)),
                run: Some(Run::Plugin(Some("tab-bar".into()))),
            },
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![
                    Layout {
                        direction: Direction::Horizontal,
                        borderless: false,
                        parts: vec![],
                        tabs: vec![],
                        split_size: Some(SplitSize::Percent(50)),
                        run: None,
                    },
                    Layout {
                        direction: Direction::Horizontal,
                        borderless: false,
                        parts: vec![
                            Layout {
                                direction: Direction::Vertical,
                                borderless: false,
                                parts: vec![],
                                tabs: vec![],
                                split_size: Some(SplitSize::Percent(50)),
                                run: None,
                            },
                            Layout {
                                direction: Direction::Vertical,
                                borderless: false,
                                parts: vec![],
                                tabs: vec![],
                                split_size: Some(SplitSize::Percent(50)),
                                run: None,
                            },
                        ],
                        tabs: vec![],
                        split_size: None,
                        run: None,
                    },
                ],
                tabs: vec![],
                split_size: None,
                run: None,
            },
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Fixed(2)),
                run: Some(Run::Plugin(Some("status-bar".into()))),
            },
        ],
        tabs: vec![],
        split_size: None,
        run: None,
    };
    assert_eq!(merged_layout, tab_layout);
}

#[test]
fn three_panes_with_tab_and_default_plugins_new_tab_is_correct() {
    let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.as_ref().unwrap().construct_layout_template();
    let tab_layout = layout_template.construct_tab_layout(None);
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        borderless: false,
        parts: vec![
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Fixed(1)),
                run: Some(Run::Plugin(Some("tab-bar".into()))),
            },
            Layout {
                direction: Direction::Horizontal,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: None,
                run: None,
            },
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Fixed(2)),
                run: Some(Run::Plugin(Some("status-bar".into()))),
            },
        ],
        tabs: vec![],
        split_size: None,
        run: None,
    };
    assert_eq!(merged_layout, tab_layout);
}

#[test]
fn deeply_nested_tab_is_ok() {
    let path = layout_test_dir("deeply-nested-tab-layout.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    assert!(layout.is_ok());
}

#[test]
fn deeply_nested_tab_has_one_tab() {
    let path = layout_test_dir("deeply-nested-tab-layout.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.unwrap().construct_layout_template();
    assert_eq!(layout_template.tabs.len(), 1);
}

#[test]
fn deeply_nested_tab_three_post_tab() {
    let path = layout_test_dir("deeply-nested-tab-layout.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.unwrap().construct_layout_template();
    assert_eq!(layout_template.post_tab.len(), 3);
}

#[test]
fn deeply_nested_tab_has_many_pre_tab() {
    let path = layout_test_dir("deeply-nested-tab-layout.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.unwrap().construct_layout_template();
    assert!(!layout_template.pre_tab.parts.is_empty());
}

#[test]
fn deeply_nested_tab_merged_correctly() {
    let path = layout_test_dir("deeply-nested-tab-layout.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.as_ref().unwrap().construct_layout_template();
    let tab_layout = layout_template.construct_tab_layout(Some(layout_template.tabs[0].clone()));
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        borderless: false,
        parts: vec![
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![
                    Layout {
                        direction: Direction::Horizontal,
                        borderless: false,
                        parts: vec![],
                        tabs: vec![],
                        split_size: Some(SplitSize::Percent(21)),
                        run: None,
                    },
                    Layout {
                        direction: Direction::Vertical,
                        borderless: false,
                        parts: vec![
                            Layout {
                                direction: Direction::Horizontal,
                                borderless: false,
                                parts: vec![],
                                tabs: vec![],
                                split_size: Some(SplitSize::Percent(22)),
                                run: None,
                            },
                            Layout {
                                direction: Direction::Horizontal,
                                borderless: false,
                                parts: vec![Layout {
                                    direction: Direction::Horizontal,
                                    borderless: false,
                                    parts: vec![],
                                    tabs: vec![],
                                    split_size: Some(SplitSize::Percent(23)),
                                    run: None,
                                }],
                                tabs: vec![],
                                split_size: Some(SplitSize::Percent(78)),
                                run: None,
                            },
                        ],
                        tabs: vec![],
                        split_size: Some(SplitSize::Percent(79)),
                        run: None,
                    },
                ],
                tabs: vec![],
                split_size: Some(SplitSize::Percent(90)),
                run: None,
            },
            Layout {
                direction: Direction::Horizontal,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Percent(24)),
                run: None,
            },
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Percent(15)),
                run: None,
            },
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Percent(15)),
                run: None,
            },
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Percent(15)),
                run: None,
            },
        ],
        tabs: vec![],
        split_size: None,
        run: None,
    };
    assert_eq!(merged_layout, tab_layout);
}

#[test]
fn three_tabs_is_ok() {
    let path = layout_test_dir("three-tabs-merged-correctly.yaml".into());
    let layout_from_yaml = LayoutFromYaml::new(&path);
    assert!(layout_from_yaml.is_ok());
}

#[test]
fn three_tabs_has_three_tabs() {
    let path = layout_test_dir("three-tabs-merged-correctly.yaml".into());
    let layout_from_yaml = LayoutFromYaml::new(&path);
    let layout_template = layout_from_yaml.unwrap().construct_layout_template();
    assert_eq!(layout_template.tabs.len(), 3);
}

#[test]
fn three_tabs_has_one_post_tab() {
    let path = layout_test_dir("three-tabs-merged-correctly.yaml".into());
    let layout_from_yaml = LayoutFromYaml::new(&path);
    let layout_template = layout_from_yaml.unwrap().construct_layout_template();
    assert_eq!(layout_template.post_tab.len(), 1);
}

#[test]
fn three_tabs_tab_one_merged_correctly() {
    let path = layout_test_dir("three-tabs-merged-correctly.yaml".into());
    let layout_from_yaml = LayoutFromYaml::new(&path);
    let layout_template = layout_from_yaml
        .as_ref()
        .unwrap()
        .construct_layout_template();
    let tab_layout = layout_template.construct_tab_layout(Some(layout_template.tabs[0].clone()));
    let merged_layout = Layout {
        direction: Direction::Vertical,
        borderless: false,
        parts: vec![
            Layout {
                direction: Direction::Horizontal,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Percent(50)),
                run: None,
            },
            Layout {
                direction: Direction::Horizontal,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: None,
                run: None,
            },
        ],
        tabs: vec![],
        split_size: None,
        run: None,
    };

    assert_eq!(merged_layout, tab_layout);
}

#[test]
fn three_tabs_tab_two_merged_correctly() {
    let path = layout_test_dir("three-tabs-merged-correctly.yaml".into());
    let layout_from_yaml = LayoutFromYaml::new(&path);
    let layout_template = layout_from_yaml
        .as_ref()
        .unwrap()
        .construct_layout_template();
    let tab_layout = layout_template.construct_tab_layout(Some(layout_template.tabs[1].clone()));
    let merged_layout = Layout {
        direction: Direction::Vertical,
        borderless: false,
        parts: vec![
            Layout {
                direction: Direction::Horizontal,
                borderless: false,
                parts: vec![
                    Layout {
                        direction: Direction::Horizontal,
                        borderless: false,
                        parts: vec![],
                        tabs: vec![],
                        split_size: Some(SplitSize::Percent(50)),
                        run: None,
                    },
                    Layout {
                        direction: Direction::Horizontal,
                        borderless: false,
                        parts: vec![],
                        tabs: vec![],
                        split_size: None,
                        run: None,
                    },
                ],
                tabs: vec![],
                split_size: Some(SplitSize::Percent(50)),
                run: None,
            },
            Layout {
                direction: Direction::Horizontal,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: None,
                run: None,
            },
        ],
        tabs: vec![],
        split_size: None,
        run: None,
    };

    assert_eq!(merged_layout, tab_layout);
}

#[test]
fn three_tabs_tab_three_merged_correctly() {
    let path = layout_test_dir("three-tabs-merged-correctly.yaml".into());
    let layout = LayoutFromYaml::new(&path);
    let layout_template = layout.as_ref().unwrap().construct_layout_template();
    let tab_layout = layout_template.construct_tab_layout(Some(layout_template.tabs[2].clone()));
    let merged_layout = Layout {
        direction: Direction::Vertical,
        borderless: false,
        parts: vec![
            Layout {
                direction: Direction::Vertical,
                borderless: false,
                parts: vec![
                    Layout {
                        direction: Direction::Vertical,
                        borderless: false,
                        parts: vec![],
                        tabs: vec![],
                        split_size: Some(SplitSize::Percent(50)),
                        run: None,
                    },
                    Layout {
                        direction: Direction::Horizontal,
                        borderless: false,
                        parts: vec![],
                        tabs: vec![],
                        split_size: None,
                        run: None,
                    },
                ],
                tabs: vec![],
                split_size: Some(SplitSize::Percent(50)),
                run: None,
            },
            Layout {
                direction: Direction::Horizontal,
                borderless: false,
                parts: vec![],
                tabs: vec![],
                split_size: None,
                run: None,
            },
        ],
        tabs: vec![],
        split_size: None,
        run: None,
    };
    assert_eq!(merged_layout, tab_layout);
}
