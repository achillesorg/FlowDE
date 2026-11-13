#!/bin/sh
# Binary size budget check. Usage: packaging/size-check.sh [target-dir]
# Builds release binaries unless SKIP_BUILD=1. Fails if a daemon exceeds
# MAX_DAEMON_KB (default 2560) or `flow` exceeds MAX_CLI_KB (default 1024).
set -eu
T=${1:-target}/release
[ "${SKIP_BUILD:-0}" = 1 ] || cargo build --release --workspace
MAXD=${MAX_DAEMON_KB:-2560}; MAXC=${MAX_CLI_KB:-1024}
fail=0
for f in "$T"/flow "$T"/flowd "$T"/flow-* "$T"/kiln-* "$T"/noxal-ai; do
  [ -f "$f" ] && [ -x "$f" ] || continue
  case "$f" in *.d) continue;; esac
  kb=$(( $(wc -c < "$f") / 1024 ))
  lim=$MAXD; [ "$(basename "$f")" = flow ] && lim=$MAXC
  st=ok; [ "$kb" -gt "$lim" ] && { st="OVER ($lim)"; fail=1; }
  printf '%-20s %6s KiB  %s\n' "$(basename "$f")" "$kb" "$st"
done
exit $fail
