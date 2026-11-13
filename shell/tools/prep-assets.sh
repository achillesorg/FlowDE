#!/usr/bin/env bash
# Regenerate shell/assets from the FeDe repo (read-only source): ink icons recoloured SVG copies in two inks (needs the Qt SVG image plugin: qt6-svg-plugins),
# Kiln wallpapers copied. Credit: FeDe/Kiln artwork, MIT OR Apache-2.0 (fede/assets/wallpapers/LICENSE.md).
set -e
R="$(cd "$(dirname "$0")/../../.." && pwd)"; F="$R/fede/assets"; [ -d "$F" ] || F="$R/glaze/assets"   # the FeDe repo was renamed (glaze)
H="$(cd "$(dirname "$0")/.." && pwd)/assets"
SRC="$(cd "$(dirname "$0")/.." && pwd)/assets-src/icons"
for n in apps browser calendar clock folder lock search settings terminal bell volume-high network-wifi power sleep phone camera photos music maps notes weather wallet chat app noxal; do
  IN="$F/icons/ink/stroke-1.5/$n.svg"; [ -f "$IN" ] || IN="$SRC/$n.svg"   # own 1.5px line icons where FeDe/Glaze has none
  for v in "dark:#2B2A26" "light:#FAF9F5"; do
    name=${v%%:*}; col=${v#*:}   # "dark" = dark ink (for light themes)
    sed "s/currentColor/$col/g" "$IN" > "$H/icons/$n-$name.svg"
  done
done
cp "$F/wallpapers/kiln/kiln-light-portrait-1080x2340.png" "$H/wallpapers/kiln-light.png"
cp "$F/wallpapers/night-paper/night-paper-dark-portrait-1080x2340.png" "$H/wallpapers/night-paper-dark.png"
