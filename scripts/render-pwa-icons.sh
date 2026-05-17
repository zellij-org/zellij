#!/bin/sh
# Renders PWA icon PNGs from assets/logo.svg into zellij-client/assets/.
# Re-run from the repository root after logo.svg changes:
#
#     scripts/render-pwa-icons.sh
#
# Requires: librsvg (rsvg-convert) and ImageMagick (convert).
#
# Produces two PNGs:
#   icon-192.png    192x192, transparent, logo centered      (purpose "any")
#   icon-512.png    512x512, transparent, logo centered      (purpose "any")
#
# The logo SVG is taller than wide (2307x2664). rsvg-convert preserves aspect
# ratio when scaling to a target height; ImageMagick then composites the result
# onto a transparent square canvas so the manifest's declared "192x192" /
# "512x512" sizes are honest.

set -eu

cd "$(dirname "$0")/.."

SRC_SVG="assets/logo.svg"
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

echo "Rendering PWA icons from $SRC_SVG:"
render_any 192
render_any 512

echo "Done."
