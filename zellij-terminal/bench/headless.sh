#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
target="${CARGO_TARGET_DIR:-$here/../../target}"
zellij="${ZELLIJ_BIN:-$target/perf/zellij}"
runs="${RUNS:-10}"
rows="${ROWS:-50}"
cols="${COLS:-200}"
cases="${CASES:-ascii log color unicode}"
label="${LABEL:-headless}"
work="${BENCH_DIR:-/tmp/zellij-throughput}"
results="$work/results/$label"

command -v hyperfine >/dev/null || { echo "hyperfine is required" >&2; exit 1; }
[[ -x "$zellij" ]] || { echo "$zellij is missing; build it with: cargo build --profile perf" >&2; exit 1; }

python3 "$here/gen-fixtures.py" "$work/fixtures" >/dev/null
if [[ -n "${FILE:-}" ]]; then
    extra="$(basename "$FILE")"
    extra="${extra%.*}"
    cp "$FILE" "$work/fixtures/$extra.txt"
    cases="${CASES:-$extra}"
fi

base="$work/env-$label"
rm -rf "$base" "$results"
mkdir -p "$base/sock" "$base/tmp" "$base/cache" "$results"
cat >"$base/config.kdl" <<'EOF'
show_startup_tips false
show_release_notes false
mouse_hover_tips false
EOF
if [[ -n "${NO_PLUGINS:-}" ]]; then
    printf 'layout {\n    pane\n}\n' >"$base/bare.kdl"
    printf 'default_layout "%s"\n' "$base/bare.kdl" >>"$base/config.kdl"
fi
export ZELLIJ_SOCKET_DIR="$base/sock" TMPDIR="$base/tmp" XDG_CACHE_HOME="$base/cache" ZELLIJ_CONFIG_FILE="$base/config.kdl"

session="throughput-bench"
${WRAP:-} "$zellij" terminal --headless --rows "$((rows + ${BAR_ROWS:-2}))" --cols "$cols" --duration-secs 600 "$session" -c -- \
    bash "$here/in-pane.sh" "$work/fixtures" "$results" "$runs" "${cases// /,}" >"$base/client.log" 2>&1 &
client=$!

while [[ ! -e "$results/done" ]]; do
    if ! kill -0 "$client" 2>/dev/null; then
        echo "headless client exited early, see $base/client.log" >&2
        exit 1
    fi
    sleep 0.2
done
"$zellij" kill-session "$session" >/dev/null 2>&1 || true
kill "$client" 2>/dev/null || true
wait "$client" 2>/dev/null || true

python3 "$here/report.py" "$label" "$results"
