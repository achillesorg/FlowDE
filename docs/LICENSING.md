# Licensing notes

Repository: MIT (see LICENSE). All crates declare `license = "MIT"`.

Dependency policy (`deny.toml`, enforced in CI only if `cargo-deny` is
available): allow MIT, Apache-2.0 (with or without LLVM exception), BSD-2/3,
ISC, Unicode-3.0/Unicode-DFS-2016, Zlib, MPL-2.0 (file-level copyleft, none
modified here), CC0-1.0. No GPL/AGPL dependencies. This was **not** audited with
`cargo-deny` here (tool not installed); the list above is the intended policy.

Runtime programs invoked, not linked: `yt-dlp` (Unlicense), `bwrap` (LGPL-2+),
`flatpak` (LGPL-2.1+), `apt`/`dpkg` (GPL), `llama.cpp` (MIT), `systemd-run`
(LGPL-2.1+). Invoking them as separate processes does not affect FlowDE's licence.

Icon packs: FlowDE never bundles third-party packs. Papirus is GPL-3.0; Adwaita
is LGPL-3.0/CC-BY-SA-3.0; users install them from their distribution.
