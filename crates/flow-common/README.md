# flow-common

Shared library: versioned TOML config with migrations, paths (env-overridable), tracing logging, JSON-lines IPC server/client, health/metrics, civil time helpers, daemon bootstrap.

**Status:** Implemented and tested.

## TODO(feature)

- Peer credential check (SO_PEERCRED) in addition to file permissions
- Socket activation (LISTEN_FDS) support
