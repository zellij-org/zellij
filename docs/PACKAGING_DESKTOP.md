# Packaging Zellij for the desktop (Linux)

Each release has a separate `zellij-desktop-files.tar.gz` (with a
`zellij-desktop-files.sha256sum`) holding a `share/` folder laid out like
`/usr/share`. The binary archives hold only the binary. Install the folder's
contents alongside the `zellij` binary so the desktop app shows up in
application menus and software centres:

| File in `zellij-desktop-files.tar.gz` | Install to |
|---|---|
| `share/applications/dev.zellij.Zellij.desktop` | `/usr/share/applications/` |
| `share/icons/hicolor/128x128/apps/dev.zellij.Zellij.png` | `/usr/share/icons/hicolor/128x128/apps/` |
| `share/icons/hicolor/256x256/apps/dev.zellij.Zellij.png` | `/usr/share/icons/hicolor/256x256/apps/` |
| `share/icons/hicolor/512x512/apps/dev.zellij.Zellij.png` | `/usr/share/icons/hicolor/512x512/apps/` |
| `share/metainfo/dev.zellij.Zellij.metainfo.xml` | `/usr/share/metainfo/` |

The same files are in the source tree: `assets/dev.zellij.Zellij.desktop`,
`zellij-utils/assets/icons/logo128.png`, `logo256.png` and `logo512.png` (each installed as
`dev.zellij.Zellij.png` in its size folder) and `assets/dev.zellij.Zellij.metainfo.xml`.

Notes for packagers:

- The launcher **replaces** the old `zellij.desktop`. Do not ship both, or the app
  menu shows two "Zellij" entries.
- `Exec=zellij window` and `TryExec=zellij` find the binary through `PATH`, so
  `zellij` must be on `PATH` (for example `/usr/bin/zellij`).
- The app ID is `dev.zellij.Zellij`. The window's Wayland app ID and X11 class,
  the icon name and the notification sender all use it, so keep the file names.
- The launcher carries the keys `xdg-terminal-exec` reads (`X-TerminalArgExec`,
  `X-TerminalArgDir`, `X-TerminalArgHold`), so Zellij can be chosen as the default
  terminal once it is installed.
- When Zellij finds its launcher in `$XDG_DATA_DIRS/applications`, it shows it as
  "installed by your package manager" and never writes its own copy.
- A desktop entry or default-terminal setting that Zellij writes itself only ever
  runs a binary in a permanent place: a package-manager path (Nix, Homebrew,
  `cargo install`, `/usr` outside `/usr/local`), a system folder (`/usr/local`
  or `/opt`), or Zellij's own copy in `~/.local/bin`. Run from anywhere else, Zellij
  first copies itself to `~/.local/bin` (or updates an older Zellij there) and
  points the entry at that copy. A binary in one of the permanent places is never
  offered a copy.
- Check the listing with `appstreamcli validate --no-net
  dev.zellij.Zellij.metainfo.xml` and the launcher with `desktop-file-validate`.
- Zellij decides whether it runs on a desktop from the server's environment: no
  `DISPLAY` or `WAYLAND_DISPLAY`, an `SSH_CONNECTION`, `SSH_CLIENT` or `SSH_TTY`,
  a container or WSL all count as "no desktop". Set
  `ZELLIJ_DESKTOP_INTEGRATION=on` to treat the environment as a desktop anyway,
  or `off` to never offer desktop items.
- Everything the setup screen does is also available from the command line:
  `zellij setup --copy-to-permanent-folder [--replace-existing]`,
  `--install-desktop-entry`, `--uninstall-desktop-entry`,
  `--make-default-terminal` and `--keybinding-preset NAME`. They use the same
  checks as the setup screen and can be combined.
