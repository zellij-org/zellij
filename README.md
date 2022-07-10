<h1 align="center">
  <br>
  <img src="https://raw.githubusercontent.com/zellij-org/zellij/main/assets/logo.png" alt="logo" width="200">
  <br>
  Zellij
  <br>
  <br>
</h1>

<p align="center">
  <a href="https://discord.gg/CrUAFH3"><img alt="Discord Chat" src="https://img.shields.io/discord/771367133715628073?color=5865F2&label=discord&style=flat-square"></a>
  <a href="https://matrix.to/#/#zellij_general:matrix.org"><img alt="Matrix Chat" src="https://img.shields.io/matrix/zellij_general:matrix.org?color=1d7e64&label=matrix%20chat&style=flat-square&logo=matrix"></a>
  <a href="https://zellij.dev/documentation/"><img alt="Zellij documentation" src="https://img.shields.io/badge/zellij-documentation-fc0060?style=flat-square"></a>
  <a href="https://builtwithnix.org"><img alt="Built with nix" src="https://img.shields.io/static/v1?label=built%20with&message=nix&color=5277C3&logo=nixos&style=flat-square&logoColor=ffffff"></a>
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/zellij-org/zellij/main/assets/demo.gif" alt="demo">
</p>

<h4 align="center">
  [<a href="https://zellij.dev/documentation/installation">Installation</a>]
  [<a href="https://zellij.dev/documentation/overview">Overview</a>]
  [<a href="https://zellij.dev/documentation/configuration">Configuration</a>]
  [<a href="https://zellij.dev/documentation/layouts-templates">Templates</a>]
  [<a href="https://zellij.dev/documentation/faq">FAQ</a>]
</h4>

# What is this?

[Zellij](https://en.wikipedia.org/wiki/Zellij) is a workspace aimed at developers, ops-oriented people and anyone who loves the terminal.
At its core, it is a terminal multiplexer (similar to [tmux](https://github.com/tmux/tmux) and [screen](https://www.gnu.org/software/screen/)), but this is merely its infrastructure layer.

Zellij includes a [layout system](https://zellij.dev/documentation/layouts.html), and a [plugin system](https://zellij.dev/documentation/plugins.html) allowing one to create plugins in any language that compiles to WebAssembly.

For more details about our future plans, read about upcoming features in our [roadmap](#roadmap).

Zellij was initially called "Mosaic".

## How do I install it?

You can install with `cargo`:

```
cargo install zellij
```

Or if want to a prebuilt binary, you can download it from our [Releases](https://github.com/zellij-org/zellij/releases), or use [`cargo-binstall`](https://github.com/ryankurte/cargo-binstall).

```
cargo-binstall zellij
```

Or you can also use [Third Party Repositories](./docs/THIRD_PARTY_INSTALL.md).

#### Try Zellij without installing

bash/zsh:
```
bash <(curl -L zellij.dev/launch)
```
fish:
```
bash (curl -L zellij.dev/launch | psub)
```

## How do I get involved?

Zellij is a labour of love built by an enthusiastic team of volunteers. We eagerly welcome anyone who would like to join us, regardless of experience level, so long as they adhere to our [code of conduct](CODE_OF_CONDUCT.md).

Please report any code of conduct violations to [aram@poor.dev](mailto:aram@poor.dev)

To get started, you can:
1. Take a look at the "Issues" in this repository - especially those marked "Good first issue". Those with the "Help Wanted" tag probably don't have anyone else working on them.
2. Drop by our [discord](https://discord.gg/CrUAFH3), or [matrix](https://matrix.to/#/#zellij_general:matrix.org) chat and ask what you can work on, or how to get started.
3. Open an issue with your idea(s) for the project or tell us about them in our chat.

## How do I start a development environment?

* Clone the project
* Install cargo-make with `cargo install --force cargo-make`
* In the project folder, for debug builds run: `cargo make run`
* To run all tests: `cargo make test`

For more build commands, see [`Contributing.md`](CONTRIBUTING.md).

## Configuration
For configuring Zellij, please see the [Configuration documentation](https://zellij.dev/documentation/configuration.html).

## What is the current status of the project?

Zellij should be ready for everyday use, but it's still classified as a beta. This means that there might be a rare crash or wrong behaviour here and there, but that once found it should be fixed rather quickly. If this happens to you, we would be very happy if you could open an issue and tell us how to reproduce it as best you can.



## Roadmap
Presented here is the project roadmap, divided into three main sections.

These are issues that are either being actively worked on or are planned for the near future.

*If you'll click on the image, you'll be led to an SVG version of it on the website where you can directly click on every issue*

[![roadmap](https://user-images.githubusercontent.com/795598/168313474-f6cb9754-77ea-4ce3-bc84-8840f2eadd75.png)](https://zellij.dev/roadmap)

## License

MIT
