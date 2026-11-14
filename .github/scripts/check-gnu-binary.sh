#!/usr/bin/env bash
set -euo pipefail

bin="$1"
max_glibc="${2:-2.17}"

if [[ ! -f "$bin" ]]; then
    echo "::error::$bin does not exist"
    exit 1
fi

needed=$(readelf -d "$bin" | sed -n 's/.*Shared library: \[\(.*\)\]/\1/p')
echo "Libraries needed at startup:"
echo "$needed" | sed 's/^/  /'

if ! grep -qx 'libc\.so\.6' <<<"$needed"; then
    echo "::error::$bin does not link glibc dynamically, so it cannot load the desktop libraries"
    exit 1
fi

allowed='^(libc\.so\.6|libm\.so\.6|libdl\.so\.2|libpthread\.so\.0|libutil\.so\.1|librt\.so\.1|libgcc_s\.so\.1|ld-linux[-a-z0-9_]*\.so\.[0-9]+)$'
unexpected=$(grep -Ev "$allowed" <<<"$needed" || true)
if [[ -n "$unexpected" ]]; then
    echo "::error::$bin needs libraries that are not on every Linux system: $(tr '\n' ' ' <<<"$unexpected")"
    exit 1
fi

newest=$(readelf -V "$bin" | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/^GLIBC_//' | sort -uV | tail -1)
echo "Newest glibc version needed: $newest (allowed: $max_glibc)"
if [[ -z "$newest" ]]; then
    echo "::error::could not read the glibc versions $bin needs"
    exit 1
fi
if [[ "$(printf '%s\n%s\n' "$newest" "$max_glibc" | sort -V | tail -1)" != "$max_glibc" ]]; then
    echo "::error::$bin needs glibc $newest, newer than $max_glibc"
    exit 1
fi

echo "$bin passed the GNU build check"
