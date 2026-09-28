#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
target="${CARGO_TARGET_DIR:-$here/../../target}"
zellij="${ZELLIJ_BIN:-$target/debug/zellij}"
config="${CAPTURE_CONFIG:-$here/capture.kdl}"

rows="${ROWS:-40}"
cols="${COLS:-120}"
settle="${SETTLE:-2}"
cell_width="${CELL_WIDTH:-10}"
cell_height="${CELL_HEIGHT:-20}"

if [[ ! -x "$zellij" ]]; then
    echo "capture: $zellij is missing; build it first" >&2
    exit 1
fi

session_of() {
    echo "window-capture-$1"
}

start_session() {
    local session="$1"
    "$zellij" kill-session "$session" >/dev/null 2>&1 || true
    "$zellij" delete-session "$session" >/dev/null 2>&1 || true
    "$zellij" -c "$config" attach --create-background "$session" >/dev/null 2>&1
    sleep "$settle"
}

stop_session() {
    local session="$1"
    "$zellij" kill-session "$session" >/dev/null 2>&1 || true
    "$zellij" delete-session "$session" >/dev/null 2>&1 || true
}

capture() {
    local case_name="$1"
    local hold="$2"
    shift 2

    local session
    session="$(session_of "$case_name")"
    local fixture="$here/$case_name.jsonl"

    start_session "$session"

    "$zellij" window "$session" --headless \
        --record "$fixture" \
        --rows "$rows" --cols "$cols" \
        --cell-width "$cell_width" --cell-height "$cell_height" \
        --duration-secs "$hold" >/dev/null 2>&1 &
    local recorder=$!
    sleep "$settle"

    export ZELLIJ_CAPTURE_SESSION="$session"
    "$@" || echo "capture: the driver for $case_name reported a failure" >&2
    unset ZELLIJ_CAPTURE_SESSION

    wait "$recorder"
    stop_session "$session"
    echo "capture: wrote $fixture"
}

run_in_session() {
    local script="export LC_ALL=C.utf8
$2
sleep 3600"
    "$zellij" -s "$ZELLIJ_CAPTURE_SESSION" run --close-on-exit --name "$1" -- \
        bash -c "$script" >/dev/null
}

act() {
    "$zellij" -s "$ZELLIJ_CAPTURE_SESSION" action "$@" >/dev/null
}

case_smoke() {
    run_in_session smoke '
        printf "\033[1;31mBOLD-RED\033[0m \033[4;32mUNDERLINE-GREEN\033[0m \033[7mREVERSE\033[0m \033[38;5;208m256-COLOR\033[0m\n"
    '
}

case_styling() {
    run_in_session styling '
        printf "\033[1mBOLD\033[0m \033[2mDIM\033[0m \033[3mITALIC\033[0m \033[4mUNDERLINE\033[0m\n"
        printf "\033[21mDOUBLE\033[0m \033[4:3mUNDERCURL\033[0m \033[9mSTRIKE\033[0m \033[7mREVERSE\033[0m\n"
        for i in 0 1 2 3 4 5 6 7; do printf "\033[3%dmF%d\033[0m " "$i" "$i"; done; printf "\n"
        for i in 0 1 2 3 4 5 6 7; do printf "\033[9%dmB%d\033[0m " "$i" "$i"; done; printf "\n"
        for i in 0 1 2 3 4 5 6 7; do printf "\033[4%dm %d \033[0m" "$i" "$i"; done; printf "\n"
        for i in 16 52 88 124 160 196 226 231 244; do printf "\033[38;5;%dmP%d\033[0m " "$i" "$i"; done; printf "\n"
        printf "\033[38;2;255;0;128mTRUECOLOR-FG\033[0m \033[48;2;0;64;128mTRUECOLOR-BG\033[0m\n"
        printf "\033[1;4;7;31mCOMBINED\033[0m \033[8mHIDDEN\033[0m end\n"
    '
}

case_scroll_regions() {
    run_in_session scroll-regions '
        printf "\033[2J\033[H"
        for i in $(seq 1 12); do printf "line-%02d\n" "$i"; done
        printf "\033[3;9r\033[9;1H"
        for i in $(seq 1 4); do printf "scrolled-%02d\n" "$i"; done
        printf "\033[r\033[12;1Hafter-region-reset"
    '
}

case_wide_chars() {
    run_in_session wide-chars '
        printf "CJK: \u4f60\u597d\u4e16\u754c\n"
        printf "KANA: \u30a2\u30a4\u30a6\u30a8\u30aa\n"
        printf "EMOJI: \U0001f600\U0001f680\U0001f30d\n"
        printf "COMBINING: e\u0301 a\u0308 o\u0302\n"
        printf "MIXED: ab\u4f60cd\U0001f600ef\n"
        printf "BOX: \u250c\u2500\u2510 \u2502 \u2514\u2500\u2518\n"
    '
}

case_glyph_torture() {
    run_in_session glyph-torture '
        printf "\033[2J\033[H"
        printf "CJK:       \u4f60\u597d\u4e16\u754c \u6f22\u5b57 \u65e5\u672c\u8a9e\n"
        printf "KANA:      \u3042\u3044\u3046\u3048\u304a \u30a2\u30a4\u30a6\u30a8\u30aa\n"
        printf "HANGUL:    \ud55c\uae00 \uc548\ub155\n"
        printf "EMOJI:     \U0001f600 \U0001f680 \U0001f30d \U0001f525 \U0001f984 \u2764\n"
        printf "MARKS:     e\u0301 a\u0308 o\u0302 n\u0303 u\u0300\n"
        printf "BRAILLE:   \u2801\u2803\u2807\u280f\u281f\u283f\n"
        printf "BOX:       \u250c\u2500\u252c\u2500\u2510 \u2502 \u2514\u2500\u2534\u2500\u2518\n"
        printf "BLOCKS:    \u2588\u2593\u2592\u2591 \u25b2\u25bc\u25c6\u25cf\n"
        printf "POWERLINE: \ue0b0\ue0b1\ue0b2\ue0b3\n"
        printf "WIDEPUNCT: \uff08\uff09\uff0c\u3001\u3002\uff1a\n"
        printf "MISSING:   \U0010fffd end\n"
        printf "\033[4mSINGLE\033[0m \033[4:2mDOUBLE\033[0m \033[4:3mCURLY\033[0m \033[4:4mDOTTED\033[0m \033[4:5mDASHED\033[0m\n"
        printf "\033[4m\033[58;5;208mORANGE-UNDER\033[59;0m \033[4:3m\033[58;2;0;255;255mCYAN-CURL\033[0m\n"
        printf "MIXED:     ab\u4f60cd\U0001f600ef \033[1m\u4f60BOLD\033[0m \033[2m\U0001f600DIM\033[0m\n"
    '
}

case_cursor_shapes() {
    run_in_session cursor-shapes '
        printf "\033[2J\033[H"
        printf "block:     \033[2 q\n"
        printf "underline: \033[4 q\n"
        printf "beam:      \033[6 q\n"
        printf "\033[4;12Hcursor parked here\n"
        printf "\033[6;1Hhidden next: \033[?25l"
        sleep 2
        printf "\033[?25h\033[6 q\033[8;20H"
    '
}

kitty_helpers() {
    cat <<'HELPERS'
gen() {
    local w=$1 h=$2 x y row cell
    for ((y = 0; y < h; y++)); do
        row=""
        for ((x = 0; x < w; x++)); do
            printf -v cell "\\\\x%02x\\\\x%02x\\\\x%02x" \
                $(( (x * 255) / (w - 1) )) $(( (y * 255) / (h - 1) )) 128
            row+="$cell"
        done
        printf "%b" "$row"
    done
}
emit() {
    printf "\033_G%s;%s\033\\\\" "$1" "$2"
}
HELPERS
}

case_kitty_graphics() {
    run_in_session kitty-graphics "$(kitty_helpers)"'
        wide=$(gen 32 40 | base64 -w0)
        small=$(gen 16 20 | base64 -w0)
        printf "\033[2J\033[H"
        printf "kitty graphics\n"
        printf "\033[3;3H"
        emit "a=T,q=2,f=24,t=d,i=1,s=32,v=40" "$wide"
        printf "\033[3;14H"
        emit "a=T,q=2,f=24,t=d,i=2,s=16,v=20,z=-1" "$small"
        printf "\033[8;3Hcropped:"
        printf "\033[9;3H"
        emit "a=p,q=2,i=1,p=7,x=8,y=10,w=16,h=20" ""
        printf "\033[12;3H"
        emit "a=T,q=2,f=24,t=d,i=3,s=16,v=20" "$small"
        sleep 2
        emit "a=d,q=2,d=i,i=3" ""
        printf "\033[14;3Himage 3 placement deleted"
    '
}

case_kitty_pane_open() {
    run_in_session kitty-pane-open "$(kitty_helpers)"'
        wide=$(gen 32 40 | base64 -w0)
        printf "\033[2J\033[H"
        printf "picture below\n"
        printf "\033[4;3H"
        emit "a=T,q=2,f=24,t=d,i=1,s=32,v=40" "$wide"
    '
    sleep 5
    act new-pane --direction right
    sleep 3
}

sixel_helpers() {
    cat <<'HELPERS'
bands() {
    local width=$1 count=$2 registers=$3 index out=""
    for ((index = 0; index < count; index++)); do
        out+="#$(( index % registers ))!${width}~-"
    done
    printf "%s" "$out"
}
sixel() {
    local background=$1 width=$2 bands=$3 palette=$4 body=$5
    printf "\033P0;%s;0q\"1;1;%s;%s%s%s\033\\\\" \
        "$background" "$width" "$(( bands * 6 ))" "$palette" "$body"
}
HELPERS
}

case_sixel_graphics() {
    run_in_session sixel-graphics "$(sixel_helpers)"'
        rgb="#0;2;100;0;0#1;2;0;100;0#2;2;0;0;100#3;2;100;100;0"
        hls="#0;1;0;50;100#1;1;120;50;100#2;1;240;50;100"
        printf "\033[2J\033[H"
        printf "sixel graphics\n"
        printf "\033[3;3H"
        sixel 1 32 6 "$rgb" "$(bands 32 6 4)"
        printf "\033[3;10H"
        sixel 1 24 6 "$hls" "$(bands 24 6 3)"
        printf "\033[10;3Hopaque background:"
        printf "\033[11;3H"
        sixel 0 32 3 "$rgb" "#0!16~"
        printf "\033[15;3Hoverwritten:"
        printf "\033[16;3H"
        sixel 1 40 6 "$rgb" "$(bands 40 6 4)"
        sleep 2
        printf "\033[18;4HXXXX"
    '
}

case_sixel_pane_open() {
    run_in_session sixel-pane-open "$(sixel_helpers)"'
        rgb="#0;2;100;0;0#1;2;0;100;0"
        printf "\033[2J\033[H"
        printf "picture below\n"
        printf "\033[4;3H"
        sixel 1 48 8 "$rgb" "$(bands 48 8 2)"
    '
    sleep 5
    act new-pane --direction right
    sleep 3
}

case_panes_and_tabs() {
    act new-pane --direction right
    sleep 1
    act new-pane --direction down
    sleep 1
    act new-tab
    sleep 1
    act new-pane --direction right
    sleep 1
    act go-to-tab 1
    sleep 1
    act move-focus right
    sleep 1
}

case_blink() {
    run_in_session blink '
        printf "\033[5mSLOW-BLINK\033[0m plain\n"
        printf "\033[6mFAST-BLINK\033[0m plain\n"
        printf "\033[5;6mBOTH\033[0m \033[5mSLOW\033[25mCLEARED\033[0m\n"
        printf "\033[5;31;1mBLINK-BOLD-RED\033[0m \033[6;4mBLINK-UNDERLINE\033[0m\n"
    '
}

case_osc_signals() {
    run_in_session osc-signals '
        printf "\033]0;window-osc-title\007"
        printf "titled\n"
        printf "\033]8;id=1;https://example.com/one\033\\hyperlink\033]8;;\033\\ plain\n"
        printf "bell:\a\n"
        printf "\033]9;osc9 notification body\007"
        printf "\033]99;i=1:d=0;osc99 notification body\033\\"
        printf "signalled\n"
    '
}

case_reference_app() {
    local app="$1"
    if ! command -v "$app" >/dev/null 2>&1; then
        echo "capture: $app is not installed, skipping" >&2
        return 1
    fi
    run_in_session "$app" "$app"
}

requested=("$@")
if [[ ${#requested[@]} -eq 0 ]]; then
    requested=(smoke styling scroll-regions wide-chars glyph-torture cursor-shapes blink osc-signals kitty-graphics kitty-pane-open sixel-graphics sixel-pane-open panes-and-tabs vttest htop)
fi

for case_name in "${requested[@]}"; do
    case "$case_name" in
        smoke) capture smoke 8 case_smoke ;;
        styling) capture styling 8 case_styling ;;
        scroll-regions) capture scroll-regions 8 case_scroll_regions ;;
        wide-chars) capture wide-chars 8 case_wide_chars ;;
        glyph-torture) capture glyph-torture 8 case_glyph_torture ;;
        cursor-shapes) capture cursor-shapes 10 case_cursor_shapes ;;
        blink) capture blink 8 case_blink ;;
        osc-signals) capture osc-signals 8 case_osc_signals ;;
        kitty-graphics)
            if ! command -v base64 >/dev/null 2>&1; then
                echo "capture: base64 is not installed, skipping" >&2
                continue
            fi
            saved_cell="$cell_width $cell_height"
            cell_width="${CELL_WIDTH:-8}"
            cell_height="${CELL_HEIGHT:-20}"
            capture kitty-graphics 14 case_kitty_graphics
            read -r cell_width cell_height <<<"$saved_cell"
            ;;
        kitty-pane-open)
            if ! command -v base64 >/dev/null 2>&1; then
                echo "capture: base64 is not installed, skipping" >&2
                continue
            fi
            saved_cell="$cell_width $cell_height"
            cell_width="${CELL_WIDTH:-8}"
            cell_height="${CELL_HEIGHT:-20}"
            capture kitty-pane-open 16 case_kitty_pane_open
            read -r cell_width cell_height <<<"$saved_cell"
            ;;
        sixel-graphics)
            saved_cell="$cell_width $cell_height"
            cell_width="${CELL_WIDTH:-8}"
            cell_height="${CELL_HEIGHT:-20}"
            capture sixel-graphics 14 case_sixel_graphics
            read -r cell_width cell_height <<<"$saved_cell"
            ;;
        sixel-pane-open)
            saved_cell="$cell_width $cell_height"
            cell_width="${CELL_WIDTH:-8}"
            cell_height="${CELL_HEIGHT:-20}"
            capture sixel-pane-open 16 case_sixel_pane_open
            read -r cell_width cell_height <<<"$saved_cell"
            ;;
        panes-and-tabs) capture panes-and-tabs 14 case_panes_and_tabs ;;
        vttest)
            if command -v vttest >/dev/null 2>&1; then
                capture vttest 10 case_reference_app vttest
            else
                echo "capture: vttest is not installed, skipping" >&2
            fi
            ;;
        htop)
            if command -v htop >/dev/null 2>&1; then
                capture htop 10 case_reference_app htop
            else
                echo "capture: htop is not installed, skipping" >&2
            fi
            ;;
        *)
            echo "capture: unknown case $case_name" >&2
            exit 1
            ;;
    esac
done
