use super::super::config::Config;
use super::super::config_settings::*;
use super::super::options::{
    Clipboard, HostNotificationProtocol, NestedSessionHandling, OnForceClose, Options,
    PaneFrameStyle,
};
use super::super::theme::{FrameConfig, UiConfig};
use super::super::web_client::{CursorInactiveStyle, CursorStyle, WebClientConfig};
use crate::data::{
    BorderStyleOverride, InputMode, LineStyle, SettingKey, SettingSection, ThemeHue, WebSharing,
};
use kdl::KdlDocument;
use std::collections::BTreeSet;
use std::path::PathBuf;

fn every_option_set() -> Options {
    Options {
        simplified_ui: Some(true),
        theme: Some("dracula".to_owned()),
        theme_dark: Some("nord".to_owned()),
        theme_light: Some("gruvbox-light".to_owned()),
        explicit_theme_hue: Some(ThemeHue::Light),
        default_mode: Some(InputMode::Locked),
        default_shell: Some(PathBuf::from("/bin/fish")),
        default_cwd: Some(PathBuf::from("/tmp/work")),
        default_layout: Some(PathBuf::from("compact")),
        layout_dir: Some(PathBuf::from("/tmp/layouts")),
        theme_dir: Some(PathBuf::from("/tmp/themes")),
        keybinds_dir: Some(PathBuf::from("/tmp/keybinds")),
        keybinds_preset: None,
        keybinds_primary: None,
        keybinds_secondary: None,
        keybinds_unlock: None,
        mouse_mode: Some(false),
        pane_frames: Some(false),
        pane_frame_style: Some(PaneFrameStyle::Full),
        mirror_session: Some(true),
        on_force_close: Some(OnForceClose::Quit),
        scroll_buffer_size: Some(5000),
        copy_command: Some("wl-copy".to_owned()),
        copy_clipboard: Some(Clipboard::Primary),
        copy_on_select: Some(false),
        osc8_hyperlinks: Some(false),
        scrollback_editor: Some(PathBuf::from("/usr/bin/vim")),
        session_name: Some("work".to_owned()),
        attach_to_session: Some(true),
        auto_layout: Some(false),
        session_serialization: Some(false),
        serialize_pane_viewport: Some(true),
        scrollback_lines_to_serialize: Some(1000),
        styled_underlines: Some(false),
        serialization_interval: Some(30),
        disable_session_metadata: Some(true),
        support_kitty_keyboard_protocol: Some(false),
        support_kitty_graphics_protocol: Some(false),
        web_server: Some(true),
        web_sharing: Some(WebSharing::Disabled),
        stacked_resize: Some(false),
        stacked_pane_list: Some(false),
        show_startup_tips: Some(false),
        show_release_notes: Some(false),
        advanced_mouse_actions: Some(false),
        mouse_scroll_resize: Some(false),
        scroll_mode_sync: Some(false),
        mouse_hover_effects: Some(false),
        mouse_hover_tips: Some(false),
        visual_bell: Some(false),
        focus_follows_mouse: Some(true),
        mouse_click_through: Some(true),
        context_menu_enabled: Some(false),
        osc133_command_selection: Some(false),
        word_separators: Some("[]".to_owned()),
        host_notification_protocol: Some(HostNotificationProtocol::Osc99),
        web_server_ip: Some("127.0.0.2".parse().unwrap()),
        web_server_port: Some(9000),
        web_server_cert: Some(PathBuf::from("/tmp/cert.pem")),
        web_server_key: Some(PathBuf::from("/tmp/key.pem")),
        enforce_https_for_localhost: Some(true),
        dangerously_allow_web_serving_without_a_certificate: Some(true),
        post_command_discovery_hook: Some("echo hook".to_owned()),
        client_async_worker_tasks: Some(8),
        nested_session_handling: Some(NestedSessionHandling::Never),
        dangerously_enable_paste_buffer_read: Some(true),
    }
}

fn every_border_override_set(line_style: LineStyle) -> BorderStyleOverride {
    BorderStyleOverride {
        all: Some(line_style),
        top: Some(LineStyle::Double),
        right: Some(LineStyle::Dashed),
        bottom: Some(LineStyle::HeavyDashed),
        left: Some(LineStyle::Single),
        rounded_corners: Some(true),
    }
}

const EVERY_BLOCK_SET: &str = "keybinds preset=\"default\"
mousebinds {
    normal {
        bind \"Alt Middle\" { NewTab; }
    }
}
plugins {
    mine location=\"zellij:strider\" {
        size \"3\"
    }
}
load_plugins {
    \"zellij:strider\"
}
env {
    EDITOR \"vim\"
}
themes {
    mine {
        fg 1
        bg 2
        red 3
        green 4
        blue 5
        yellow 6
        magenta 7
        orange 8
        cyan 9
        black 10
        white 11
    }
}
context_menu {
    pane clear-defaults=true {
        item \"Mine\" { NewTab; }
    }
}
";

pub(crate) fn every_setting_set() -> Config {
    let with_preset = Config::from_kdl(
        EVERY_BLOCK_SET,
        Some(Config::from_default_assets().unwrap()),
    )
    .unwrap();
    Config {
        keybinds: with_preset.keybinds.clone(),
        keybinds_layers: with_preset.keybinds_layers,
        mousebinds: with_preset.mousebinds.clone(),
        plugins: with_preset.plugins,
        background_plugins: with_preset.background_plugins,
        env: with_preset.env,
        themes: with_preset.themes,
        context_menu: with_preset.context_menu,
        options: every_option_set(),
        ui: UiConfig {
            pane_frames: FrameConfig {
                rounded_corners: true,
                hide_session_name: true,
                border_style: every_border_override_set(LineStyle::Heavy),
                floating_border_style: every_border_override_set(LineStyle::Double),
            },
        },
        web_client: WebClientConfig {
            font: "Fira Code".to_owned(),
            theme: None,
            cursor_blink: true,
            cursor_inactive_style: Some(CursorInactiveStyle::NoStyle),
            cursor_style: Some(CursorStyle::Underline),
            mac_option_is_meta: false,
            base_url: Some("/zellij".to_owned()),
            font_size: Some(14),
        },
        ..Default::default()
    }
}

fn kdl_names_in(config: &Config) -> BTreeSet<(SettingSection, String)> {
    let document: KdlDocument = config.to_string(false).parse().unwrap();
    let blocks = [
        "keybinds",
        "mousebinds",
        "themes",
        "plugins",
        "load_plugins",
        "ui",
        "env",
        "web_client",
        "context_menu",
    ];
    let mut names = BTreeSet::new();
    for node in document.nodes() {
        let name = node.name().value();
        if !blocks.contains(&name) {
            names.insert((SettingSection::TopLevel, name.to_owned()));
        } else if !["keybinds", "mousebinds", "ui", "web_client"].contains(&name) {
            names.insert((SettingSection::Blocks, name.to_owned()));
        }
    }
    names.insert((SettingSection::Keybinds, "keybinds".to_owned()));
    names.insert((SettingSection::Keybinds, "mousebinds".to_owned()));
    let pane_frames = document
        .get("ui")
        .and_then(|ui| ui.children())
        .and_then(|ui| ui.get("pane_frames"))
        .and_then(|pane_frames| pane_frames.children())
        .unwrap();
    for node in pane_frames.nodes() {
        names.insert((SettingSection::PaneFrames, node.name().value().to_owned()));
    }
    let web_client = document
        .get("web_client")
        .and_then(|web_client| web_client.children())
        .unwrap();
    for node in web_client.nodes() {
        names.insert((SettingSection::WebClient, node.name().value().to_owned()));
    }
    names
}

#[test]
fn every_setting_in_the_config_file_has_exactly_one_setting_key() {
    let from_config_file = kdl_names_in(&every_setting_set());
    let from_setting_keys: BTreeSet<(SettingSection, String)> = SettingKey::all()
        .into_iter()
        .map(|key| (key.section(), key.kdl_name().to_owned()))
        .collect();
    assert_eq!(from_setting_keys.len(), SettingKey::all().len());
    assert_eq!(from_config_file, from_setting_keys);
}

#[test]
fn every_setting_key_has_a_unique_id_that_reads_back() {
    let ids: BTreeSet<String> = SettingKey::all().iter().map(|key| key.id()).collect();
    assert_eq!(ids.len(), SettingKey::all().len());
    for key in SettingKey::all() {
        assert_eq!(SettingKey::from_id(&key.id()), Some(key));
    }
}

#[test]
fn every_setting_has_a_value_when_set_and_reads_back_from_the_file() {
    let config = every_setting_set();
    let values = setting_values(&config);
    for key in SettingKey::all() {
        assert!(values.contains_key(&key), "{} has no value", key);
        if key != SettingKey::Keybinds && key != SettingKey::Mousebinds {
            assert!(values[&key].is_some(), "{} has no value", key);
        }
    }
    let read_back = Config::from_kdl(
        &config.to_string(false),
        Some(Config::from_default_assets().unwrap()),
    )
    .unwrap();
    assert_eq!(setting_values(&read_back), values);
    assert!(!settings_differ(&read_back, &config, SettingKey::Keybinds));
    assert!(!settings_differ(
        &read_back,
        &config,
        SettingKey::Mousebinds
    ));
}

#[test]
fn every_setting_is_found_in_a_file_that_sets_it() {
    let file_contents = every_setting_set().to_string(false);
    let set_in_file = settings_set_in_file(&file_contents);
    assert_eq!(
        set_in_file,
        SettingKey::all().into_iter().collect::<BTreeSet<_>>()
    );
    assert!(settings_set_in_file("").is_empty());
    let partial = settings_set_in_file("mouse_mode false\nweb_client {\n font \"x\"\n}\n");
    assert_eq!(
        partial,
        [SettingKey::MouseMode, SettingKey::WebClientFont]
            .into_iter()
            .collect::<BTreeSet<_>>()
    );
}

#[test]
fn copying_a_setting_changes_only_that_setting() {
    let source = every_setting_set();
    let default_values = setting_values(&Config::default());
    for key in SettingKey::all() {
        let mut target = Config::default();
        copy_setting(&mut target, &source, key);
        assert!(
            !settings_differ(&target, &source, key),
            "{} was not copied",
            key
        );
        let target_values = setting_values(&target);
        for other in SettingKey::all() {
            if other != key {
                assert_eq!(
                    target_values[&other], default_values[&other],
                    "copying {} changed {}",
                    key, other
                );
            }
        }
        assert_eq!(differing_settings(&target, &Config::default()), vec![key]);
    }
}

#[test]
fn unsetting_a_setting_restores_its_default() {
    let default_values = setting_values(&Config::default());
    for key in SettingKey::all() {
        let mut config = every_setting_set();
        unset_setting(&mut config, key);
        if key == SettingKey::Keybinds {
            assert!(config.keybinds_layers.user.is_empty());
            assert_eq!(
                config.keybinds,
                Config::from_default_assets().unwrap().keybinds
            );
        } else if key == SettingKey::Mousebinds {
            assert!(config.keybinds_layers.user_mouse.is_empty());
            assert_eq!(
                config.mousebinds,
                Config::from_default_assets().unwrap().mousebinds
            );
        } else {
            assert_eq!(setting_value(&config, key), default_values[&key].clone());
        }
    }
}

#[test]
fn a_partial_ui_or_web_client_block_keeps_the_other_values() {
    let base = every_setting_set();
    let changed = Config::from_kdl(
        "ui {\n pane_frames {\n border_top \"single\"\n }\n}\nweb_client {\n font_size 20\n}\n",
        Some(base.clone()),
    )
    .unwrap();
    assert_eq!(
        differing_settings(&base, &changed),
        vec![SettingKey::FrameBorderTop, SettingKey::WebClientFontSize]
    );
}
