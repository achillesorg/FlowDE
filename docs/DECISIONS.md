# Decisions

Recorded autonomously by the backend agent; change any of them by editing this
file and the code together. "Open" items need the user.

| # | Topic | Decision | Why / notes |
|---|---|---|---|
| 1 | Language, layout | Rust 2021 workspace, one crate per daemon (lib with pure logic + thin bin), `flow-common` shared | Logic is unit-tested on macOS; bins stay tiny |
| 2 | Runtime | tokio `current_thread`; blocking work on dedicated threads only where a child process is streamed (dlp, ai) | idle cost, RAM |
| 3 | IPC | JSON lines over per-daemon Unix sockets plus D-Bus only where the ecosystem expects it | no bus dependency between FlowDE parts; scriptable |
| 4 | D-Bus crate | `zbus` 5, no default features, `tokio` feature, no libdbus | pure Rust: builds on macOS, no system packages |
| 5 | Naming | User brand wins: `kiln-notifs`, `kiln-lock`, `kiln-dm`, `kiln-wall`, `kiln-desktop-mode`, `noxal-ai`; `flow-*` for the rest | lead/user instruction (renamed from flow-notify / flow-lock) |
| 6 | Notifications bus name | `kiln-notifs` still owns `org.freedesktop.Notifications` | clients (notify-send) need that name |
| 7 | Supervisor | systemd user units preferred; `flowd` for non-systemd | postmarketOS may use OpenRC |
| 8 | Config | versioned TOML (`version = N`) with migration list; state as JSON/TOML in `$XDG_STATE_HOME/flowde`; atomic writes 0600 | |
| 9 | Settings schema | TOML schema embedded + drop-ins in `<config>/schema.d/`; drop-ins cannot override built-in keys | other components can add keys safely |
| 10 | Passcode | argon2id 19 MiB / t=2 / p=1 (OWASP minimum) | phones can afford it; params are embedded in the PHC string so they can be raised later |
| 11 | Lock enforcement | kiln-lock is policy and state; compositor enforces | a daemon cannot lock a Wayland session by itself |
| 12 | Interface themes | Four selectable themes resolved into one token struct: `liquid-glass` (default), `minimalist`, `material-you`, `anthropic-design` | user feature list |
| 13 | Anthropic Design | = Glaze Clay (flat cut paper, cream/terracotta/ink, hairlines, zero-blur offset shadow); colour role names identical to `glaze-design` | lets a shared crate replace this later |
| 14 | MaterialYou | Original tonal-palette maths in CIE L*a*b*/LCh (tone = L*) as an HCT stand-in, gamut-clamped chroma; no Google code or tables | CAM16 can replace the Lab step later without API change. Light/dark and standard/medium/high contrast supported |
| 15 | Icon packs | Independent of interface theme; freedesktop theme discovery with `Inherits` chain to hicolor; theme only supplies a default pack id; `appearance.icon_pack` + per-app override | user answer "icon packs". FlowDE extension keys `X-Flow-Style/License/Recolor` in index.theme |
| 16 | Quality ladder | full lensing -> blur only -> flat; down immediately, up one step after 10 s stable (hysteresis) | avoids flapping near battery/thermal limits and shader recompiles |
| 17 | GPU class | From DRM driver name; unknown driver => Low (blur only) | safe default |
| 18 | Wallpaper seed | Hue-histogram dominant colour (HSV, saturation weighted) from PPM/PNG; JPEG decode is TODO | avoids a heavy decoder dependency for now |
| 19 | Calendar | Local JSON store; ICS parse (VEVENT, basic RRULE, VALARM); UTC/floating times; CalDAV-ready fields | TZID and CalDAV are TODO |
| 20 | Clock | Absolute unix times; fires missed items once after resume; wake via `systemd-run --user --timer-property=WakeSystem=true` (best effort) | survives suspend if the manager may arm wake alarms |
| 21 | Camera | Enumeration (V4L2 sysfs) + permission prompt state only; capture is TODO (libcamera / PipeWire portal) | no hardware here |
| 22 | Media | MPRIS2 aggregation by polling while subscribed (5 s) | signal-based is TODO |
| 23 | Store | Same commands/data model as Kiln Store (`kiln-apps/kiln-store`): `pkexec apt-get`, `flatpak` CLI; validated identifiers; single-job queue | PackageKit backend TODO |
| 24 | FlowDLP | A yt-dlp style download service (user answer): drives the real `yt-dlp` binary first (`--progress-template`, `--dump-json`), allowlisted args, http(s) only, optional bubblewrap; native Rust extractor is a documented later goal behind the `Engine` trait | legal note: users are responsible for rights/ToS |
| 25 | KilnDM | Display manager/login service (confirmed). Unprivileged logic now; PAM and session launch are TODO and will make it the one root service | |
| 26 | KilnDesktop | Dock detection from DRM connector sysfs (poll 3 s, 1 s while settling), policy auto/ask/never, decline remembered until unplug | udev hotplug events are TODO (needs netlink socket) |
| 27 | NoxalAi | Local Qwen (ChatML, `<tool_call>` format); runtime trait with mock and llama.cpp CLI backends; no model downloads; gating on battery/thermal/RAM; idle unload | server and in-process backends TODO |
| 28 | Assistant tools | Allowlist of 5; setting keys limited; per-tool ask/allow/deny; one tool round per reply | multi-step agent loop is TODO |
| 29 | Privacy | Conversations local, optional encrypted at rest, wipe call, no telemetry | |
| 30 | Toolchain | `rust-toolchain.toml` = stable + rustfmt + clippy; MSRV declared 1.75 but not verified | on shared machines set `RUSTUP_TOOLCHAIN` so rustup does not update the toolchain |
| 31 | Licence | MIT for the repo; dependencies MIT/Apache-2.0 compatible (see LICENSING.md and deny.toml) | |

## Open questions for the user

See the final report section "Questions" and docs/FEATURES-INBOX.md mapping.
