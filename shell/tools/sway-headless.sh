#!/usr/bin/env bash
# Start/stop a THROWAWAY headless Sway for portrait tests (never the live session).
#   sway-headless.sh start [WxH]   -> prints env to eval
#   sway-headless.sh stop
export XDG_RUNTIME_DIR=/tmp/xdg-flow
case "${1:-start}" in
start)
  SIZE="${2:-540x1170}"
  mkdir -p "$XDG_RUNTIME_DIR"; chmod 700 "$XDG_RUNTIME_DIR"
  printf 'default_border none\nfont pango:monospace 1\nfocus_follows_mouse no\noutput * mode %s\n' "$SIZE" > "$XDG_RUNTIME_DIR/sway.conf"
  WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=${WLR_RENDERER:-pixman} \
    nohup sway -c "$XDG_RUNTIME_DIR/sway.conf" >"$XDG_RUNTIME_DIR/sway.log" 2>&1 &
  echo $! > "$XDG_RUNTIME_DIR/sway.pid"
  for i in $(seq 50); do ls "$XDG_RUNTIME_DIR"/wayland-* >/dev/null 2>&1 && break; sleep 0.2; done
  sleep 0.5
  export WAYLAND_DISPLAY=$(basename "$(ls "$XDG_RUNTIME_DIR"/wayland-[0-9] | head -1)")
  export SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock | head -1)
  swaymsg output HEADLESS-1 resolution "$SIZE" >/dev/null 2>&1 || true
  echo "export XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR WAYLAND_DISPLAY=$WAYLAND_DISPLAY SWAYSOCK=$SWAYSOCK"
  ;;
stop)
  [ -f "$XDG_RUNTIME_DIR/sway.pid" ] && kill "$(cat "$XDG_RUNTIME_DIR/sway.pid")" 2>/dev/null
  sleep 0.5; rm -rf "$XDG_RUNTIME_DIR"
  ;;
esac
