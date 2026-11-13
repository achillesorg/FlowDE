#!/usr/bin/env bash
# VM: drive the running shell over its control socket and grim every state. Output: ~/flowde-shell-build/shots/
export XDG_RUNTIME_DIR=/tmp/xdg-flow WAYLAND_DISPLAY=wayland-1
S=$XDG_RUNTIME_DIR/flowde-shell.sock; P=~/flowde-shell-build/tools/send.py; O=~/flowde-shell-build/shots; mkdir -p $O
set_() { python3 $P $S "{\"set\":\"$1\",\"value\":\"$2\"}"; }
shot() { sleep ${W:-1.6}; grim $O/$1.png; }
set_ theme LiquidGlass; set_ scheme light
set_ screen lock;     shot lock
set_ screen home;     shot home
set_ screen search;   shot search
set_ screen switcher; shot switcher
set_ screen shade;    shot shade
set_ scheme dark; set_ screen lock; shot lock-dark
set_ screen home;     shot home-dark
set_ screen shade;    shot shade-dark
set_ scheme light; set_ screen home
for t in Minimalist MaterialYou AnthropicDesign; do set_ theme $t; set_ screen home; shot home-$t; set_ screen shade; shot shade-$t; done
set_ theme LiquidGlass; set_ screen home
set_ glass blur; shot home-blur
set_ glass flat; shot home-flat
set_ glass full; set_ reduceTransparency 1; shot home-reduce-transparency; set_ reduceTransparency 0
