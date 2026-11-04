use crate::config::options::Options;
use crate::{config, emulation};

pub(crate) fn panel_layout(view: &str) -> String {
    format!(
        "keybinds clear-defaults=true {{\n}}\nlayout {{\n    pane {{\n        plugin location=\"zellij:session-manager\" {{\n            role \"app_panel\"\n            view \"{}\"\n        }}\n    }}\n}}\n",
        view
    )
}

pub(crate) fn panel_look(options: &mut Options) {
    options.transparency = crate::render::scene::Transparency::OPAQUE;
    options.blur = false;
    options.padding = config::options::Padding {
        top: PANEL_GRIP_TOP,
        right: PANEL_GRIP_SIDE,
        bottom: PANEL_GRIP_SIDE,
        left: PANEL_GRIP_SIDE,
    };
    options.padding_balance = false;
    options.padding_color = zellij_utils::input::terminal::PaddingColor::Background;
}

pub(crate) const PANEL_GRIP_TOP: f32 = 10.0;
pub(crate) const PANEL_GRIP_SIDE: f32 = 8.0;

pub(crate) fn panel_border(
    settings: &config::settings::Settings,
    options: &Options,
) -> emulation::color::Srgb {
    let Some(theme) = settings.theme(None) else {
        return options.paints.indexed(8);
    };
    let styling = theme.palette;
    let color = styling
        .frame_unselected
        .map(|frame| frame.base)
        .unwrap_or(styling.frame_selected.base);
    match color {
        zellij_utils::data::PaletteColor::Rgb((r, g, b)) => [r, g, b],
        zellij_utils::data::PaletteColor::EightBit(index) => options.paints.indexed(index),
    }
}
