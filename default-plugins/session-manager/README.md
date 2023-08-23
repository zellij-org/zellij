## About

This is an example [Zellij][zellij] plugin in Rust. It can be used as a template to start developing your own plugins.

More about Zellij plugins: [Zellij Documentation][docs]

[zellij]: https://github.com/zellij-org/zellij
[docs]: https://zellij.dev/documentation/plugins.html

## Development

*Note*: you will need to have `wasm32-wasi` added to rust as a target to build the plugin. This can be done with `rustup target add wasm32-wasi`.

## Inside Zellij
![img-2023-06-14-143355](https://github.com/zellij-org/rust-plugin-example/assets/795598/d9e563dc-5d71-4e10-af5b-190365bdca3b)

You can load the `./plugin-dev-workspace.kdl` file as a Zellij layout to get a terminal development environment:

Either when starting Zellij:
```
zellij --layout ./plugin-dev-workspace.kdl
```
*Note that in this case there's a small bug where the plugin is opened twice, it can be remedied by closing the oldest instance or loading with the new-tab action as secified below - this will be addressed in the near future*

Or from a running Zellij session:
```bash
zellij action new-tab --layout ./plugin-dev-workspace.kdl
```

## Otherwise

1. Build the project: `cargo build`
2. Load it inside a running Zellij session: `zellij action start-or-reload-plugin file:target/wasm32-wasi/debug/rust-plugin-example.wasm`
3. Repeat on changes (perhaps with a `watchexec` or similar command to run on fs changes).
