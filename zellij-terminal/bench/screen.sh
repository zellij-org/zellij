#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="${BENCH_DIR:-/tmp/zellij-throughput}"
cases="${CASES:-ascii log color unicode}"

python3 "$here/gen-fixtures.py" "$work/fixtures" >/dev/null

bin="$(cd "$here/../.." && cargo test --profile perf -p zellij-server --lib --no-run --message-format=json 2>/dev/null \
    | python3 -c 'import json,sys
for l in sys.stdin:
    m = json.loads(l)
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == "zellij_server":
        print(m["executable"])' | tail -1)"
[[ -x "$bin" ]] || { echo "could not build the zellij-server test binary" >&2; exit 1; }

if [[ -n "${CALLGRIND:-}" ]]; then
    name="${cases%% *}"
    head -c "${CALLGRIND_BYTES:-1048576}" "$work/fixtures/$name.txt" >"$work/callgrind-$name.txt"
    out="$work/callgrind.$name.out"
    ZELLIJ_BENCH_FILE="$work/callgrind-$name.txt" valgrind --tool=callgrind --collect-atstart=no \
        --toggle-collect='*Screen::apply_pty_bytes*' --toggle-collect='*Screen::render_to_clients*' \
        --toggle-collect='*Screen::handle_render_frame_ack*' \
        --callgrind-out-file="$out" "$bin" throughput_bench --ignored --nocapture --test-threads=1 2>&1 \
        | grep -E "throughput_bench:|Collected" || true
    echo "$out"
    exit 0
fi

for name in $cases; do
    printf "%-10s " "$name"
    ZELLIJ_BENCH_FILE="$work/fixtures/$name.txt" "$bin" throughput_bench --ignored --nocapture --test-threads=1 2>&1 \
        | grep -o "throughput_bench: .*" | sed 's/throughput_bench: //'
done
