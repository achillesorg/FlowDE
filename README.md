# FlowDE

FlowDE is a mobile desktop environment for Linux phones (Debian, Fedora and
postmarketOS style systems, aarch64 first). This repository holds the services/backend layer (`crates/`),
a Qt 6/QML shell prototype (`shell/`) and a nested Smithay mobile compositor
(`compositor/`). Everything is an early prototype.

Goals: optimised for a phone (tiny daemons, idle means idle), a proper
"liquid glass" default look (original implementation, see
[docs/liquid-glass.md](docs/liquid-glass.md)), and a clean crossover with
[Glaze](https://github.com/achillesorg) (the desktop sibling; KilnDesktop hands
over to it when a phone is docked).

## Status

Early scaffold, Rust 2021 workspace. All crates build and unit-test on macOS
and Linux; the daemons are Linux-only at runtime. Everything that needs
hardware or a bus that is not present is an honest stub that returns
`unavailable` (never fake data). See each crate README for its `TODO(feature)`
list and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the design.

| Crate / binary | Role |
|---|---|
| `flow-common` | config (versioned TOML), paths, logging, JSON-lines IPC, health, time helpers |
| `flow-session` (`flowd`) | supervisor with dependency order and restart backoff |
| `kiln-notifs` | `org.freedesktop.Notifications`, DND, grouping, history |
| `kiln-lock` | lock state machine, argon2id passcode, biometric hook |
| `kiln-dm` | display-manager backend (sessions, autologin policy, greeter state machine) |
| `kiln-wall` | wallpaper service, rotation, dominant colour seed |
| `kiln-desktop-mode` | KilnDesktop: dock detection and Glaze handover |
| `noxal-ai` | local Qwen-based assistant with safe tool layer |
| `flow-power` / `flow-net` / `flow-sensors` / `flow-haptics` | hardware services |
| `flow-settings` | typed settings store with schema |
| `flow-apps` | XDG app registry, safe launch, frecency, permissions |
| `flow-theme` | liquid glass, minimalist, MaterialYou, Anthropic Design, icon packs |
| `flow-calendar` / `flow-clock` / `flow-camera` / `flow-media` | app backends |
| `flow-store` / `flow-dlp` | app catalogue backend; yt-dlp style download service |
| `flow-cli` (`flow`) | command line client |

## Build and test

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --workspace   # opt-level=z, LTO, strip, panic=abort
```

Try the CLI against a running daemon:

```sh
flow-settings &            # starts the control socket in $XDG_RUNTIME_DIR/flowde/
flow settings set appearance.theme material-you
flow theme tokens
flow status
```

## Docs

- [Architecture](docs/ARCHITECTURE.md) - daemon map, IPC, power budget, security
- [Decisions](docs/DECISIONS.md)
- [Liquid glass](docs/liquid-glass.md) and [shader sketches](docs/liquid-glass/)
- [Budgets](docs/BUDGETS.md) - measured binary sizes and memory
- [Feature inbox](docs/FEATURES-INBOX.md)
- [Credits](docs/CREDITS.md), [licences](docs/LICENSING.md)

## Legal

MIT licensed. `flow-dlp` drives third-party tools (yt-dlp); users are
responsible for having the right to download content and for each site's terms
of service and local law.
