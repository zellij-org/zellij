#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
    echo "usage: $0 <test_name> [file]" >&2
    exit 1
fi

test_name=$1
file=${2:-}
snapshot_dir=tests/snapshots

if [[ ! -d $snapshot_dir ]]; then
    echo "error: $snapshot_dir not found (run from the zellij-integration-tests folder)" >&2
    exit 1
fi

prefix=${file:-*}

shopt -s nullglob
candidates=("$snapshot_dir"/${prefix}__"$test_name".snap "$snapshot_dir"/${prefix}__"$test_name"-*.snap)
shopt -u nullglob

snapshots=()
for candidate in "${candidates[@]}"; do
    [[ -f $candidate ]] && snapshots+=("$candidate")
done

if [[ ${#snapshots[@]} -eq 0 ]]; then
    echo "error: no snapshots found for test '$test_name'${file:+ in file '$file'}" >&2
    exit 1
fi

if [[ -z $file ]]; then
    declare -A files_seen=()
    for snapshot in "${snapshots[@]}"; do
        base=${snapshot##*/}
        files_seen[${base%%__*}]=1
    done
    if [[ ${#files_seen[@]} -gt 1 ]]; then
        files=("${!files_seen[@]}")
        echo "error: test '$test_name' exists in multiple files: ${files[*]}" >&2
        echo "re-run with the file, e.g. $0 $test_name ${files[0]}" >&2
        exit 1
    fi
fi

for snapshot in "${snapshots[@]}"; do
    zellij action new-pane --stacked --close-on-exit -- vim "$(realpath "$snapshot")"
done
