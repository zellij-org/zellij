mod desktop_app;
mod open;
mod panel;
mod plan;
mod standalone;
mod target;
mod waited;

pub(crate) use desktop_app::{describe, launch_app, run_app};
pub(crate) use open::{hosted_by_the_app, load, open, panel_view, window_request, Loaded};
pub(crate) use panel::{panel_border, panel_layout, panel_look};
pub(crate) use plan::{configured, plan, remembered, starting_folder, Plan};
pub(crate) use standalone::{installed_font_families, open_alone, refuse};
pub(crate) use target::{connect, resolve, switch, Reach, Refusal, Target};
pub(crate) use waited::{run_in_background, run_waited};

#[cfg(test)]
mod tests;
