#!/usr/bin/env bash
# From the Mac: sync shell/ to the VM and (nice) rebuild. Light build (4 C++ files): runs without the heavy lock.
set -e
D="$(cd "$(dirname "$0")/../../.." && pwd)"
rsync -a --delete --exclude tools -e "ssh -i $D/fedeos_key -p 2222 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR" "$D/flowde-mobile/shell/" fede@localhost:flowde-shell-build/shell/
"$D/vm" 'cd ~/flowde-shell-build && cmake -S shell -B b-shell >/dev/null; nice cmake --build b-shell -j2 2>&1 | grep -E "error|Error|FAILED|warning: .*qml" -A3 | head -40'
