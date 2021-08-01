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
    let layout = Layout::new(&path);
    assert!(layout.is_ok());
}

#[test]
fn default_layout_has_one_tab() {
    let path = default_layout_dir("default.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.as_ref().unwrap().construct_main_layout();
    assert_eq!(main_layout.tabs.len(), 1);
}

#[test]
fn default_layout_has_one_pre_tab() {
    let path = default_layout_dir("default.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.as_ref().unwrap().construct_main_layout();
    assert_eq!(main_layout.pre_tab.parts.len(), 1);
}

#[test]
fn default_layout_has_one_post_tab() {
    let path = default_layout_dir("default.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.as_ref().unwrap().construct_main_layout();
    assert_eq!(main_layout.post_tab.len(), 1);
}

#[test]
fn default_layout_merged_correctly() {
    let path = default_layout_dir("default.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.as_ref().unwrap().construct_main_layout();
    let tab_layout = main_layout.construct_tab_layout(Some(main_layout.tabs[0].clone()));
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        parts: vec![
            Layout {
                direction: Direction::Vertical,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Fixed(1)),
                run: Some(Run::Plugin(Some("tab-bar".into()))),
            },
            Layout {
                direction: Direction::Vertical,
                parts: vec![],
                tabs: vec![],
                split_size: None,
                run: None,
            },
            Layout {
                direction: Direction::Vertical,
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
    let layout = Layout::new(&path);
    let main_layout = layout.as_ref().unwrap().construct_main_layout();
    let tab_layout = main_layout.construct_tab_layout(None);
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        parts: vec![
            Layout {
                direction: Direction::Vertical,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Fixed(1)),
                run: Some(Run::Plugin(Some("tab-bar".into()))),
            },
            Layout {
                direction: Direction::Horizontal,
                parts: vec![],
                tabs: vec![],
                split_size: None,
                run: None,
            },
            Layout {
                direction: Direction::Vertical,
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
    let layout = Layout::new(&path);
    assert!(layout.is_ok());
}

#[test]
fn default_disable_status_layout_is_ok() {
    let path = default_layout_dir("disable-status-bar.yaml".into());
    let layout = Layout::new(&path);
    assert!(layout.is_ok());
}

#[test]
fn default_disable_status_layout_has_one_tab() {
    let path = default_layout_dir("disable-status-bar.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.as_ref().unwrap().construct_main_layout();
    assert_eq!(main_layout.tabs.len(), 1);
}

#[test]
fn default_disable_status_layout_has_one_pre_tab() {
    let path = default_layout_dir("disable-status-bar.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.as_ref().unwrap().construct_main_layout();
    assert_eq!(main_layout.pre_tab.parts.len(), 1);
}

#[test]
fn default_disable_status_layout_has_no_post_tab() {
    let path = default_layout_dir("disable-status-bar.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.as_ref().unwrap().construct_main_layout();
    assert!(main_layout.post_tab.is_empty());
}

#[test]
fn three_panes_with_tab_is_ok() {
    let path = layout_test_dir("three-panes-with-tab.yaml".into());
    let layout = Layout::new(&path);
    assert!(layout.is_ok());
}

#[test]
fn three_panes_with_tab_has_one_tab() {
    let path = layout_test_dir("three-panes-with-tab.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.unwrap().construct_main_layout();
    assert_eq!(main_layout.tabs.len(), 1);
}

#[test]
fn three_panes_with_tab_no_post_tab() {
    let path = layout_test_dir("three-panes-with-tab.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.unwrap().construct_main_layout();
    assert!(main_layout.post_tab.is_empty());
}

#[test]
fn three_panes_with_tab_no_pre_tab() {
    let path = layout_test_dir("three-panes-with-tab.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.unwrap().construct_main_layout();
    assert!(main_layout.pre_tab.parts.is_empty());
}

#[test]
fn three_panes_with_tab_merged_correctly() {
    let path = layout_test_dir("three-panes-with-tab.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.as_ref().unwrap().construct_main_layout();
    let tab_layout = main_layout.construct_tab_layout(Some(main_layout.tabs[0].clone()));
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        parts: vec![Layout {
            direction: Direction::Vertical,
            parts: vec![
                Layout {
                    direction: Direction::Horizontal,
                    parts: vec![],
                    tabs: vec![],
                    split_size: Some(SplitSize::Percent(50)),
                    run: None,
                },
                Layout {
                    direction: Direction::Horizontal,
                    parts: vec![
                        Layout {
                            direction: Direction::Vertical,
                            parts: vec![],
                            tabs: vec![],
                            split_size: Some(SplitSize::Percent(50)),
                            run: None,
                        },
                        Layout {
                            direction: Direction::Vertical,
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
    let layout = Layout::new(&path);
    let main_layout = layout.as_ref().unwrap().construct_main_layout();
    let tab_layout = main_layout.construct_tab_layout(None);
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        parts: vec![Layout {
            direction: Direction::Horizontal,
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
    let layout = Layout::new(&path);
    assert!(layout.is_ok());
}

#[test]
fn three_panes_with_tab_and_default_plugins_has_one_tab() {
    let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.unwrap().construct_main_layout();
    assert_eq!(main_layout.tabs.len(), 1);
}

#[test]
fn three_panes_with_tab_and_default_plugins_one_post_tab() {
    let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.unwrap().construct_main_layout();
    assert_eq!(main_layout.post_tab.len(), 1);
}

#[test]
fn three_panes_with_tab_and_default_plugins_has_pre_tab() {
    let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.unwrap().construct_main_layout();
    assert!(!main_layout.pre_tab.parts.is_empty());
}

#[test]
fn three_panes_with_tab_and_default_plugins_merged_correctly() {
    let path = layout_test_dir("three-panes-with-tab-and-default-plugins.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.as_ref().unwrap().construct_main_layout();
    let tab_layout = main_layout.construct_tab_layout(Some(main_layout.tabs[0].clone()));
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        parts: vec![
            Layout {
                direction: Direction::Vertical,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Fixed(1)),
                run: Some(Run::Plugin(Some("tab-bar".into()))),
            },
            Layout {
                direction: Direction::Vertical,
                parts: vec![
                    Layout {
                        direction: Direction::Horizontal,
                        parts: vec![],
                        tabs: vec![],
                        split_size: Some(SplitSize::Percent(50)),
                        run: None,
                    },
                    Layout {
                        direction: Direction::Horizontal,
                        parts: vec![
                            Layout {
                                direction: Direction::Vertical,
                                parts: vec![],
                                tabs: vec![],
                                split_size: Some(SplitSize::Percent(50)),
                                run: None,
                            },
                            Layout {
                                direction: Direction::Vertical,
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
    let layout = Layout::new(&path);
    let main_layout = layout.as_ref().unwrap().construct_main_layout();
    let tab_layout = main_layout.construct_tab_layout(None);
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        parts: vec![
            Layout {
                direction: Direction::Vertical,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Fixed(1)),
                run: Some(Run::Plugin(Some("tab-bar".into()))),
            },
            Layout {
                direction: Direction::Horizontal,
                parts: vec![],
                tabs: vec![],
                split_size: None,
                run: None,
            },
            Layout {
                direction: Direction::Vertical,
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
    let layout = Layout::new(&path);
    assert!(layout.is_ok());
}

#[test]
fn deeply_nested_tab_has_one_tab() {
    let path = layout_test_dir("deeply-nested-tab-layout.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.unwrap().construct_main_layout();
    assert_eq!(main_layout.tabs.len(), 1);
}

#[test]
fn deeply_nested_tab_three_post_tab() {
    let path = layout_test_dir("deeply-nested-tab-layout.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.unwrap().construct_main_layout();
    assert_eq!(main_layout.post_tab.len(), 3);
}

#[test]
fn deeply_nested_tab_has_many_pre_tab() {
    let path = layout_test_dir("deeply-nested-tab-layout.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.unwrap().construct_main_layout();
    assert!(!main_layout.pre_tab.parts.is_empty());
}

#[test]
fn deeply_nested_tab_merged_correctly() {
    let path = layout_test_dir("deeply-nested-tab-layout.yaml".into());
    let layout = Layout::new(&path);
    let main_layout = layout.as_ref().unwrap().construct_main_layout();
    let tab_layout = main_layout.construct_tab_layout(Some(main_layout.tabs[0].clone()));
    let merged_layout = Layout {
        direction: Direction::Horizontal,
        parts: vec![
            Layout {
                direction: Direction::Vertical,
                parts: vec![
                    Layout {
                        direction: Direction::Horizontal,
                        parts: vec![],
                        tabs: vec![],
                        split_size: Some(SplitSize::Percent(21)),
                        run: None,
                    },
                    Layout {
                        direction: Direction::Vertical,
                        parts: vec![
                            Layout {
                                direction: Direction::Horizontal,
                                parts: vec![],
                                tabs: vec![],
                                split_size: Some(SplitSize::Percent(22)),
                                run: None,
                            },
                            Layout {
                                direction: Direction::Horizontal,
                                parts: vec![Layout {
                                    direction: Direction::Horizontal,
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
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Percent(24)),
                run: None,
            },
            Layout {
                direction: Direction::Vertical,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Percent(15)),
                run: None,
            },
            Layout {
                direction: Direction::Vertical,
                parts: vec![],
                tabs: vec![],
                split_size: Some(SplitSize::Percent(15)),
                run: None,
            },
            Layout {
                direction: Direction::Vertical,
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
#[should_panic]
// TODO Make error out of this
fn no_tabs_specified_should_panic() {
    let path = layout_test_dir("no-tabs-should-panic.yaml".into());
    let layout = Layout::new(&path);
    let _main_layout = layout.unwrap().construct_main_layout();
}

#[test]
fn multiple_tabs_specified_should_not_panic() {
    let path = layout_test_dir("multiple-tabs-should-panic.yaml".into());
    let layout = Layout::new(&path);
    let _main_layout = layout.unwrap().construct_main_layout();
}
