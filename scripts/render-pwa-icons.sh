#!/bin/sh
# Renders PWA icon PNGs from zellij-client/assets/icon.svg.
# Re-run from the repository root after icon.svg changes:
#
#     scripts/render-pwa-icons.sh
#
# Requires: librsvg (rsvg-convert) and ImageMagick (convert).
#
# Produces three PNGs alongside icon.svg:
#   icon-192.png             192x192, transparent, logo centered      (purpose "any")
#   icon-512.png             512x512, transparent, logo centered      (purpose "any")
#   icon-maskable-512.png    512x512, #080317 fill, logo at 70% scale (purpose "maskable")
#
# The maskable scale (70% of canvas, glyph height 358 of 512) keeps the logo's
# vertical tips inside the W3C safe-zone radius of 40% (205px), with margin to
# spare for aggressive circular and squircle masks.

set -eu

cd "$(dirname "$0")/.."

SRC_SVG="zellij-client/assets/icon.svg"
OUT_DIR="zellij-client/assets"

if [ ! -f "$SRC_SVG" ]; then
    echo "error: $SRC_SVG not found (run from repository root)" >&2
    exit 1
fi

command -v rsvg-convert >/dev/null 2>&1 || { echo "error: rsvg-convert (librsvg) required" >&2; exit 1; }
command -v convert >/dev/null 2>&1 || { echo "error: ImageMagick (convert) required" >&2; exit 1; }

render_any() {
    size=$1
    out="$OUT_DIR/icon-${size}.png"
    tmp=$(mktemp --suffix=.png)
    rsvg-convert -h "$size" -o "$tmp" "$SRC_SVG"
    convert -size "${size}x${size}" xc:none "$tmp" -gravity center -composite "$out"
    rm -f "$tmp"
    echo "  $out"
}

render_maskable() {
    canvas=512
    glyph=358   # 70% of 512 — content sits inside the 80%-diameter safe zone
    bg="#080317"
    out="$OUT_DIR/icon-maskable-${canvas}.png"
    tmp=$(mktemp --suffix=.png)
    rsvg-convert -h "$glyph" -o "$tmp" "$SRC_SVG"
    convert -size "${canvas}x${canvas}" "xc:${bg}" "$tmp" -gravity center -composite "$out"
    rm -f "$tmp"
    echo "  $out"
}

echo "Rendering PWA icons from $SRC_SVG:"
render_any 192
render_any 512
render_maskable

echo "Done."
