#!/usr/bin/env bash
set -euo pipefail

REPO="zellij-org/zellij"
INSTALL_DIR=""
NO_WEB=false
TMPDIR_WORK=""

die() { echo "Error: $1" >&2; exit 1; }

detect_target() {
    local arch sys
    case "$(uname -m)" in
        x86_64)          arch="x86_64" ;;
        aarch64 | arm64) arch="aarch64" ;;
        *) die "Unsupported architecture: $(uname -m)" ;;
    esac
    case "$(uname -s)" in
        Linux)  sys="unknown-linux-musl" ;;
        Darwin) sys="apple-darwin" ;;
        *) die "Unsupported OS: $(uname -s)" ;;
    esac
    echo "${arch}-${sys}"
}

pick_install_dir() {
    if [ -w /usr/local/bin ]; then
        echo /usr/local/bin
    else
        echo "$HOME/.local/bin"
    fi
}

prompt_no_web() {
    local answer
    printf "Install variant:\n"
    printf "  [1] Full (with web plugins) [default]\n"
    printf "  [2] No-web (lighter, no WebAssembly plugin support)\n"
    printf "Choice [1/2]: "
    read -r answer </dev/tty
    case "$answer" in
        2) NO_WEB=true ;;
        *) NO_WEB=false ;;
    esac
}

prompt_install_dir() {
    local default_dir answer
    default_dir="$(pick_install_dir)"
    printf "Install directory [%s]: " "$default_dir"
    read -r answer </dev/tty
    if [ -z "$answer" ]; then
        INSTALL_DIR="$default_dir"
    else
        INSTALL_DIR="$answer"
    fi
}

verify_sha256() {
    local file="$1" expected="$2"
    local actual
    if command -v sha256sum &>/dev/null; then
        actual="$(sha256sum "$file" | awk '{print $1}')"
    elif command -v shasum &>/dev/null; then
        actual="$(shasum -a 256 "$file" | awk '{print $1}')"
    else
        die "sha256sum / shasum not found"
    fi
    [ "$actual" = "$expected" ] || die "SHA256 mismatch!\n  expected: $expected\n  got:      $actual"
}

main() {
    local target version prefix asset checksum_file asset_url checksum_url expected_hash

    target="$(detect_target)"

    echo "=== Zellij Installer ==="
    echo "Detected target: $target"
    echo ""

    prompt_no_web
    prompt_install_dir

    echo ""
    echo "Fetching latest release..."
    version="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
    [ -n "$version" ] || die "Could not determine latest version"
    echo "Version: $version"

    prefix=""
    "$NO_WEB" && prefix="no-web-"

    asset="zellij-${prefix}${target}.tar.gz"
    checksum_file="zellij-${prefix}${target}.sha256sum"
    asset_url="https://github.com/${REPO}/releases/download/${version}/${asset}"
    checksum_url="https://github.com/${REPO}/releases/download/${version}/${checksum_file}"

    TMPDIR_WORK="$(mktemp -d)"
    trap 'rm -rf "$TMPDIR_WORK"' EXIT

    echo "Downloading ${asset}..."
    curl -fSL --progress-bar "$asset_url" -o "$TMPDIR_WORK/$asset"

    echo "Extracting..."
    tar -C "$TMPDIR_WORK" -xzf "$TMPDIR_WORK/$asset"

    echo "Verifying checksum..."
    expected_hash="$(curl -fsSL "$checksum_url" | awk '{print $1}')"
    verify_sha256 "$TMPDIR_WORK/zellij" "$expected_hash"
    echo "Checksum OK"

    mkdir -p "$INSTALL_DIR"
    mv "$TMPDIR_WORK/zellij" "$INSTALL_DIR/zellij"
    chmod +x "$INSTALL_DIR/zellij"

    echo ""
    echo "Installed: $INSTALL_DIR/zellij"

    case ":$PATH:" in
        *":$INSTALL_DIR:"*) ;;
        *) echo "Warning: $INSTALL_DIR is not in \$PATH" ;;
    esac

    echo "Done. Run: zellij"
}

main "$@"
