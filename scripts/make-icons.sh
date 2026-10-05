#!/bin/sh
#
# The app icon's derived files, from the icon's source PNG.
#
#   in:  crates/whirl-ui/assets/app-icon-1024.png
#   out: app-icon.icns                    macOS, 16..1024 px (iconutil)
#        app-icon.ico                     Windows, one 256 px image
#        app-icon-{128,256,512}.png       Linux and anything else
#
# Needs only what macOS ships: sips and iconutil.
#
# The menu bar marks are not built here. They are 16x16 and 32x32
# rasterisations of the three SVGs beside them, and no macOS built-in rasterises
# an SVG with an alpha channel (qlmanage draws one onto white, which is exactly
# what a template image must not be), so those PNGs are committed as the sources.
# With rsvg-convert on the PATH this script re-rasterises them, and the 1024
# source, from the SVGs, which is how the committed ones were made; without it,
# the files already in assets/ are used as they are.
#
# usage: scripts/make-icons.sh

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
assets="$root/crates/whirl-ui/assets"
source_png="$assets/app-icon-1024.png"

for tool in sips iconutil; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "make-icons: $tool is not on PATH; sips and iconutil both ship with macOS" >&2
        exit 1
    fi
done

if command -v rsvg-convert >/dev/null 2>&1; then
    rsvg-convert -w 16 -h 16 "$assets/mark.svg" -o "$assets/tray-iconTemplate.png"
    rsvg-convert -w 32 -h 32 "$assets/mark.svg" -o "$assets/tray-iconTemplate@2x.png"
    rsvg-convert -w 16 -h 16 "$assets/mark-paused.svg" -o "$assets/tray-icon-pausedTemplate.png"
    rsvg-convert -w 32 -h 32 "$assets/mark-paused.svg" -o "$assets/tray-icon-pausedTemplate@2x.png"
    rsvg-convert -w 16 -h 16 "$assets/mark-unreachable.svg" -o "$assets/tray-icon-unreachableTemplate.png"
    rsvg-convert -w 32 -h 32 "$assets/mark-unreachable.svg" -o "$assets/tray-icon-unreachableTemplate@2x.png"
    rsvg-convert -w 1024 -h 1024 "$assets/app-icon.svg" -o "$source_png"
    echo "make-icons: redrew the six tray rasters and the 1024 source from the SVGs"
else
    echo "make-icons: rsvg-convert is not on PATH, so the SVGs are not re-rasterised;" >&2
    echo "            the committed PNGs in assets/ are used as they are" >&2
fi

if ! sips -g pixelWidth "$source_png" | grep -q 'pixelWidth: 1024'; then
    echo "make-icons: $source_png is not 1024 px wide" >&2
    exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# The rasters iconutil reads out of an .iconset, ten of them.
iconset="$work/app-icon.iconset"
mkdir "$iconset"
for size in 16 32 128 256 512; do
    twice=$((size * 2))
    sips -z "$size" "$size" "$source_png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$twice" "$twice" "$source_png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil --convert icns --output "$assets/app-icon.icns" "$iconset"

for size in 128 256 512; do
    sips -z "$size" "$size" "$source_png" --out "$assets/app-icon-$size.png" >/dev/null
done

# sips writes a single-image ICO, not the multi-size one Windows prefers; that
# is a tool this macOS does not carry, so the 256 px image goes in and the
# limitation is stated rather than worked around.
sips -s format ico -z 256 256 "$source_png" --out "$assets/app-icon.ico" >/dev/null

echo "make-icons: wrote app-icon.icns, app-icon.ico and app-icon-{128,256,512}.png"
