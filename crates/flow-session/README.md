# flow-session

`flowd`: user-session supervisor. Dependency order, restart backoff, crash-loop breaker, status/start/stop/restart/health over the socket.

**Status:** Implemented. Spawns the daemon binaries found next to flowd or in PATH.

## TODO(feature)

- Socket activation instead of eager start
- Readiness notification (wait for the unit's socket before starting dependents)
- Log capture per unit
