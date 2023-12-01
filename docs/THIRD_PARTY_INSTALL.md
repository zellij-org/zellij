# Third Party Install

* [Packages](#packages)
    * [Arch Linux](#arch-linux)
    * [MacOS](#macos)
    * [Fedora Linux](#fedora-linux)
    * [Void Linux](#void-linux)

## Packages

 :warning: **These packages are not affiliated with the Zellij maintainers and are provided here for convenience.**

[![Packaging status](https://repology.org/badge/vertical-allrepos/zellij.svg)](https://repology.org/project/zellij/versions)

### Arch Linux
You can install the `zellij` package from the [official extra repository](https://archlinux.org/packages/extra/x86_64/zellij/):

```
pacman -S zellij
```

Or install from AUR repository with [AUR Helper](https://wiki.archlinux.org/title/AUR_helpers):

```
paru -S zellij-git
```

### Fedora Linux
You can install the `zellij` package from the [COPR](https://copr.fedorainfracloud.org/coprs/varlad/zellij/)

```
sudo dnf copr enable varlad/zellij 
sudo dnf install zellij
```

### MacOS
You can install `zellij` with [Homebrew on MacOS](https://formulae.brew.sh/formula/zellij):

```
brew install zellij
```

Or install with [MacPorts](https://ports.macports.org/port/zellij/details/):

```
sudo port install zellij
```

### Void Linux

```
sudo xbps-install zellij
```
