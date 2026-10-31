#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
os="$(uname -s)"
known="ghostty kitty alacritty wezterm foot zellij"

usage() {
    cat <<'EOF'
Usage: run.sh [options]

Measures how long terminals take to print large files with cat, in a pane of the
same size in every terminal, and prints comparison tables (Markdown, so they can
be pasted into an issue). The report is also saved to a file.

Each file is timed twice with hyperfine inside the terminal:
  until processed   the terminal has parsed all of the output: after the file,
                    the terminal is asked for its cursor position, which it can
                    only answer once everything before the question is parsed
  until cat exits   the terminal has accepted the output; a terminal that
                    buffers a lot looks faster here than it is
Compare terminals with the "until processed" table.

Options:
  -t, --targets "LIST"    terminals to run, separated by spaces (default: every
                          supported terminal that is installed)
                          supported: ghostty kitty alacritty wezterm foot zellij
  --custom NAME=TEMPLATE  add any other terminal by its launch command; {rows},
                          {cols} and {cmd} are replaced, eg.
                          --custom 'xterm=xterm -geometry {cols}x{rows} -e {cmd}'
  --rows N, --cols N      pane size (default 48 x 191)
  --runs N                measured runs per file (default 10, plus 2 warm-up)
  --size-mb N             size of each generated file in MB (default 8)
  --cases "LIST"          generated files to use (default: ascii log color unicode)
  --file PATH             also time this file (repeatable)
  --zellij-bin PATH       zellij binary for the zellij target (default: this
                          repository's target/release/zellij if built, else zellij
                          on PATH); it is started as "zellij terminal"
  --zellij-variant LABEL:VAR=VALUE
                          also run Zellij as a separate column named LABEL with an
                          environment variable set (repeatable), eg.
                          --zellij-variant zellij-async:ZELLIJ_PTY_READER=async
  --ablate [GROUPS]       also run Zellij once per improvement group with that group
                          turned off (columns "no-GROUP"), plus "no-all" with every
                          group off, and print how much slower or faster each one is
                          than the full build; GROUPS is a comma list, default every
                          group: bulk_text parser_scan style_memo lazy_links
                          pty_fast_path big_reads ack_wait row_shifts row_recycle
                          frame_style_runs
  --old-parser            also run Zellij with the previous terminal-output parser
                          (column "old-parser") and print how much slower or faster
                          it is than the new one; combine with --rounds to interleave
  --rounds N              repeat every terminal N times, interleaved, and report
                          each terminal's mean over all rounds (default 1); each
                          round starts one terminal later in the list, so every
                          terminal runs in every position equally often when N is
                          a multiple of the number of terminals; this spreads
                          slow-downs from heat or background load evenly
  --font-size N           give every built-in terminal this font size, so the
                          windows are about the same size in pixels (default:
                          each terminal's own default)
  --font-family NAME      give every built-in terminal this font (an installed
                          monospace font, eg. "DejaVu Sans Mono"); terminals ship
                          different default fonts, whose cells differ in size
  --zellij-bars N         rows taken by Zellij's tab and status bars (default 2);
                          the Zellij window is made this much taller so its pane
                          matches the other terminals
  --timeout SECONDS       give up on a terminal after this long (default 900)
  --out PATH              where to save the report (default <work dir>/report.md)
  --work-dir PATH         scratch directory (default /tmp/terminal-throughput)

GHOSTTY_BIN, KITTY_BIN, ALACRITTY_BIN, WEZTERM_BIN and FOOT_BIN override where a
terminal is found.

Requires bash, python3 and hyperfine (1.15 or newer). Every terminal runs with
its default configuration. Leave the windows alone while they run. A terminal
that opens at the wrong size is reopened with a corrected request; if it still
does not match, the report flags it.
EOF
}

targets=""
customs=()
variants=()
files=()
rounds=1
ablate=""
old_parser=""
all_groups="bulk_text,parser_scan,style_memo,lazy_links,pty_fast_path,big_reads,ack_wait,row_shifts,row_recycle,frame_style_runs"
font_size=""
font_family=""
runs=10
rows=48
cols=191
size_mb=8
cases="ascii log color unicode"
zellij_bin=""
bars=2
timeout=900
out=""
work="/tmp/terminal-throughput"
attempts=4

while [[ $# -gt 0 ]]; do
    case "$1" in
        -t | --targets) targets="$2"; shift 2 ;;
        --custom) customs+=("$2"); shift 2 ;;
        --rows) rows="$2"; shift 2 ;;
        --cols) cols="$2"; shift 2 ;;
        --runs) runs="$2"; shift 2 ;;
        --size-mb) size_mb="$2"; shift 2 ;;
        --cases) cases="$2"; shift 2 ;;
        --file) files+=("$2"); shift 2 ;;
        --zellij-bin) zellij_bin="$2"; shift 2 ;;
        --zellij-variant) variants+=("$2"); shift 2 ;;
        --rounds) rounds="$2"; shift 2 ;;
        --old-parser) old_parser=1; shift ;;
        --ablate)
            if [[ $# -gt 1 && "$2" != -* ]]; then
                ablate="$2"
                shift 2
            else
                ablate="$all_groups"
                shift
            fi
            ;;
        --font-size) font_size="$2"; shift 2 ;;
        --font-family) font_family="$2"; shift 2 ;;
        --zellij-bars) bars="$2"; shift 2 ;;
        --timeout) timeout="$2"; shift 2 ;;
        --out) out="$2"; shift 2 ;;
        --work-dir) work="$2"; shift 2 ;;
        -h | --help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 1 ;;
    esac
done

command -v hyperfine >/dev/null || { echo "hyperfine is required (cargo install hyperfine)" >&2; exit 1; }
command -v python3 >/dev/null || { echo "python3 is required" >&2; exit 1; }

variant_env() {
    local entry
    for entry in ${variants[@]+"${variants[@]}"}; do
        if [[ "${entry%%:*}" == "$1" ]]; then
            printf '%s' "${entry#*:}"
            return 0
        fi
    done
    return 1
}

base_target() {
    if variant_env "$1" >/dev/null; then
        echo zellij
    else
        echo "$1"
    fi
}

custom_template() {
    local entry
    for entry in ${customs[@]+"${customs[@]}"}; do
        if [[ "${entry%%=*}" == "$1" ]]; then
            printf '%s' "${entry#*=}"
            return 0
        fi
    done
    return 1
}

first_existing() {
    local candidate
    for candidate in "$@"; do
        if [[ -n "$candidate" ]] && command -v "$candidate" >/dev/null 2>&1; then
            command -v "$candidate"
            return 0
        fi
    done
    return 1
}

bin_for() {
    set -- "$(base_target "$1")"
    local override
    override="$(printf '%s_BIN' "$1" | tr '[:lower:]' '[:upper:]')"
    case "$1" in
        ghostty) first_existing "${!override:-}" ghostty /Applications/Ghostty.app/Contents/MacOS/ghostty ;;
        kitty) first_existing "${!override:-}" kitty /Applications/kitty.app/Contents/MacOS/kitty ;;
        alacritty) first_existing "${!override:-}" alacritty /Applications/Alacritty.app/Contents/MacOS/alacritty ;;
        wezterm) first_existing "${!override:-}" wezterm /Applications/WezTerm.app/Contents/MacOS/wezterm ;;
        foot) first_existing "${!override:-}" foot ;;
        zellij) first_existing "$zellij_bin" "$repo/target/release/zellij" zellij ;;
        *) custom_template "$1" >/dev/null && echo custom ;;
    esac
}

version_of() {
    case "$(base_target "$1")" in
        ghostty | kitty | alacritty | wezterm | foot | zellij)
            "$2" --version 2>/dev/null | head -1 || true
            ;;
    esac
}

launch() {
    local t="$1" req_rows="$2" req_cols="$3" bin="$4"
    shift 4
    local font=()
    local extra_env=()
    variant_env "$t" >/dev/null && extra_env=("$(variant_env "$t")")
    case "$(base_target "$t")" in
        ghostty)
            [[ -n "$font_size" ]] && font+=(--font-size="$font_size")
            [[ -n "$font_family" ]] && font+=(--font-family="$font_family")
            ;;
        kitty)
            [[ -n "$font_size" ]] && font+=(-o "font_size=$font_size")
            [[ -n "$font_family" ]] && font+=(-o "font_family=$font_family")
            ;;
        alacritty)
            [[ -n "$font_size" ]] && font+=(-o "font.size=$font_size")
            [[ -n "$font_family" ]] && font+=(-o "font.normal.family=\"$font_family\"")
            ;;
        wezterm)
            [[ -n "$font_size" ]] && font+=(--config "font_size=$font_size")
            [[ -n "$font_family" ]] && font+=(--config "font=wezterm.font(\"$font_family\")")
            ;;
        foot)
            if [[ -n "$font_size$font_family" ]]; then
                font+=(-o "font=${font_family:-monospace}:size=${font_size:-12}")
            fi
            ;;
    esac
    case "$(base_target "$t")" in
        ghostty)
            local extra=()
            [[ "$os" == Linux ]] && extra=(--gtk-single-instance=false)
            "$bin" --config-default-files=false ${extra[@]+"${extra[@]}"} \
                --window-width="$req_cols" --window-height="$req_rows" ${font[@]+"${font[@]}"} -e "$@" &
            ;;
        kitty)
            "$bin" --config NONE -o initial_window_width="${req_cols}c" -o initial_window_height="${req_rows}c" \
                -o remember_window_size=no ${font[@]+"${font[@]}"} "$@" &
            ;;
        alacritty)
            "$bin" --config-file /dev/null -o "window.dimensions.columns=$req_cols" \
                -o "window.dimensions.lines=$req_rows" ${font[@]+"${font[@]}"} -e "$@" &
            ;;
        wezterm)
            "$bin" -n --config "initial_cols=$req_cols" --config "initial_rows=$req_rows" ${font[@]+"${font[@]}"} \
                start --always-new-process -- "$@" &
            ;;
        foot)
            "$bin" -c /dev/null --window-size-chars="${req_cols}x${req_rows}" ${font[@]+"${font[@]}"} "$@" &
            ;;
        zellij)
            local base="$work/zellij"
            rm -rf "$base"
            mkdir -p "$base/s"
            cat >"$base/config.kdl" <<EOF
show_startup_tips false
show_release_notes false
terminal {
    initial_columns $req_cols
    initial_rows $req_rows
$([[ -n "$font_size" ]] && echo "    font_size $font_size")
$([[ -n "$font_family" ]] && echo "    font \"$font_family\"")
}
EOF
            env ${extra_env[@]+"${extra_env[@]}"} ZELLIJ_CONFIG_FILE="$base/config.kdl" "$bin" terminal --standalone \
                --startup-mode windowed --socket-dir "$base/s" -e "$@" &
            ;;
        *)
            local template quoted="" arg
            template="$(custom_template "$t")"
            for arg in "$@"; do
                quoted+="$(printf '%q' "$arg") "
            done
            template="${template//"{cmd}"/$quoted}"
            template="${template//"{rows}"/$req_rows}"
            template="${template//"{cols}"/$req_cols}"
            eval "$template" &
            ;;
    esac
    pid=$!
}

wait_for() {
    local results="$1" started=$SECONDS
    while [[ ! -e "$results/done" ]]; do
        if [[ ! -e "$results/started" ]] && ((SECONDS - started > 30)); then
            echo "the benchmark never started inside the window" >&2
            return 1
        fi
        if ((SECONDS - started > timeout)); then
            echo "gave up after ${timeout}s" >&2
            return 1
        fi
        sleep 0.2
    done
}

[[ -n "$ablate$old_parser" && -z "$targets" ]] && targets="zellij"
if [[ -z "$targets" ]]; then
    for t in $known; do
        bin_for "$t" >/dev/null && targets="$targets $t"
    done
fi
if [[ -n "$ablate" ]]; then
    [[ " $targets " == *" zellij "* ]] || targets="zellij $targets"
    for group in ${ablate//,/ }; do
        variants+=("no-$group:ZELLIJ_OPT_OFF=$group")
    done
    variants+=("no-all:ZELLIJ_OPT_OFF=all")
fi
if [[ -n "$old_parser" ]]; then
    [[ " $targets " == *" zellij "* ]] || targets="zellij $targets"
    variants+=("old-parser:ZELLIJ_VT_PARSER=old")
fi
for entry in ${variants[@]+"${variants[@]}"}; do
    name="${entry%%:*}"
    [[ " $targets " == *" $name "* ]] || targets="$targets $name"
done
for entry in ${customs[@]+"${customs[@]}"}; do
    name="${entry%%=*}"
    [[ " $targets " == *" $name "* ]] || targets="$targets $name"
done
[[ -n "${targets// /}" ]] || { echo "no terminals found; pass --targets or --custom" >&2; exit 1; }

if [[ -n "$font_family" ]] && command -v fc-list >/dev/null; then
    if [[ -z "$(fc-list ":family=$font_family" family)" ]]; then
        echo "warning: font \"$font_family\" is not installed; terminals will fall back to other fonts" >&2
        echo "installed monospace fonts: fc-list :spacing=mono family" >&2
    fi
fi

fixtures="$work/fixtures-${size_mb}mb"
rm -rf "$work/results"
mkdir -p "$work/results"
BENCH_SIZE=$((size_mb * 1024 * 1024)) python3 "$here/gen-fixtures.py" "$fixtures" >/dev/null
for f in ${files[@]+"${files[@]}"}; do
    name="$(basename "$f")"
    name="${name%.*}"
    cp "$f" "$fixtures/$name.txt"
    cases="$cases $name"
done
case_list="$(echo $cases | tr ' ' ',')"
expect="${rows}x${cols}"

cpu=""
if [[ -r /proc/cpuinfo ]]; then
    cpu="$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ *//')"
elif [[ "$os" == Darwin ]]; then
    cpu="$(sysctl -n machdep.cpu.brand_string 2>/dev/null || true)"
fi
meta="$work/results/machine.txt"
{
    echo "date: $(date '+%Y-%m-%d %H:%M')"
    echo "system: $(uname -srm)"
    [[ -n "$cpu" ]] && echo "cpu: $cpu"
    echo "hyperfine: $(hyperfine --version | head -1)"
    echo "pane: $rows x $cols, $runs runs per file after 2 warm-up runs, ${size_mb} MB files, $rounds interleaved round(s), font ${font_family:-default} at size ${font_size:-default}"
} >"$meta"

run_target() {
    local t="$1" results="$2"
    if ! bin="$(bin_for "$t")"; then
        echo "skipping $t: not found" >&2
        return 1
    fi
    req_rows="$rows"
    req_cols="$cols"
    [[ "$(base_target "$t")" == zellij ]] && req_rows=$((rows + bars))
    previous=""
    ok=1
    echo "running $t (round $round of $rounds) ..." >&2
    for attempt in $(seq 1 "$attempts"); do
        rm -rf "$results"
        mkdir -p "$results"
        final=0
        [[ "$attempt" == "$attempts" ]] && final=1
        launch "$t" "$req_rows" "$req_cols" "$bin" \
            bash "$here/in-pane.sh" "$fixtures" "$results" "$runs" "$case_list" "$expect" "$final"
        wait_for "$results" || { ok=0; break; }
        got="$(cat "$results/pane-size" 2>/dev/null || true)"
        if [[ "$got" == "$expect" || "$final" == 1 || ! "$got" =~ ^[0-9]+x[0-9]+$ ]]; then
            break
        fi
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
        if [[ "$got" == "$previous" ]]; then
            echo "$t ignores the requested size; running at ${got/x/ x }" >&2
            rm -rf "$results"
            mkdir -p "$results"
            launch "$t" "$req_rows" "$req_cols" "$bin" \
                bash "$here/in-pane.sh" "$fixtures" "$results" "$runs" "$case_list" "$expect" 1
            wait_for "$results" || ok=0
            break
        fi
        previous="$got"
        echo "$t: pane came out ${got/x/ x }, asked for $req_rows x $req_cols; retrying" >&2
        req_rows=$((req_rows + rows - ${got%x*}))
        req_cols=$((req_cols + cols - ${got#*x}))
    done
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    if [[ "$ok" == 0 ]]; then
        echo "$t failed; leaving it out of the report" >&2
        echo "failed: $t" >>"$meta"
        return 1
    fi
    version_of "$t" "$bin" >"$results/version"
}

failed=" "
read -ra order <<<"$targets"
for round in $(seq 1 "$rounds"); do
    shift_by=$(((round - 1) % ${#order[@]}))
    for t in "${order[@]:shift_by}" "${order[@]:0:shift_by}"; do
        [[ "$failed" == *" $t "* ]] && continue
        run_target "$t" "$work/results/round-$round/$t" || failed="$failed$t "
    done
done

report_args=()
for t in $targets; do
    [[ "$failed" == *" $t "* ]] && continue
    merged="$work/results/$t"
    rm -rf "$merged"
    mkdir -p "$merged"
    python3 "$here/merge.py" "$merged" "$work"/results/round-*/"$t"
    report_args+=("$t" "$merged")
done

[[ ${#report_args[@]} -gt 0 ]] || { echo "no terminal produced results" >&2; exit 1; }
out="${out:-$work/report.md}"
REPORT_BASE="$([[ -n "$ablate$old_parser" ]] && echo zellij)" \
    REPORT_META="$meta" REPORT_FIXTURES="$fixtures" EXPECT="$expect" \
    python3 "$here/report.py" "${report_args[@]}" | tee "$out"
echo >&2
echo "report saved to $out" >&2
