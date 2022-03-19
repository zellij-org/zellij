#!/usr/bin/env bash
set -euo pipefail
# dependencies: bash, curl, jq
#
# Updates a rust-toolchain file in relation to the official rust releases.

# How many minor versions delta there should be,
# will automatically advance patch versions.
MINOR_DELTA=2
RUST_TOOLCHAIN_VERSION=$(grep -oP 'channel = "\K[^"]+' "./rust-toolchain")
#RUST_TOOLCHAIN_FILE=../../rust-toolchain
RUST_TOOLCHAIN_FILE=./rust-toolchain

# update with new version number
update_channel(){
printf \
"
# This file is updated by \`update-toolchain.sh\`
# We aim to be around 1-2 rust releases behind in order
# to get people the time to update their toolchains properly.
# By enforcing this, we can also make full use of the features
# provided by the current channel.

"
sed -e "/channel/s/\".*\"/\"$1\"/" "${RUST_TOOLCHAIN_FILE}"
}

get_last_no_releases() {
    curl --silent "https://api.github.com/repos/rust-lang/rust/releases" | \
        jq '.[range(20)].tag_name' | sed -e 's/\"//g'
}

function parse_semver() {
    local token="$1"
    local major=0
    local minor=0
    local patch=0

    if grep -E '^[0-9]+\.[0-9]+\.[0-9]+' <<<"$token" >/dev/null 2>&1 ; then
        local n=${token//[!0-9]/ }
        local a=(${n//\./ })
        major=${a[0]}
        minor=${a[1]}
        patch=${a[2]}
    fi

    echo "$major $minor $patch"
}
function get_string_arrary() {
    IFS=' ' read  -r -a array <<< "$1";
    echo "${array["${2}"]}"
}

RUST_TOOLCHAIN_VERSION="$(parse_semver $(echo $RUST_TOOLCHAIN_VERSION))"
TARGET_TOOLCHAIN_MINOR_VERSION=$(($(get_string_arrary "${RUST_TOOLCHAIN_VERSION}" 1) - MINOR_DELTA))

LATEST=0
for i in $(get_last_no_releases);do
    SEMVER=($(parse_semver "$i"))
    if [ "$LATEST" != "${SEMVER[1]}" ];then
        MINOR_DELTA=$((MINOR_DELTA - 1))
    fi
    LATEST="${SEMVER[1]}"
    if [ -1 == "${MINOR_DELTA}" ];then
        echo "$(update_channel "$i")"> ${RUST_TOOLCHAIN_FILE}
        exit
    fi
done
