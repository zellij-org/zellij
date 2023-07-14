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
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/zellij-org/zellij/main/assets/demo.gif" alt="demo">
</p>

<h4 align="center">
  [<a href="https://zellij.dev/documentation/installation">Installation</a>]
  [<a href="https://zellij.dev/screencasts/">Screencasts & Tutorials</a>]
  [<a href="https://zellij.dev/documentation/configuration">Configuration</a>]
  [<a href="https://zellij.dev/documentation/layouts">Layouts</a>]
  [<a href="https://zellij.dev/documentation/faq">FAQ</a>]
</h4>

# What is this?

[Zellij](https://en.wikipedia.org/wiki/Zellij) is a workspace aimed at developers, ops-oriented people and anyone who loves the terminal. Similar programs are sometimes called "Terminal Multiplexers".

Zellij is designed around the philosophy that one must not sacrifice simplicity for power, taking pride in its great experience out of the box as well as the advanced features it places at its users' fingertips.

Zellij is geared toward beginner and power users alike - allowing deep customizability, personal automation through [layouts](https://zellij.dev/documentation/layouts.html), true multiplayer collaboration, unique UX features such as floating and stacked panes, and a [plugin system](https://zellij.dev/documentation/plugins.html) allowing one to create plugins in any language that compiles to WebAssembly.

You can get started by [installing](https://zellij.dev/documentation/installation.html) Zellij and checking out the [Screencasts & Tutorials](https://zellij.dev/screencasts/).

For more details about our future plans, read about upcoming features in our [roadmap](#roadmap).

## How do I install it?

The easiest way to install Zellij is through a [package for your OS](./docs/THIRD_PARTY_INSTALL.md).

If one is not available for your OS, you could download a prebuilt binary from the [latest release](https://github.com/zellij-org/zellij/releases/latest) and place it in your `$PATH`. If you'd like, we could [automatically choose one for you](#try-zellij-without-installing).

You can also install (compile) with `cargo`:

```
cargo install --locked zellij
```

#### Try Zellij without installing

bash/zsh:
```bash
bash <(curl -L zellij.dev/launch)
```
fish/xonsh:
```bash
bash -c 'bash <(curl -L zellij.dev/launch)'
```

## How do I get involved?

Zellij is a labour of love built by an enthusiastic team of volunteers. We eagerly welcome anyone who would like to join us, regardless of experience level, so long as they adhere to our [Code of Conduct](CODE_OF_CONDUCT.md).

Please report any code of conduct violations to [aram@poor.dev](mailto:aram@poor.dev)

To get started, you can:
1. Take a look at the [Issues](https://github.com/zellij-org/zellij/issues) in this repository - especially those marked "good first issue". Those with the "help wanted" tag probably don't have anyone else working on them.
2. Drop by our [Discord](https://discord.gg/CrUAFH3), or [Matrix](https://matrix.to/#/#zellij_general:matrix.org) chat and ask what you can work on, or how to get started.
3. Open an issue with your idea(s) for the project or tell us about them in our chat.

## How do I start a development environment?

* Clone the project
* In the project folder, for debug builds run: `cargo xtask run`
* To run all tests: `cargo xtask test`

For more build commands, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Configuration
For configuring Zellij, please see the [Configuration Documentation](https://zellij.dev/documentation/configuration.html).

## What is the current status of the project?

Zellij should be ready for everyday use, but it's still classified as a beta. This means that there might be a rare crash or wrong behaviour here and there, but that once found it should be fixed rather quickly. If this happens to you, we would be very happy if you could open an issue and tell us how to reproduce it as best you can.

## Roadmap
Presented here is the project roadmap, divided into three main sections.

These are issues that are either being actively worked on or are planned for the near future.

***If you'll click on the image, you'll be led to an SVG version of it on the website where you can directly click on every issue***

[![roadmap](https://user-images.githubusercontent.com/795598/228612600-78fbd05a-ec84-484d-8b82-bba071460690.png)](https://zellij.dev/roadmap)

## License

MIT
