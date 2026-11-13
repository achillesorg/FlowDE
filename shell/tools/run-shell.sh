#!/usr/bin/env bash
# VM: (re)start the shell inside the throwaway Sway. Env passthrough: FLOWDE_START/THEME/SCHEME/GLASS...
eval "$(~/flowde-shell-build/tools/sway-headless.sh start ${SIZE:-540x1170})"
export QT_QPA_PLATFORM=wayland FLOWDE_SHELL_SOCKET=$XDG_RUNTIME_DIR/flowde-shell.sock FLOWDE_FPS=1 QSG_RHI_BACKEND=opengl
pkill -x flowde-shell 2>/dev/null
nohup ~/flowde-shell-build/b-shell/flowde-shell >/tmp/shell.log 2>&1 &
sleep ${WAIT:-6}
