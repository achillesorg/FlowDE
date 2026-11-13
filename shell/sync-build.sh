#!/usr/bin/env bash
# Sync shell/ to the FedeOS VM and build it (heavy build lock). Usage: shell/sync-build.sh
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"; VM="$HERE/../../vm"
rsync -a --delete -e "ssh -i $HERE/../../fedeos_key -p 2222 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR" \
  "$HERE/" fede@localhost:flowde-shell-build/shell/
"$VM" 'cd ~/flowde-shell-build/shell && cmake -S . -B ../b-shell -G Ninja -DCMAKE_BUILD_TYPE=Release >/dev/null && flock /tmp/heavy-build.lock cmake --build ../b-shell -j2 2>&1 | tail -40'
