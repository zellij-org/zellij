#!/usr/bin/env bash
set -euo pipefail

VERSION="${IOSEVKA_VERSION:-34.8.1}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

RANGES='U+0000-024F,U+0250-02AF,U+02B0-02FF,U+0300-036F,U+0370-03FF,U+0400-04FF,U+2000-206F,U+2070-209F,U+20A0-20BF,U+2100-214F,U+2150-218F,U+2190-21FF,U+2200-22FF,U+2300-23FF,U+2400-243F,U+2500-257F,U+2580-259F,U+25A0-25FF,U+2600-26FF,U+2700-27BF,U+E0A0-E0D4,U+FFFD'

CJK_RANGES='U+3000-303F,U+3040-30FF,U+31F0-31FF,U+3130-318F,U+FF01-FF60,U+FFE0-FFE6'
CJK_TEXT='你好世界漢字中文日本語東京大阪山川水火木金土人手口目耳心力刀田石竹米糸貝車門雨青草虫犬牛魚鳥花空天地男女子学校先生年月時分間今何前後上下左右内外高安新古長短多少白黒赤黄한글안녕하세요각'

EMOJI_RANGES='*'

CJK_URL="${CJK_URL:-https://github.com/notofonts/noto-cjk/raw/main/Sans/Mono/NotoSansMonoCJKsc-Regular.otf}"
EMOJI_URL="${EMOJI_URL:-https://github.com/googlefonts/noto-emoji/raw/main/2D/fonts/NotoColorEmoji.ttf}"
NERD_FONTS_VERSION="${NERD_FONTS_VERSION:-3.5.1}"
SYMBOLS_URL="${SYMBOLS_URL:-https://github.com/ryanoasis/nerd-fonts/releases/download/v${NERD_FONTS_VERSION}/NerdFontsSymbolsOnly.zip}"
FONTS="${FONTS:-iosevka cjk emoji symbols}"

wanted() { [[ " $FONTS " == *" $1 "* ]]; }

if command -v pyftsubset >/dev/null; then
    subset() { pyftsubset "$@"; }
elif python3 -c 'import fontTools' 2>/dev/null; then
    subset() { python3 -m fontTools.subset "$@"; }
else
    echo "fonttools not found; pip install fonttools brotli" >&2
    exit 1
fi

if wanted iosevka; then
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
        --layout-features=calt,ccmp,mark,mkmk \
        --name-IDs+=13,14 \
        --no-hinting \
        --desubroutinize \
        --output-file="$HERE/IosevkaTerm-$style.ttf"
done

curl -fsSL -o "$HERE/LICENSE.md" \
    "https://raw.githubusercontent.com/be5invis/Iosevka/v${VERSION}/LICENSE.md"
fi

if wanted cjk; then
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
fi

if wanted emoji; then
echo "fetching $EMOJI_URL"
curl -fsSL -o "$WORK/emoji.ttf" "$EMOJI_URL"
echo "subsetting NotoColorEmoji"
subset "$WORK/emoji.ttf" \
    --unicodes="$EMOJI_RANGES" \
    --layout-features='*' \
    --layout-scripts='*' \
    --name-IDs+=13,14 \
    --no-hinting \
    --output-file="$HERE/NotoColorEmoji.ttf"

curl -fsSL -o "$HERE/LICENSE-NotoColorEmoji.md" \
    "https://raw.githubusercontent.com/googlefonts/noto-emoji/main/2D/fonts/LICENSE"
fi

if wanted symbols; then
echo "fetching $SYMBOLS_URL"
curl -fsSL -o "$WORK/symbols.zip" "$SYMBOLS_URL"
python3 - "$WORK" "$HERE" <<'PY'
import sys, zipfile
work, here = sys.argv[1], sys.argv[2]
z = zipfile.ZipFile(work + "/symbols.zip")
open(here + "/SymbolsNerdFontMono-Regular.ttf", "wb").write(z.read("SymbolsNerdFontMono-Regular.ttf"))
open(here + "/LICENSE-SymbolsNerdFont.md", "wb").write(z.read("LICENSE"))
PY
fi

echo "done; regenerate PNG goldens with UPDATE_GOLDENS=1 cargo test -p zellij-window"
