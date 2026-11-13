# Budgets and measurements

Measured on the aarch64 Debian VM (Rust 1.100 stable, `cargo build --release
--workspace -j2`, profile: opt-level z, LTO, 1 codegen unit, panic=abort, strip).
The VM is a shared QEMU guest (load average about 5 to 8 during the runs), so
**timings are noisy; sizes and RSS are reliable**. Reproduce the size part with
`packaging/size-check.sh` (fails above 2560 KiB per daemon, 1024 KiB for `flow`).

## Binary sizes (release, stripped, KiB)

| Binary | KiB | | Binary | KiB |
|---|---:|---|---|---:|
| flow (CLI) | 516 | | flow-sensors | 1606 |
| flowd | 1221 | | flow-settings | 1734 |
| flow-apps | 1029 | | flow-store | 1029 |
| flow-calendar | 1029 | | flow-theme | 1798 |
| flow-camera | 965 | | kiln-desktop-mode | 1157 |
| flow-clock | 1029 | | kiln-dm | 1798 |
| flow-dlp | 1221 | | kiln-lock | 965 |
| flow-haptics | 1606 | | kiln-notifs | 1606 |
| flow-media | 1606 | | kiln-wall | 1093 |
| flow-net | 1670 | | noxal-ai | 1798 |
| flow-power | 1799 | | | |

Total of all 20 daemons plus CLI: about 28 MiB on disk. Daemons that use zbus
(about 1.6 to 1.8 MB) cost roughly 0.6 MB more than the socket-only ones
(about 1.0 MB). Building the whole workspace used about 330 MB in the VM
`target/` directory (deleted afterwards).

## Idle memory and CPU (measured)

Seventeen daemons ran side by side for about 25 s in a private session bus
(`dbus-run-session`), after the commands in the evidence run:

| Daemon | RSS kB (after 20 s idle) | CPU ticks (1 tick = 10 ms) since start |
|---|---:|---:|
| flow-settings | 3260 | 0 |
| kiln-notifs | 3132 | 2 |
| flow-power | 3256 | 3 |
| flow-sensors | 3172 | 5 |
| flow-haptics | 3092 | 2 |
| flow-net | 3096 | 2 |
| flow-apps | 2668 | 2 |
| flow-theme | 3716 | 3 |
| kiln-wall | 2444 | 1 |
| flow-clock | 2512 | 1 |
| flow-calendar | 2456 | 0 |
| flow-media | 3088 | 5 |
| kiln-desktop-mode | 2664 | 1 |
| flow-camera | 2436 | 1 |
| noxal-ai (mock backend) | 3280 | 3 |
| kiln-dm | 3220 | 0 |
| kiln-lock | 21904 (see below) | 67 |

- All but kiln-lock are within the goal of < 6 MB RSS and show no measurable
  idle CPU (0 to 5 ticks including start-up and the requests of the evidence
  run). This is a short sample, not a power measurement: no battery or
  wake-up/power-meter test was possible in the VM.
- **kiln-lock finding:** one argon2id hash (19 MiB) leaves 21.9 MB RSS because
  glibc raises its dynamic mmap threshold and keeps the freed buffer. With
  `MALLOC_MMAP_THRESHOLD_=131072` the same sequence ends at 2.4 MB RSS (peak
  during the hash stays about 21.6 MB). All packaged systemd units and `flowd`
  set `MALLOC_ARENA_MAX=1 MALLOC_MMAP_THRESHOLD_=131072 MALLOC_TRIM_THRESHOLD_=131072`.
  The 21.9 MB row above is from a manual start without those variables.
- CPU of kiln-lock (67 ticks) is the three argon2id operations of the test
  (about 0.2 s each in this VM).
- `flow-store`, `flow-dlp` and `flowd` were measured separately (not in the
  table): started with `flow-store`/`flow-dlp` they report RSS through
  `flow status`/`health`; numbers were of the same order (about 3 MB) but were
  not recorded in this table.

Not measured: noxal-ai with a real model (no model present, none downloaded),
flow-dlp with a real download, any daemon on real phone hardware, wake-ups per
second (no `perf`/powertop run), flow-theme GPU cost (no GPU).

## Goals vs measured

See "Idle CPU / RAM goals" in ARCHITECTURE.md: RSS goals met in this VM run;
the CPU goal is plausible from the tick counts but unproven without a longer
run and a power measurement on hardware.
