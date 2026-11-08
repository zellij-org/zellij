#!/usr/bin/env bash
set -euo pipefail

# Protoc is pinned to a specific release and verified before extraction.
# Checksums are SHA-256 of the official assets, verified 2026-11-08 from:
# https://github.com/protocolbuffers/protobuf/releases/tag/v23.4
readonly protoc_version="23.4"
readonly protoc_base_url="https://github.com/protocolbuffers/protobuf/releases/download/v${protoc_version}"

case "${RUNNER_OS:-$(uname -s)}" in
  Linux)
    asset="protoc-${protoc_version}-linux-x86_64.zip"
    expected_sha256="0502f286ac9ed860b629a7965a14527b1f2dd131e4283fa23c2d7f184672aa9a"
    ;;
  macOS|Darwin)
    asset="protoc-${protoc_version}-osx-universal_binary.zip"
    expected_sha256="123d184419b1b83b36e463bc7ee8345b4376e691af0865b4e43c7d8ac459f972"
    ;;
  Windows|MINGW*|MSYS*|CYGWIN*)
    asset="protoc-${protoc_version}-win64.zip"
    expected_sha256="a309c39442fb75f0db343cb22c111a00f91cdf0767f332e170644b9378e2bcc6"
    ;;
  *)
    echo "Unsupported runner OS: ${RUNNER_OS:-$(uname -s)}" >&2
    exit 1
    ;;
esac

temp_dir="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
archive="${temp_dir}/protoc-${protoc_version}.zip"
destination="${temp_dir}/protoc-${protoc_version}"

curl --fail --silent --show-error --location --proto '=https' \
  --retry 3 --connect-timeout 15 \
  --output "$archive" "${protoc_base_url}/${asset}"

if command -v sha256sum >/dev/null 2>&1; then
  printf '%s  %s\n' "$expected_sha256" "$archive" | sha256sum --check -
else
  printf '%s  %s\n' "$expected_sha256" "$archive" | shasum -a 256 --check -
fi

rm -rf "$destination"
unzip -q -o "$archive" -d "$destination"
echo "${destination}/bin" >> "${GITHUB_PATH:?GITHUB_PATH must be set by the runner}"
