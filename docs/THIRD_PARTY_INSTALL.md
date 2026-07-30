# Third Party Install

* [Packages](#packages)
    * [Arch Linux](#arch-linux)
    * [Debian/Ubuntu](#debianubuntu)
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

### Debian/Ubuntu
You can install the `zellij` package from the unofficial [deb.griffo.io](https://deb.griffo.io/install-latest-zellij-in-debian.html) APT repository, maintained by [dariogriffo](https://github.com/dariogriffo). Packages are built automatically from the official releases:

```
sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://deb.griffo.io/EA0F721D231FDD3A0A17B9AC7808B4DD62C41256.asc | sudo gpg --dearmor --yes -o /etc/apt/keyrings/deb.griffo.io.gpg
echo "deb [signed-by=/etc/apt/keyrings/deb.griffo.io.gpg] https://deb.griffo.io/apt $(lsb_release -sc) main" | sudo tee /etc/apt/sources.list.d/deb.griffo.io.list > /dev/null
sudo apt update && sudo apt install zellij
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
