#!/usr/bin/env bash
set -euo pipefail

VERSION="${IOSEVKA_VERSION:-34.8.1}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

RANGES='U+0000-024F,U+0250-02AF,U+02B0-02FF,U+0300-036F,U+0370-03FF,U+0400-04FF,U+2000-206F,U+2070-209F,U+20A0-20BF,U+2100-214F,U+2150-218F,U+2190-21FF,U+2200-22FF,U+2300-23FF,U+2400-243F,U+2500-257F,U+2580-259F,U+25A0-25FF,U+2600-26FF,U+2700-27BF,U+E0A0-E0D4,U+FFFD'

CJK_RANGES='U+3000-303F,U+3040-30FF,U+31F0-31FF,U+3130-318F,U+FF01-FF60,U+FFE0-FFE6'
CJK_TEXT='你好世界漢字中文日本語東京大阪山川水火木金土人手口目耳心力刀田石竹米糸貝車門雨青草虫犬牛魚鳥花空天地男女子学校先生年月時分間今何前後上下左右内外高安新古長短多少白黒赤黄한글안녕하세요'

EMOJI_RANGES='U+FE0F,U+20E3,U+00A9,U+00AE,U+2122,U+2139,U+2194-2199,U+21A9-21AA,U+231A-231B,U+23F0,U+23F3,U+24C2,U+25B6,U+26A0,U+26A1,U+2600-2603,U+2611,U+2614-2615,U+2620,U+2622-2623,U+262F,U+2639-263A,U+2660,U+2663,U+2665,U+2666,U+2693,U+26D4,U+2705,U+2708-270D,U+2712,U+2714,U+2716,U+271D,U+2728,U+2733-2734,U+2744,U+2747,U+274C,U+274E,U+2753-2755,U+2757,U+2764,U+2795-2797,U+27A1,U+27B0,U+2B05-2B07,U+2B1B-2B1C,U+2B50,U+2B55,U+1F004,U+1F0CF,U+1F195,U+1F308,U+1F30A,U+1F30D,U+1F319,U+1F31F,U+1F332-1F333,U+1F337-1F33B,U+1F344,U+1F349,U+1F34E,U+1F355,U+1F356,U+1F35E,U+1F37A,U+1F382,U+1F389,U+1F38A,U+1F408,U+1F40D,U+1F411,U+1F419,U+1F41B,U+1F41F,U+1F426,U+1F42D,U+1F431,U+1F436,U+1F437,U+1F440,U+1F44B,U+1F44D,U+1F44E,U+1F44F,U+1F451,U+1F464,U+1F4A1,U+1F4A9,U+1F4AA,U+1F4BB,U+1F4C1,U+1F4C4,U+1F4CA,U+1F4CC,U+1F4D6,U+1F4E6,U+1F4E9,U+1F4F1,U+1F4FA,U+1F500,U+1F511,U+1F512,U+1F514,U+1F525,U+1F52B,U+1F600-1F60F,U+1F612-1F614,U+1F618,U+1F61C,U+1F621,U+1F622,U+1F62D,U+1F631,U+1F633,U+1F634,U+1F642,U+1F644,U+1F680,U+1F6A8,U+1F6E0,U+1F914,U+1F926,U+1F937,U+1F947,U+1F984,U+1F9E0'

CJK_URL="${CJK_URL:-https://github.com/notofonts/noto-cjk/raw/main/Sans/Mono/NotoSansMonoCJKsc-Regular.otf}"
EMOJI_URL="${EMOJI_URL:-https://github.com/googlefonts/noto-emoji/raw/main/fonts/NotoColorEmoji.ttf}"

if command -v pyftsubset >/dev/null; then
    subset() { pyftsubset "$@"; }
elif python3 -c 'import fontTools' 2>/dev/null; then
    subset() { python3 -m fontTools.subset "$@"; }
else
    echo "fonttools not found; pip install fonttools brotli" >&2
    exit 1
fi

url="https://github.com/be5invis/Iosevka/releases/download/v${VERSION}/PkgTTF-IosevkaTerm-${VERSION}.zip"
echo "fetching $url"
curl -fsSL -o "$WORK/pkg.zip" "$url"

python3 - "$WORK" <<'PY'
import sys, zipfile
work = sys.argv[1]
z = zipfile.ZipFile(work + "/pkg.zip")
for style in ("Regular", "Bold", "Italic", "BoldItalic"):
    name = "IosevkaTerm-%s.ttf" % style
    open("%s/%s" % (work, name), "wb").write(z.read(name))
PY

for style in Regular Bold Italic BoldItalic; do
    echo "subsetting $style"
    subset "$WORK/IosevkaTerm-$style.ttf" \
        --unicodes="$RANGES" \
        --layout-features=calt \
        --drop-tables+=GPOS \
        --name-IDs+=13,14 \
        --no-hinting \
        --desubroutinize \
        --output-file="$HERE/IosevkaTerm-$style.ttf"
done

curl -fsSL -o "$HERE/LICENSE.md" \
    "https://raw.githubusercontent.com/be5invis/Iosevka/v${VERSION}/LICENSE.md"

echo "fetching $CJK_URL"
curl -fsSL -o "$WORK/cjk.otf" "$CJK_URL"
printf '%s' "$CJK_TEXT" >"$WORK/cjk.txt"
echo "subsetting NotoSansMonoCJK"
subset "$WORK/cjk.otf" \
    --unicodes="$CJK_RANGES" \
    --text-file="$WORK/cjk.txt" \
    --layout-features='' \
    --drop-tables+=GSUB,GPOS,GDEF,DSIG,BASE \
    --name-IDs+=13,14 \
    --no-hinting \
    --desubroutinize \
    --output-file="$HERE/NotoSansMonoCJK-Regular.otf"

curl -fsSL -o "$HERE/LICENSE-NotoSansMonoCJK.md" \
    "https://raw.githubusercontent.com/notofonts/noto-cjk/main/Sans/LICENSE"

echo "fetching $EMOJI_URL"
curl -fsSL -o "$WORK/emoji.ttf" "$EMOJI_URL"
echo "subsetting NotoColorEmoji"
subset "$WORK/emoji.ttf" \
    --unicodes="$EMOJI_RANGES" \
    --layout-features='' \
    --drop-tables+=GSUB,GPOS,GDEF \
    --name-IDs+=13,14 \
    --no-hinting \
    --output-file="$HERE/NotoColorEmoji.ttf"

curl -fsSL -o "$HERE/LICENSE-NotoColorEmoji.md" \
    "https://raw.githubusercontent.com/googlefonts/noto-emoji/main/fonts/LICENSE"

echo "done; regenerate PNG goldens with UPDATE_GOLDENS=1 cargo test -p zellij-window"
