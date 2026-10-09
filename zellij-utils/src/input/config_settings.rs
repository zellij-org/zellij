use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use kdl::{KdlDocument, KdlNode, KdlValue};

use super::config::Config;
use super::config_blocks::copy_config_file_themes;
use super::options::{
    Clipboard, HostNotificationProtocol, NestedSessionHandling, OnForceClose, Options,
    PaneFrameStyle,
};
use super::theme::{FrameConfig, UiConfig};
use super::web_client::{CursorInactiveStyle, CursorStyle, WebClientConfig};
use super::window::WindowConfig;
use crate::data::{
    join_setting_list, BorderStyleOverride, ConfigSettingState, ContextMenuEntry, InputMode,
    LineStyle, SettingKey, SettingValueShape, ThemeHue, WebSharing,
};
use crate::kdl::load_plugins_to_kdl;

fn path_text(path: &Option<PathBuf>) -> Option<String> {
    path.as_ref().map(|p| p.display().to_string())
}

fn display_text<T: ToString>(value: &Option<T>) -> Option<String> {
    value.as_ref().map(|v| v.to_string())
}

fn input_mode_text(mode: &InputMode) -> String {
    format!("{:?}", mode).to_lowercase()
}

fn theme_hue_text(hue: &ThemeHue) -> String {
    hue.to_string()
}

fn pane_frame_style_text(style: &PaneFrameStyle) -> String {
    match style {
        PaneFrameStyle::Full => "full",
        PaneFrameStyle::Titles => "titles",
        PaneFrameStyle::None => "none",
    }
    .to_owned()
}

fn on_force_close_text(value: &OnForceClose) -> String {
    match value {
        OnForceClose::Quit => "quit",
        OnForceClose::Detach => "detach",
    }
    .to_owned()
}

fn clipboard_text(value: &Clipboard) -> String {
    match value {
        Clipboard::System => "system",
        Clipboard::Primary => "primary",
    }
    .to_owned()
}

fn web_sharing_text(value: &WebSharing) -> String {
    match value {
        WebSharing::On => "on",
        WebSharing::Off => "off",
        WebSharing::Disabled => "disabled",
    }
    .to_owned()
}

fn host_notification_protocol_text(value: &HostNotificationProtocol) -> String {
    value.as_str().to_owned()
}

fn nested_session_handling_text(value: &NestedSessionHandling) -> String {
    match value {
        NestedSessionHandling::Ask => "ask",
        NestedSessionHandling::Fullscreen => "fullscreen",
        NestedSessionHandling::Descend => "descend",
        NestedSessionHandling::Never => "never",
    }
    .to_owned()
}

fn line_style_text(value: &LineStyle) -> String {
    value.to_string()
}

fn cursor_style_text(value: &CursorStyle) -> String {
    value.to_string()
}

fn cursor_inactive_style_text(value: &CursorInactiveStyle) -> String {
    match value {
        CursorInactiveStyle::Outline => "outline",
        CursorInactiveStyle::Block => "block",
        CursorInactiveStyle::Bar => "bar",
        CursorInactiveStyle::Underline => "underline",
        CursorInactiveStyle::NoStyle => "no_style",
    }
    .to_owned()
}

fn option_values(options: &Options, values: &mut BTreeMap<SettingKey, Option<String>>) {
    let Options {
        simplified_ui,
        theme,
        theme_dark,
        theme_light,
        explicit_theme_hue,
        default_mode,
        default_shell,
        default_cwd,
        default_layout,
        layout_dir,
        theme_dir,
        keybinds_dir,
        keybinds_preset: _keybinds_preset_is_command_line_only,
        keybinds_primary: _keybinds_primary_is_command_line_only,
        keybinds_secondary: _keybinds_secondary_is_command_line_only,
        keybinds_unlock: _keybinds_unlock_is_command_line_only,
        mouse_mode,
        pane_frames,
        pane_frame_style,
        mirror_session,
        on_force_close,
        scroll_buffer_size,
        copy_command,
        copy_clipboard,
        copy_on_select,
        osc8_hyperlinks,
        scrollback_editor,
        session_name,
        attach_to_session,
        auto_layout,
        session_serialization,
        serialize_pane_viewport,
        scrollback_lines_to_serialize,
        styled_underlines,
        serialization_interval,
        disable_session_metadata,
        support_kitty_keyboard_protocol,
        support_kitty_graphics_protocol,
        web_server,
        web_sharing,
        stacked_resize,
        stacked_pane_list,
        show_startup_tips,
        show_release_notes,
        advanced_mouse_actions,
        mouse_scroll_resize,
        scroll_mode_sync,
        mouse_hover_effects,
        mouse_hover_tips,
        visual_bell,
        focus_follows_mouse,
        mouse_click_through,
        context_menu_enabled,
        osc133_command_selection,
        word_separators,
        host_notification_protocol,
        web_server_ip,
        web_server_port,
        web_server_cert,
        web_server_key,
        enforce_https_for_localhost,
        post_command_discovery_hook,
        client_async_worker_tasks,
        nested_session_handling,
        dangerously_enable_paste_buffer_read,
        session_card,
        session_indicator,
        swap_layout_notification,
        on_quit,
    } = options;
    let entries = [
        (SettingKey::SimplifiedUi, display_text(simplified_ui)),
        (SettingKey::Theme, theme.clone()),
        (SettingKey::ThemeDark, theme_dark.clone()),
        (SettingKey::ThemeLight, theme_light.clone()),
        (
            SettingKey::ExplicitThemeHue,
            explicit_theme_hue.as_ref().map(theme_hue_text),
        ),
        (
            SettingKey::DefaultMode,
            default_mode.as_ref().map(input_mode_text),
        ),
        (SettingKey::DefaultShell, path_text(default_shell)),
        (SettingKey::DefaultCwd, path_text(default_cwd)),
        (SettingKey::DefaultLayout, path_text(default_layout)),
        (SettingKey::LayoutDir, path_text(layout_dir)),
        (SettingKey::ThemeDir, path_text(theme_dir)),
        (SettingKey::KeybindsDir, path_text(keybinds_dir)),
        (SettingKey::MouseMode, display_text(mouse_mode)),
        (SettingKey::PaneFrames, display_text(pane_frames)),
        (
            SettingKey::PaneFrameStyle,
            pane_frame_style.as_ref().map(pane_frame_style_text),
        ),
        (SettingKey::MirrorSession, display_text(mirror_session)),
        (
            SettingKey::OnForceClose,
            on_force_close.as_ref().map(on_force_close_text),
        ),
        (
            SettingKey::ScrollBufferSize,
            display_text(scroll_buffer_size),
        ),
        (SettingKey::CopyCommand, copy_command.clone()),
        (
            SettingKey::CopyClipboard,
            copy_clipboard.as_ref().map(clipboard_text),
        ),
        (SettingKey::CopyOnSelect, display_text(copy_on_select)),
        (SettingKey::Osc8Hyperlinks, display_text(osc8_hyperlinks)),
        (SettingKey::ScrollbackEditor, path_text(scrollback_editor)),
        (SettingKey::SessionName, session_name.clone()),
        (SettingKey::AttachToSession, display_text(attach_to_session)),
        (SettingKey::AutoLayout, display_text(auto_layout)),
        (
            SettingKey::SessionSerialization,
            display_text(session_serialization),
        ),
        (
            SettingKey::SerializePaneViewport,
            display_text(serialize_pane_viewport),
        ),
        (
            SettingKey::ScrollbackLinesToSerialize,
            display_text(scrollback_lines_to_serialize),
        ),
        (
            SettingKey::StyledUnderlines,
            display_text(styled_underlines),
        ),
        (
            SettingKey::SerializationInterval,
            display_text(serialization_interval),
        ),
        (
            SettingKey::DisableSessionMetadata,
            display_text(disable_session_metadata),
        ),
        (
            SettingKey::SupportKittyKeyboardProtocol,
            display_text(support_kitty_keyboard_protocol),
        ),
        (
            SettingKey::SupportKittyGraphicsProtocol,
            display_text(support_kitty_graphics_protocol),
        ),
        (SettingKey::WebServer, display_text(web_server)),
        (
            SettingKey::WebSharing,
            web_sharing.as_ref().map(web_sharing_text),
        ),
        (SettingKey::StackedResize, display_text(stacked_resize)),
        (SettingKey::StackedPaneList, display_text(stacked_pane_list)),
        (SettingKey::ShowStartupTips, display_text(show_startup_tips)),
        (
            SettingKey::ShowReleaseNotes,
            display_text(show_release_notes),
        ),
        (
            SettingKey::AdvancedMouseActions,
            display_text(advanced_mouse_actions),
        ),
        (
            SettingKey::MouseScrollResize,
            display_text(mouse_scroll_resize),
        ),
        (SettingKey::ScrollModeSync, display_text(scroll_mode_sync)),
        (
            SettingKey::MouseHoverEffects,
            display_text(mouse_hover_effects),
        ),
        (SettingKey::MouseHoverTips, display_text(mouse_hover_tips)),
        (SettingKey::VisualBell, display_text(visual_bell)),
        (
            SettingKey::FocusFollowsMouse,
            display_text(focus_follows_mouse),
        ),
        (
            SettingKey::MouseClickThrough,
            display_text(mouse_click_through),
        ),
        (
            SettingKey::ContextMenuEnabled,
            display_text(context_menu_enabled),
        ),
        (
            SettingKey::Osc133CommandSelection,
            display_text(osc133_command_selection),
        ),
        (SettingKey::WordSeparators, word_separators.clone()),
        (
            SettingKey::HostNotificationProtocol,
            host_notification_protocol
                .as_ref()
                .map(host_notification_protocol_text),
        ),
        (SettingKey::WebServerIp, display_text(web_server_ip)),
        (SettingKey::WebServerPort, display_text(web_server_port)),
        (SettingKey::WebServerCert, path_text(web_server_cert)),
        (SettingKey::WebServerKey, path_text(web_server_key)),
        (
            SettingKey::EnforceHttpsForLocalhost,
            display_text(enforce_https_for_localhost),
        ),
        (
            SettingKey::PostCommandDiscoveryHook,
            post_command_discovery_hook.clone(),
        ),
        (
            SettingKey::ClientAsyncWorkerTasks,
            display_text(client_async_worker_tasks),
        ),
        (
            SettingKey::NestedSessionHandling,
            nested_session_handling
                .as_ref()
                .map(nested_session_handling_text),
        ),
        (
            SettingKey::DangerouslyEnablePasteBufferRead,
            display_text(dangerously_enable_paste_buffer_read),
        ),
        (SettingKey::SessionCard, display_text(session_card)),
        (
            SettingKey::SessionIndicator,
            display_text(session_indicator),
        ),
        (
            SettingKey::SwapLayoutNotification,
            display_text(swap_layout_notification),
        ),
        (
            SettingKey::OnQuit,
            on_quit.map(|on_quit| on_quit.as_str().to_owned()),
        ),
    ];
    values.extend(entries);
}

fn border_override_values(
    border_style_override: &BorderStyleOverride,
    keys: [SettingKey; 6],
    values: &mut BTreeMap<SettingKey, Option<String>>,
) {
    let BorderStyleOverride {
        all,
        top,
        right,
        bottom,
        left,
        rounded_corners,
    } = border_style_override;
    let [all_key, top_key, right_key, bottom_key, left_key, rounded_corners_key] = keys;
    values.insert(all_key, all.as_ref().map(line_style_text));
    values.insert(top_key, top.as_ref().map(line_style_text));
    values.insert(right_key, right.as_ref().map(line_style_text));
    values.insert(bottom_key, bottom.as_ref().map(line_style_text));
    values.insert(left_key, left.as_ref().map(line_style_text));
    values.insert(rounded_corners_key, display_text(rounded_corners));
}

const BORDER_KEYS: [SettingKey; 6] = [
    SettingKey::FrameBorderStyle,
    SettingKey::FrameBorderTop,
    SettingKey::FrameBorderRight,
    SettingKey::FrameBorderBottom,
    SettingKey::FrameBorderLeft,
    SettingKey::FrameBorderRoundedCorners,
];

const FLOATING_BORDER_KEYS: [SettingKey; 6] = [
    SettingKey::FrameFloatingBorderStyle,
    SettingKey::FrameFloatingBorderTop,
    SettingKey::FrameFloatingBorderRight,
    SettingKey::FrameFloatingBorderBottom,
    SettingKey::FrameFloatingBorderLeft,
    SettingKey::FrameFloatingBorderRoundedCorners,
];

fn ui_values(ui: &UiConfig, values: &mut BTreeMap<SettingKey, Option<String>>) {
    let UiConfig { pane_frames } = ui;
    let FrameConfig {
        rounded_corners,
        hide_session_name,
        border_style,
        floating_border_style,
    } = pane_frames;
    values.insert(
        SettingKey::FrameRoundedCorners,
        Some(rounded_corners.to_string()),
    );
    values.insert(
        SettingKey::FrameHideSessionName,
        Some(hide_session_name.to_string()),
    );
    border_override_values(border_style, BORDER_KEYS, values);
    border_override_values(floating_border_style, FLOATING_BORDER_KEYS, values);
}

fn web_client_values(
    web_client: &WebClientConfig,
    values: &mut BTreeMap<SettingKey, Option<String>>,
) {
    let WebClientConfig {
        font,
        theme: _web_client_theme_colours_are_read_only,
        cursor_blink,
        cursor_inactive_style,
        cursor_style,
        mac_option_is_meta,
        base_url,
        font_size,
    } = web_client;
    values.insert(SettingKey::WebClientFont, Some(font.clone()));
    values.insert(SettingKey::WebClientFontSize, display_text(font_size));
    values.insert(
        SettingKey::WebClientCursorBlink,
        Some(cursor_blink.to_string()),
    );
    values.insert(
        SettingKey::WebClientCursorStyle,
        cursor_style.as_ref().map(cursor_style_text),
    );
    values.insert(
        SettingKey::WebClientCursorInactiveStyle,
        cursor_inactive_style
            .as_ref()
            .map(cursor_inactive_style_text),
    );
    values.insert(
        SettingKey::WebClientMacOptionIsMeta,
        Some(mac_option_is_meta.to_string()),
    );
    values.insert(SettingKey::WebClientBaseUrl, base_url.clone());
}

fn window_document(window: &WindowConfig) -> KdlDocument {
    let mut document = KdlDocument::new();
    let node = window.to_kdl().unwrap_or_else(|| {
        let mut node = KdlNode::new("window");
        node.set_children(KdlDocument::new());
        node
    });
    document.nodes_mut().push(node);
    document
}

fn float_text(number: f64) -> String {
    (number as f32).to_string()
}

fn kdl_value_text(value: &KdlValue) -> Option<String> {
    match value {
        KdlValue::Bool(flag) => Some(flag.to_string()),
        KdlValue::Base10Float(number) => Some(float_text(*number)),
        KdlValue::String(text) | KdlValue::RawString(text) => Some(text.clone()),
        other => other.as_i64().map(|number| number.to_string()),
    }
}

fn window_node_text(key: SettingKey, node: &KdlNode) -> Option<String> {
    let arguments: Vec<&KdlValue> = node
        .entries()
        .iter()
        .filter(|entry| entry.name().is_none())
        .map(|entry| entry.value())
        .collect();
    match key.value_shape() {
        SettingValueShape::List => {
            let items: Vec<String> = arguments.iter().filter_map(|v| kdl_value_text(v)).collect();
            Some(join_setting_list(&items))
        },
        SettingValueShape::Colour => {
            let channels: Vec<i64> = arguments.iter().filter_map(|v| v.as_i64()).collect();
            if arguments.len() == 3 && channels.len() == 3 {
                Some(format!(
                    "#{:02x}{:02x}{:02x}",
                    channels[0], channels[1], channels[2]
                ))
            } else {
                arguments.first().and_then(|value| kdl_value_text(value))
            }
        },
        _ => arguments.first().and_then(|value| kdl_value_text(value)),
    }
}

fn window_values(window: &WindowConfig, values: &mut BTreeMap<SettingKey, Option<String>>) {
    let document = window_document(window);
    for key in SettingKey::all().into_iter().filter(|key| key.is_window()) {
        let text =
            setting_node_in_document(&document, key).and_then(|node| window_node_text(key, node));
        values.insert(key, text);
    }
}

fn copy_window_setting(target: &mut WindowConfig, source: &WindowConfig, key: SettingKey) {
    let source_document = window_document(source);
    let mut document = window_document(target);
    let (parents, name) = setting_node_path(key);
    let mut block = &mut document;
    for parent in parents {
        let position = match block
            .nodes()
            .iter()
            .position(|n| n.name().value() == *parent)
        {
            Some(position) => position,
            None => {
                let mut node = KdlNode::new(*parent);
                node.set_children(KdlDocument::new());
                block.nodes_mut().push(node);
                block.nodes().len() - 1
            },
        };
        let node = &mut block.nodes_mut()[position];
        if node.children().is_none() {
            node.set_children(KdlDocument::new());
        }
        block = match node.children_mut().as_mut() {
            Some(children) => children,
            None => return,
        };
    }
    block.nodes_mut().retain(|node| node.name().value() != name);
    if let Some(node) = setting_node_in_document(&source_document, key) {
        block.nodes_mut().push(node.clone());
    }
    match document
        .get("window")
        .map(WindowConfig::from_kdl)
        .transpose()
    {
        Ok(Some(window)) => *target = window,
        Ok(None) => {},
        Err(e) => log::error!("Failed to copy the {} setting: {}", key, e),
    }
}

pub fn setting_values(config: &Config) -> BTreeMap<SettingKey, Option<String>> {
    let Config {
        keybinds: _keybinds_are_compared_directly,
        options,
        themes,
        plugins,
        ui,
        env,
        background_plugins,
        web_client,
        context_menu,
        keybinds_layers: _keybinds_layers_are_compared_directly,
        window,
        session_suggestions: _session_suggestions_are_read_by_the_server,
    } = config;
    let mut values = BTreeMap::new();
    option_values(options, &mut values);
    window_values(window, &mut values);
    let injected_default_mode = config.keybinds_layers.injected_default_mode;
    if injected_default_mode.is_some() && options.default_mode == injected_default_mode {
        values.insert(
            SettingKey::DefaultMode,
            config
                .keybinds_layers
                .config_default_mode
                .as_ref()
                .map(input_mode_text),
        );
    }
    ui_values(ui, &mut values);
    web_client_values(web_client, &mut values);
    values.insert(SettingKey::Keybinds, None);
    values.insert(
        SettingKey::PluginAliases,
        Some(plugins.to_kdl(false).to_string()),
    );
    values.insert(
        SettingKey::LoadPlugins,
        Some(load_plugins_to_kdl(background_plugins, false).to_string()),
    );
    values.insert(
        SettingKey::Env,
        Some(
            env.to_kdl()
                .map(|node| node.to_string())
                .unwrap_or_default(),
        ),
    );
    values.insert(
        SettingKey::Themes,
        Some(
            themes
                .to_kdl()
                .map(|node| node.to_string())
                .unwrap_or_default(),
        ),
    );
    values.insert(
        SettingKey::ContextMenu,
        Some(
            context_menu
                .to_kdl()
                .map(|node| node.to_string())
                .unwrap_or_default(),
        ),
    );
    values
}

pub fn setting_value(config: &Config, key: SettingKey) -> Option<String> {
    setting_values(config).remove(&key).flatten()
}

pub fn keybinds_differ(first: &Config, second: &Config) -> bool {
    first.keybinds_layers.user != second.keybinds_layers.user
}

pub fn settings_differ(first: &Config, second: &Config, key: SettingKey) -> bool {
    match key {
        SettingKey::Keybinds => keybinds_differ(first, second),
        _ => setting_value(first, key) != setting_value(second, key),
    }
}

pub fn differing_settings(first: &Config, second: &Config) -> Vec<SettingKey> {
    let first_values = setting_values(first);
    let second_values = setting_values(second);
    SettingKey::all()
        .into_iter()
        .filter(|key| match key {
            SettingKey::Keybinds => keybinds_differ(first, second),
            _ => first_values.get(key) != second_values.get(key),
        })
        .collect()
}

fn copy_border_override(
    target: &mut BorderStyleOverride,
    source: &BorderStyleOverride,
    index: usize,
) {
    match index {
        0 => target.all = source.all,
        1 => target.top = source.top,
        2 => target.right = source.right,
        3 => target.bottom = source.bottom,
        4 => target.left = source.left,
        _ => target.rounded_corners = source.rounded_corners,
    }
}

pub fn copy_setting(target: &mut Config, source: &Config, key: SettingKey) {
    let options = &mut target.options;
    let from = &source.options;
    let frames = &mut target.ui.pane_frames;
    let from_frames = &source.ui.pane_frames;
    let web = &mut target.web_client;
    let from_web = &source.web_client;
    match key {
        SettingKey::SimplifiedUi => options.simplified_ui = from.simplified_ui,
        SettingKey::Theme => options.theme = from.theme.clone(),
        SettingKey::ThemeDark => options.theme_dark = from.theme_dark.clone(),
        SettingKey::ThemeLight => options.theme_light = from.theme_light.clone(),
        SettingKey::ExplicitThemeHue => options.explicit_theme_hue = from.explicit_theme_hue,
        SettingKey::DefaultMode => {
            options.default_mode = from.default_mode;
            target.keybinds_layers.config_default_mode = source.keybinds_layers.config_default_mode;
            target.keybinds_layers.injected_default_mode =
                source.keybinds_layers.injected_default_mode;
        },
        SettingKey::DefaultShell => options.default_shell = from.default_shell.clone(),
        SettingKey::DefaultCwd => options.default_cwd = from.default_cwd.clone(),
        SettingKey::DefaultLayout => options.default_layout = from.default_layout.clone(),
        SettingKey::LayoutDir => options.layout_dir = from.layout_dir.clone(),
        SettingKey::ThemeDir => options.theme_dir = from.theme_dir.clone(),
        SettingKey::KeybindsDir => options.keybinds_dir = from.keybinds_dir.clone(),
        SettingKey::MouseMode => options.mouse_mode = from.mouse_mode,
        SettingKey::PaneFrames => options.pane_frames = from.pane_frames,
        SettingKey::PaneFrameStyle => options.pane_frame_style = from.pane_frame_style,
        SettingKey::MirrorSession => options.mirror_session = from.mirror_session,
        SettingKey::OnForceClose => options.on_force_close = from.on_force_close,
        SettingKey::ScrollBufferSize => options.scroll_buffer_size = from.scroll_buffer_size,
        SettingKey::CopyCommand => options.copy_command = from.copy_command.clone(),
        SettingKey::CopyClipboard => options.copy_clipboard = from.copy_clipboard,
        SettingKey::CopyOnSelect => options.copy_on_select = from.copy_on_select,
        SettingKey::Osc8Hyperlinks => options.osc8_hyperlinks = from.osc8_hyperlinks,
        SettingKey::ScrollbackEditor => options.scrollback_editor = from.scrollback_editor.clone(),
        SettingKey::SessionName => options.session_name = from.session_name.clone(),
        SettingKey::AttachToSession => options.attach_to_session = from.attach_to_session,
        SettingKey::AutoLayout => options.auto_layout = from.auto_layout,
        SettingKey::SessionSerialization => {
            options.session_serialization = from.session_serialization
        },
        SettingKey::SerializePaneViewport => {
            options.serialize_pane_viewport = from.serialize_pane_viewport
        },
        SettingKey::ScrollbackLinesToSerialize => {
            options.scrollback_lines_to_serialize = from.scrollback_lines_to_serialize
        },
        SettingKey::StyledUnderlines => options.styled_underlines = from.styled_underlines,
        SettingKey::SerializationInterval => {
            options.serialization_interval = from.serialization_interval
        },
        SettingKey::DisableSessionMetadata => {
            options.disable_session_metadata = from.disable_session_metadata
        },
        SettingKey::SupportKittyKeyboardProtocol => {
            options.support_kitty_keyboard_protocol = from.support_kitty_keyboard_protocol
        },
        SettingKey::SupportKittyGraphicsProtocol => {
            options.support_kitty_graphics_protocol = from.support_kitty_graphics_protocol
        },
        SettingKey::WebServer => options.web_server = from.web_server,
        SettingKey::WebSharing => options.web_sharing = from.web_sharing,
        SettingKey::StackedResize => options.stacked_resize = from.stacked_resize,
        SettingKey::StackedPaneList => options.stacked_pane_list = from.stacked_pane_list,
        SettingKey::ShowStartupTips => options.show_startup_tips = from.show_startup_tips,
        SettingKey::ShowReleaseNotes => options.show_release_notes = from.show_release_notes,
        SettingKey::AdvancedMouseActions => {
            options.advanced_mouse_actions = from.advanced_mouse_actions
        },
        SettingKey::MouseScrollResize => options.mouse_scroll_resize = from.mouse_scroll_resize,
        SettingKey::ScrollModeSync => options.scroll_mode_sync = from.scroll_mode_sync,
        SettingKey::MouseHoverEffects => options.mouse_hover_effects = from.mouse_hover_effects,
        SettingKey::MouseHoverTips => options.mouse_hover_tips = from.mouse_hover_tips,
        SettingKey::VisualBell => options.visual_bell = from.visual_bell,
        SettingKey::FocusFollowsMouse => options.focus_follows_mouse = from.focus_follows_mouse,
        SettingKey::MouseClickThrough => options.mouse_click_through = from.mouse_click_through,
        SettingKey::ContextMenuEnabled => options.context_menu_enabled = from.context_menu_enabled,
        SettingKey::Osc133CommandSelection => {
            options.osc133_command_selection = from.osc133_command_selection
        },
        SettingKey::WordSeparators => options.word_separators = from.word_separators.clone(),
        SettingKey::HostNotificationProtocol => {
            options.host_notification_protocol = from.host_notification_protocol
        },
        SettingKey::WebServerIp => options.web_server_ip = from.web_server_ip,
        SettingKey::WebServerPort => options.web_server_port = from.web_server_port,
        SettingKey::WebServerCert => options.web_server_cert = from.web_server_cert.clone(),
        SettingKey::WebServerKey => options.web_server_key = from.web_server_key.clone(),
        SettingKey::EnforceHttpsForLocalhost => {
            options.enforce_https_for_localhost = from.enforce_https_for_localhost
        },
        SettingKey::PostCommandDiscoveryHook => {
            options.post_command_discovery_hook = from.post_command_discovery_hook.clone()
        },
        SettingKey::ClientAsyncWorkerTasks => {
            options.client_async_worker_tasks = from.client_async_worker_tasks
        },
        SettingKey::NestedSessionHandling => {
            options.nested_session_handling = from.nested_session_handling
        },
        SettingKey::DangerouslyEnablePasteBufferRead => {
            options.dangerously_enable_paste_buffer_read = from.dangerously_enable_paste_buffer_read
        },
        SettingKey::SessionCard => options.session_card = from.session_card,
        SettingKey::SessionIndicator => options.session_indicator = from.session_indicator,
        SettingKey::SwapLayoutNotification => {
            options.swap_layout_notification = from.swap_layout_notification
        },
        SettingKey::OnQuit => options.on_quit = from.on_quit,
        SettingKey::FrameRoundedCorners => frames.rounded_corners = from_frames.rounded_corners,
        SettingKey::FrameHideSessionName => {
            frames.hide_session_name = from_frames.hide_session_name
        },
        SettingKey::FrameBorderStyle => {
            copy_border_override(&mut frames.border_style, &from_frames.border_style, 0)
        },
        SettingKey::FrameBorderTop => {
            copy_border_override(&mut frames.border_style, &from_frames.border_style, 1)
        },
        SettingKey::FrameBorderRight => {
            copy_border_override(&mut frames.border_style, &from_frames.border_style, 2)
        },
        SettingKey::FrameBorderBottom => {
            copy_border_override(&mut frames.border_style, &from_frames.border_style, 3)
        },
        SettingKey::FrameBorderLeft => {
            copy_border_override(&mut frames.border_style, &from_frames.border_style, 4)
        },
        SettingKey::FrameBorderRoundedCorners => {
            copy_border_override(&mut frames.border_style, &from_frames.border_style, 5)
        },
        SettingKey::FrameFloatingBorderStyle => copy_border_override(
            &mut frames.floating_border_style,
            &from_frames.floating_border_style,
            0,
        ),
        SettingKey::FrameFloatingBorderTop => copy_border_override(
            &mut frames.floating_border_style,
            &from_frames.floating_border_style,
            1,
        ),
        SettingKey::FrameFloatingBorderRight => copy_border_override(
            &mut frames.floating_border_style,
            &from_frames.floating_border_style,
            2,
        ),
        SettingKey::FrameFloatingBorderBottom => copy_border_override(
            &mut frames.floating_border_style,
            &from_frames.floating_border_style,
            3,
        ),
        SettingKey::FrameFloatingBorderLeft => copy_border_override(
            &mut frames.floating_border_style,
            &from_frames.floating_border_style,
            4,
        ),
        SettingKey::FrameFloatingBorderRoundedCorners => copy_border_override(
            &mut frames.floating_border_style,
            &from_frames.floating_border_style,
            5,
        ),
        SettingKey::WebClientFont => web.font = from_web.font.clone(),
        SettingKey::WebClientFontSize => web.font_size = from_web.font_size,
        SettingKey::WebClientCursorBlink => web.cursor_blink = from_web.cursor_blink,
        SettingKey::WebClientCursorStyle => web.cursor_style = from_web.cursor_style.clone(),
        SettingKey::WebClientCursorInactiveStyle => {
            web.cursor_inactive_style = from_web.cursor_inactive_style.clone()
        },
        SettingKey::WebClientMacOptionIsMeta => {
            web.mac_option_is_meta = from_web.mac_option_is_meta
        },
        SettingKey::WebClientBaseUrl => web.base_url = from_web.base_url.clone(),
        SettingKey::Keybinds => {
            target.keybinds_layers.user = source.keybinds_layers.user.clone();
            target.resolve_keybinds();
        },
        SettingKey::PluginAliases => target.plugins = source.plugins.clone(),
        SettingKey::LoadPlugins => target.background_plugins = source.background_plugins.clone(),
        SettingKey::Env => target.env = source.env.clone(),
        SettingKey::Themes => copy_config_file_themes(&mut target.themes, &source.themes),
        SettingKey::ContextMenu => target.context_menu = source.context_menu.clone(),
        window_key => {
            debug_assert!(window_key.is_window());
            copy_window_setting(&mut target.window, &source.window, window_key)
        },
    }
}

pub fn default_config() -> &'static Config {
    static DEFAULT_CONFIG: std::sync::OnceLock<Config> = std::sync::OnceLock::new();
    DEFAULT_CONFIG.get_or_init(|| Config::from_default_assets().unwrap_or_default())
}

pub fn unset_setting(config: &mut Config, key: SettingKey) {
    if key == SettingKey::Keybinds {
        config.keybinds_layers.user = Default::default();
        config.resolve_keybinds();
        return;
    }
    let defaults = Config {
        keybinds: config.keybinds.clone(),
        ..Default::default()
    };
    copy_setting(config, &defaults, key);
}

pub fn setting_node_path(key: SettingKey) -> (&'static [&'static str], &'static str) {
    (key.parent_nodes(), key.kdl_name())
}

pub fn setting_kdl_values(config: &Config, key: SettingKey) -> Option<Vec<KdlValue>> {
    setting_value(config, key).and_then(|value| key.kdl_values(&value))
}

pub fn setting_is_default(config: &Config, key: SettingKey) -> bool {
    if key == SettingKey::Keybinds {
        return config.keybinds_layers.user.is_empty();
    }
    let mut defaults = config.clone();
    unset_setting(&mut defaults, key);
    setting_value(&defaults, key) == setting_value(config, key)
}

pub fn setting_node_in_document<'a>(
    document: &'a KdlDocument,
    key: SettingKey,
) -> Option<&'a KdlNode> {
    let (parents, name) = setting_node_path(key);
    let mut current = document;
    for parent in parents {
        current = current.get(parent)?.children()?;
    }
    current.get(name)
}

pub fn settings_set_in_file(file_contents: &str) -> BTreeSet<SettingKey> {
    let Ok(document) = file_contents.parse::<KdlDocument>() else {
        return BTreeSet::new();
    };
    SettingKey::all()
        .into_iter()
        .filter(|key| setting_node_in_document(&document, *key).is_some())
        .collect()
}

pub fn settings_set_in_file_at(config_file_path: &Path) -> BTreeSet<SettingKey> {
    std::fs::read_to_string(config_file_path)
        .map(|contents| settings_set_in_file(&contents))
        .unwrap_or_default()
}

pub fn setting_states(
    saved: &Config,
    current: &Config,
    set_in_file: &BTreeSet<SettingKey>,
) -> Vec<ConfigSettingState> {
    let saved_values = setting_values(saved);
    let current_values = setting_values(current);
    let keybinds_changed = keybinds_differ(saved, current);
    SettingKey::all()
        .into_iter()
        .map(|key| {
            let (saved_value, current_value) = if key.is_block() {
                let changed = if key == SettingKey::Keybinds {
                    keybinds_changed
                } else {
                    saved_values.get(&key) != current_values.get(&key)
                };
                let saved_marker = Some("saved".to_owned());
                let current_marker = if changed {
                    Some("changed".to_owned())
                } else {
                    saved_marker.clone()
                };
                (saved_marker, current_marker)
            } else {
                (
                    saved_values.get(&key).cloned().flatten(),
                    current_values.get(&key).cloned().flatten(),
                )
            };
            ConfigSettingState {
                key,
                saved_value,
                current_value,
                set_in_file: set_in_file.contains(&key),
            }
        })
        .collect()
}

pub fn theme_names(config: &Config) -> Vec<String> {
    static BUILT_IN_THEME_NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let built_in = BUILT_IN_THEME_NAMES.get_or_init(|| {
        crate::setup::get_default_themes()
            .inner()
            .keys()
            .cloned()
            .collect()
    });
    let names: BTreeSet<String> = config
        .themes
        .inner()
        .keys()
        .cloned()
        .chain(built_in.iter().cloned())
        .collect();
    names.into_iter().collect()
}

pub fn plugin_alias_lines(config: &Config) -> Vec<String> {
    config
        .plugins
        .aliases
        .iter()
        .map(|(name, run_plugin)| format!("{} → {}", name, run_plugin.location))
        .collect()
}

pub fn load_plugin_lines(config: &Config) -> Vec<String> {
    let mut lines: Vec<String> = config
        .background_plugins
        .iter()
        .map(|plugin| plugin.location_string())
        .collect();
    lines.sort();
    lines
}

pub fn env_lines(config: &Config) -> Vec<String> {
    let mut lines: Vec<String> = config
        .env
        .inner()
        .iter()
        .map(|(name, value)| format!("{}={}", name, value))
        .collect();
    lines.sort();
    lines
}

pub fn context_menu_lines(config: &Config) -> Vec<String> {
    let menu = &config.context_menu;
    let sections = [
        ("pane", &menu.pane),
        ("tab", &menu.tab),
        ("bar", &menu.bar),
        ("common", &menu.common),
    ];
    sections
        .iter()
        .flat_map(|(section, entries)| {
            entries.iter().filter_map(move |entry| match entry {
                ContextMenuEntry::Item { label, .. } => Some(format!("{}: {}", section, label)),
                ContextMenuEntry::Separator => None,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "./unit/config_settings_test.rs"]
pub(crate) mod config_settings_test;
