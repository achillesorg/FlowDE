# FlowDE architecture

## Daemon map

Everything is a small, single-purpose user daemon. Each daemon owns a Unix
control socket `$XDG_RUNTIME_DIR/flowde/<name>.sock` (mode 0600, directory 0700)
speaking JSON lines (below). Where a freedesktop or FlowDE D-Bus interface
exists it is listed too.

| Daemon | Socket name | Bus | Talks to |
|---|---|---|---|
| `flowd` (flow-session) | `flowd` | - | spawns/monitors all below |
| `flow-settings` | `flow-settings` | session: `org.flowde.Settings1` | event source for theme, others |
| `kiln-notifs` | `kiln-notifs` | session: owns `org.freedesktop.Notifications` | - |
| `flow-power` | `flow-power` | system: UPower, power-profiles-daemon, logind | emits `power.changed` |
| `flow-sensors` | `flow-sensors` | system: iio-sensor-proxy (+ IIO sysfs tilt) | emits `sensors.changed` |
| `flow-haptics` | `flow-haptics` | session: feedbackd; sysfs vibrator | follows flow-power |
| `flow-net` | `flow-net` | system: NetworkManager, ModemManager | - |
| `kiln-lock` | `kiln-lock` | - | emits `lock.changed` |
| `flow-apps` | `flow-apps` | - | launches processes |
| `flow-theme` | `flow-theme` | session: `org.flowde.Theme1` | follows settings, power, sensors |
| `kiln-wall` | `kiln-wall` | - | pushes seed to flow-theme |
| `flow-clock` | `flow-clock` | - | `systemd-run --user` wake timers |
| `flow-calendar` | `flow-calendar` | - | - |
| `flow-media` | `flow-media` | session: MPRIS2 players | - |
| `flow-camera` | `flow-camera` | - (portal TODO) | - |
| `flow-store` | `flow-store` | - | apt-cache, pkexec apt-get, flatpak |
| `flow-dlp` | `flow-dlp` | - | yt-dlp (optionally under bubblewrap) |
| `kiln-desktop-mode` | `kiln-desktop-mode` | - (DRM sysfs) | follows flow-power; starts Glaze session |
| `noxal-ai` | `noxal-ai` | session: `org.flowde.Noxal1` | inference child process; tool calls to other daemons |
| `kiln-dm` | `kiln-dm` | system: logind (read) | system service, see security |

## IPC protocol (flow-common::ipc)

```
-> {"id":1,"method":"settings.get","params":{"key":"display.brightness"}}
<- {"id":1,"result":0.6}
<- {"id":1,"error":{"code":"not_found","message":"..."}}
-> {"id":2,"method":"subscribe"}           # then the server pushes:
<- {"event":"settings.changed","data":{"key":"...","value":...}}
```

Built-in on every daemon: `ping`, `health` (name, version, uptime, request and
error counters, RSS in kB) and `subscribe`. Lines are capped at 1 MiB; garbage
gets an `invalid` error, never a crash. A second daemon instance refuses to
bind (stale sockets are replaced). Events are only built when somebody is
subscribed, so an idle daemon does no serialisation work.

Event names: `power.changed`, `sensors.changed`, `settings.changed`,
`theme.changed` (tokens plus icon pack), `notify.changed`, `lock.changed`,
`wall.changed`, `clock.fired`, `calendar.reminder`, `media.changed`,
`dlp.progress`/`dlp.job`, `ai.token`/`ai.done`/`ai.tool`/`ai.permission`,
`desktop.action`/`desktop.input_redirect`, `session.unit`.

D-Bus is used where the ecosystem expects it (notifications, UPower, NM, MPRIS,
logind). Between FlowDE components the socket is used: it needs no bus daemon
round trip, is trivially scriptable (`flow ipc DAEMON METHOD JSON`) and works
for the compositor and shell without a D-Bus binding.

## Power budget principles

Target: a daemon that has nothing to do uses no CPU and as little memory as
possible.

1. Single-threaded tokio runtime (`current_thread`), no worker pool.
2. No fixed-rate timers while idle. Timers sleep until the next real deadline
   (alarm, reminder, idle threshold) with a cap of 30 s only where a clock jump
   or resume must be noticed. Polling that cannot be avoided is adaptive: 5 s
   when warm, 30 s on AC, 60 s on battery, 120 s in low-power mode (flow-power).
3. Sensors are claimed only while a client is subscribed (flow-sensors); IIO
   tilt is read at 2 Hz only while subscribed.
4. Events are not serialised without subscribers.
5. Heavy or rare services (`flow-store`, `flow-dlp`, `flow-camera`, `noxal-ai`)
   are not autostarted; flowd starts them on demand with their dependencies.
6. Everything adapts to power: low-power mode (state in flow-power) halves idle
   timers, lowers haptics, caps the theme quality ladder at blur-only; battery
   <= 10 % or hot thermal state drops the glass to flat; NoxalAi refuses to run
   below 30 % battery (unless charging), when warm, or without free RAM.
7. Release profile: `opt-level = "z"`, LTO, one codegen unit, `panic = "abort"`,
   stripped. Measured sizes are in [BUDGETS.md](BUDGETS.md).

### Idle CPU / RAM goals (per daemon)

| Class | Idle CPU | RSS goal | systemd `MemoryMax` |
|---|---|---|---|
| core small (settings, sensors, haptics, media, camera, clock, calendar, power, notifs, net, theme, apps, lock, wall, desktop-mode) | < 0.1 % of one core averaged, 0 wakeups when no deadline | < 6 MB | 16 to 48 MB |
| flowd | same, plus child supervision | < 6 MB | - |
| flow-store / flow-dlp while idle | same | < 8 MB | 48 / 128 MB |
| noxal-ai idle (model unloaded) | same | < 8 MB | 3 GB cap while a model is loaded |

These are goals; measured numbers are in BUDGETS.md and say what was and was
not measured.

## Interface themes, icon packs and the quality ladder

`flow-theme` resolves one token structure (colour roles named like Glaze's
`glaze-design` `ThemeRole` keys, geometry, shadow, optional glass material) for
four selectable interface themes: `liquid-glass` (default), `minimalist`,
`material-you`, `anthropic-design`. Only liquid-glass uses backdrop work; the
others are flat and cost nothing at render time. Icon packs are independent of
the interface theme (each theme only provides a default). Per-app overrides
(theme, scheme, icon pack) are served per app id. See
[liquid-glass.md](liquid-glass.md).

## Security model

- No root daemons except where unavoidable. Everything except `kiln-dm` runs as
  the user. `kiln-dm` is the only component that will need root (PAM and
  starting a session as another user); today its PAM backend is a stub that
  refuses logins, so the shipped daemon holds only unprivileged logic.
- Control sockets: private runtime directory (0700), socket 0600, so only the
  owning user connects. Passcodes cross the socket in `kiln-lock`; that is
  acceptable only because of those permissions. Do not expose these sockets
  through any proxy.
- systemd user units (packaging/systemd/user) apply `NoNewPrivileges`,
  `ProtectSystem=strict`, `ProtectHome=read-only`, `PrivateTmp`, empty
  capability set, `RestrictAddressFamilies=AF_UNIX` (network only for store and
  dlp; `IPAddressDeny=any` for noxal-ai), `MemoryDenyWriteExecute` and a syscall
  filter for daemons that do not spawn programs, and `MemoryMax` per daemon.
  User-manager sandboxing depends on unprivileged user namespaces; not every
  option was exercised in the VM (see BUDGETS.md / report).
- Process launching is shell-free: `.desktop` Exec is tokenised per the spec and
  field codes expanded per argument (flow-apps); package names are validated and
  `--` guards are used (flow-store); yt-dlp gets an allowlist of options and the
  URL after `--`, http(s) only, optionally inside bubblewrap (flow-dlp);
  llama.cpp CLI arguments are substituted per argument (noxal-ai).
- Passcodes: argon2id (19 MiB, t=2, p=1), per-hash random salt, file mode 0600,
  lockout schedule after 5 failures, current passcode required to change it.
  Locking is enforced by the compositor; kiln-lock is the authority it asks.
- NoxalAi tools are an allowlist (open_app, set_setting, notify, timer,
  calendar_query), arguments validated, setting keys limited (no lock, power or
  security keys), each tool needs a permission (ask / allow / deny). Conversations
  are local; optional encryption at rest (argon2id + ChaCha20-Poly1305); no
  telemetry, no network.
- Untrusted input is bounded: IPC lines 1 MiB, ICS 4 MiB, images 64 MiB and
  16384 px, notification text 4096 chars, history 200 items.

## Crossover with Glaze

- Colour role names in `flow-theme` match `glaze-design` so a shared design crate
  can replace the theme tables; the `anthropic-design` theme carries Glaze Clay's
  palette (copied with credit, docs/CREDITS.md).
- KilnDesktop: `kiln-desktop-mode` watches DRM connectors, applies the policy
  (auto / ask / never), starts the configured Glaze session command on the
  external output and emits `desktop.input_redirect` for the compositor. The
  phone shell keeps running. `kiln-dm` lists both sessions and records handover
  requests.
- Shared services: Glaze can use the same `flow-*` sockets (settings, theme,
  notifs, media). The Kiln Store app is intended to become a client of
  `flow-store` (same data model and commands).

## Process supervision

Preferred on systemd: one user unit per daemon, grouped by `flowde.target`.
`flowd` exists for systems without that (OpenRC on postmarketOS): dependency
order, exponential restart backoff (0.5 s doubling to 30 s, reset after a 60 s
stable run), crash-loop breaker (more than 5 exits in 60 s pauses 5 minutes),
staggered boot start. Socket activation is a TODO(feature).
