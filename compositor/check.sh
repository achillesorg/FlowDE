#!/usr/bin/env bash
# Mac -> VM: sync compositor/ and cargo <check|build> under the shared heavy-build lock. Target dir lives in the VM.
set -e
D="$(cd "$(dirname "$0")/../.." && pwd)"
rsync -a --delete --exclude target -e "ssh -i $D/fedeos_key -p 2222 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR" "$D/flowde-mobile/compositor/" fede@localhost:flowde-shell-build/compositor/
"$D/vm" ". ~/.cargo/env; cd ~/flowde-shell-build/compositor && CARGO_TARGET_DIR=~/flowde-shell-build/target-comp flock /tmp/heavy-build.lock cargo ${1:-check} -j2 2>&1 | grep -E '^(error|warning)|-->|Finished|^ *= ' | head -${2:-60}"
