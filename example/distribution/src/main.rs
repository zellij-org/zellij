use zellij::{Distribution, DistributionLayout};

fn main() {
    zellij::run(
        Distribution::new("example-distribution", env!("CARGO_PKG_VERSION"))
            .with_display_name("Example Distribution")
            .with_plugin(
                "example-plugin",
                include_bytes!(env!("EXAMPLE_DISTRIBUTION_PLUGIN_WASM")),
            )
            .without_plugin("share")
            .with_config(include_str!("../assets/config.kdl"))
            .with_layout(DistributionLayout::new(
                "example",
                include_str!("../assets/layout.kdl"),
            )),
    )
}
