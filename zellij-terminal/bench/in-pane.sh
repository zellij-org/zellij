#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fixtures="$1"
results="$2"
runs="${3:-10}"
cases="${4:-ascii,log,color,unicode}"
expect="${5:-}"
final="${6:-1}"

mkdir -p "$results"
touch "$results/started"
sleep 1
size="$(stty size </dev/tty 2>/dev/null | tr ' ' x || true)"
echo "$size" >"$results/pane-size"
if [[ -n "$expect" && "$size" != "$expect" && "$final" != 1 ]]; then
    touch "$results/done"
    exit 0
fi
for name in ${cases//,/ }; do
    hyperfine -N --output=inherit --style=none --warmup 2 --runs "$runs" \
        --export-json "$results/$name.json" "cat $fixtures/$name.txt" || true
    hyperfine -N --output=inherit --style=none --warmup 2 --runs "$runs" \
        --export-json "$results/$name+processed.json" "bash $here/sync-cat.sh $fixtures/$name.txt" 2>>"$results/errors.log" || true
done
touch "$results/done"
