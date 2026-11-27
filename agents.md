# Agent Notes: SmartCopy Maintenance and Zellij Releases

This fork carries the `SmartCopy` action on top of upstream Zellij. `SmartCopy` copies an active selection; when there is no selection it forwards `Ctrl-C` to the focused pane.

## Updating to a new upstream Zellij release

1. Fetch upstream and create/update a branch from the new upstream tag or mainline commit:

   ```sh
   git fetch upstream --tags
   git switch -c smartcopy-vNEXT upstream/main   # or the new upstream tag
   ```

2. Replay the SmartCopy implementation commits from the previous fork history. Find them with:

   ```sh
   git log --all --oneline --grep='SmartCopy'
   ```

   Do not blindly replay merge/release/documentation commits. Replay the implementation, routing/IPC, example, and regression-fix commits as appropriate for the new upstream layout. The 0.46 port consisted primarily of the action/parser, screen/route/IPC, and example changes, followed by the keybinding parser fix in commit `318b01ac7`.

3. Resolve conflicts against the new upstream implementation rather than preserving stale surrounding code. Audit these areas after replay:

   - `zellij-utils/src/input/actions.rs` — `Action::SmartCopy`
   - `zellij-utils/src/kdl/mod.rs` — both action parsers must recognize `SmartCopy`:
     - `action_without_arguments()` (CLI/simple action parsing)
     - the `TryFrom<(&KdlNode, &Options)>` keybinding parser, near the `Copy` case
   - `zellij-utils/src/client_server_contract/common_types.proto` and generated protobuf assets
   - `zellij-utils/src/ipc/protobuf_conversion.rs`
   - `zellij-server/src/route.rs` and `zellij-server/src/screen.rs`
   - `example/default.kdl` or the equivalent example configuration

   A common failure is adding `SmartCopy` to the first parser but not the keybinding parser. The symptom is `Unsupported action: SmartCopy` at a config line such as:

   ```kdl
   normal {
       bind "Ctrl c" { SmartCopy; }
   }
   ```

4. Build and test both Linux variants before publishing. The full release builds can take roughly 15–20 minutes on this development machine. Do not assume a timed-out build produced a valid current binary. Verify the binary path and version explicitly.

5. Keep unrelated working-tree files out of commits. In particular, local `node_modules/`, `package.json`, and `bun.lock` files may exist from tooling and are not part of the Rust/Zellij change unless intentionally added.

## Fedora/RHEL development setup

From a Fedora/RHEL-based system, install the basic build tools and dependencies (package names can vary slightly by RHEL version):

```sh
sudo dnf install -y git gcc gcc-c++ make pkgconf-pkg-config openssl-devel protobuf-compiler
```

For desktop/terminal integration and local builds, these development packages may also be needed:

```sh
sudo dnf install -y \
  fontconfig-devel libxkbcommon-devel libX11-devel libXi-devel \
  libXcursor-devel libXrandr-devel libxcb-devel wayland-devel mesa-libGL-devel
```

Install Rust with rustup, then let `rust-toolchain.toml` select the repository toolchain and targets:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup show
```

The repository currently pins Rust `1.95.0`, `rustfmt`, `clippy`, `wasm32-wasip1`, and `x86_64-unknown-linux-musl` in `rust-toolchain.toml`. `protoc` is required for protobuf generation. `pkg-config` and OpenSSL are required for tests/build paths.

Useful commands:

```sh
cargo xtask format
cargo xtask build
cargo xtask test
cargo xtask run
cargo build --release --locked --bin zellij
```

Do not set `CARGO_TARGET_DIR` to a shared target directory for debug builds: `.cargo/config.toml` keeps plugin assets in the repository's `target` directory. Release builds are less sensitive to this, but using the repository target directory is simplest.

## Building Linux release binaries

The musl binary is static and broadly portable, but cannot provide the full desktop-terminal integration. The GNU binary can load Wayland/X11/OpenGL/fontconfig and is the one to use for the full terminal feature.

For a local native GNU build on an x86_64 GNU/Linux host:

```sh
cargo build --release --locked --target x86_64-unknown-linux-gnu --bin zellij
```

For release-compatible GNU binaries (glibc 2.17 baseline), follow the repository release workflow and use Zig/cargo-zigbuild:

```sh
cargo install --locked cargo-zigbuild
cargo xtask ci cross x86_64-unknown-linux-gnu
bash .github/scripts/check-gnu-binary.sh \
  target/x86_64-unknown-linux-gnu/release/zellij
```

Build musl with:

```sh
cargo build --release --locked \
  --target x86_64-unknown-linux-musl --bin zellij
```

Check both versions and binary types:

```sh
target/x86_64-unknown-linux-gnu/release/zellij --version
target/x86_64-unknown-linux-musl/release/zellij --version
file target/x86_64-unknown-linux-gnu/release/zellij \
     target/x86_64-unknown-linux-musl/release/zellij
```

To test configuration parsing without a real terminal, a direct invocation may fail later with a raw-mode/TTY error. That still distinguishes parsing from execution. A pseudo-terminal is better:

```sh
timeout 10s script -qec \
  'target/x86_64-unknown-linux-musl/release/zellij options --session-serialization false' \
  /dev/null
```

The important result is that the command reaches Zellij rather than reporting `Unsupported action: SmartCopy`.

## Updating the fork's existing GitHub release

The fork's release is separate from upstream. Always specify the fork explicitly because the default `gh` repository may resolve to `zellij-org/zellij`:

```sh
gh release view v0.46.0 --repo tychart/zellij
```

Keep the existing version/tag (for example `v0.46.0`) when replacing binaries; do not create a new tag merely for the SmartCopy rebuild. Confirm GitHub CLI authentication first:

```sh
gh auth status
```

Package exactly the binary names used by the official release workflow. The checksum files contain the checksum of the binary, not the archive:

```sh
stage=$(mktemp -d)
for target in x86_64-unknown-linux-gnu x86_64-unknown-linux-musl; do
  tar -czf "$stage/zellij-$target.tar.gz" \
    -C "target/$target/release" zellij
  sha256sum "target/$target/release/zellij" \
    > "$stage/zellij-$target.sha256sum"
done
```

Replace the four assets in the fork release with `--clobber`:

```sh
gh release upload v0.46.0 \
  "$stage/zellij-x86_64-unknown-linux-gnu.tar.gz" \
  "$stage/zellij-x86_64-unknown-linux-gnu.sha256sum" \
  "$stage/zellij-x86_64-unknown-linux-musl.tar.gz" \
  "$stage/zellij-x86_64-unknown-linux-musl.sha256sum" \
  --repo tychart/zellij --clobber
```

Download the assets back and verify the archives contain a single `zellij` file and that their published checksums/metadata are present:

```sh
gh release download v0.46.0 --repo tychart/zellij \
  -p 'zellij-x86_64-unknown-linux-gnu*' \
  -p 'zellij-x86_64-unknown-linux-musl*'
```

The official naming convention is:

- `zellij-x86_64-unknown-linux-gnu.tar.gz`
- `zellij-x86_64-unknown-linux-gnu.sha256sum`
- `zellij-x86_64-unknown-linux-musl.tar.gz`
- `zellij-x86_64-unknown-linux-musl.sha256sum`

After code/documentation changes, inspect status and commit only intended files, then push the current PR branch:

```sh
git status --short --branch
git add agents.md zellij-utils/src/kdl/mod.rs   # adjust intentionally
 git commit -m "Document SmartCopy update and release workflow"
git push
```
