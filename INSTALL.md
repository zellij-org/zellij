# Installation

* [Cargo](#cargo)
* [Packages](#package)
    * [Arch Linux](#arch-linux)
    * [MacOS](#macos)

## Cargo
You can install with `cargo`:

```
cargo install zellij
```

Or you can download a prebuilt binary from our [Releases](https://github.com/zellij-org/zellij/releases).

The default plugins make use of characters that are mostly found in [nerdfonts](https://www.nerdfonts.com/).
To get the best experience either install nerdfonts, or use the simplified ui by starting Zellij with `zellij options --simplified-ui`, or putting `simplified_ui: true` in the config file.

## Packages

** :warning: These packages are not affiliated with the Zellij maintainers and are provided here for convenience.**

[![Packaging status](https://repology.org/badge/vertical-allrepos/zellij.svg)](https://repology.org/project/zellij/versions)

### Arch Linux
You can install the `zellij` package from the [official community repository](https://archlinux.org/packages/community/x86_64/zellij/):

```
pacman -S zellij
```

Or install from AUR repository with [AUR Helper](https://wiki.archlinux.org/title/AUR_helpers):

```
paru -S zellij-git
```

### MacOS
You can install `zellij` with [Homebrew on MacOS](https://formulae.brew.sh/formula/zellij):

```
brew install zellij
```

Or install with [MacPorts](https://ports.macports.org/port/zellij/details/):

```
port install zellij
```