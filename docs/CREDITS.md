# Credits

- **FeDe** (same author): the `anthropic-design` colour palette values and the
  role names (`canvas`, `surface`, `accent`, ...) in `crates/flow-theme/src/themes.rs`
  come from `fede-design` (`crates/fede-design/src/theme.rs`, "Fede Clay"). The
  `Exec`/ipc patterns were read for ideas; no other FeDe code was copied.
- **Kiln Store** (same author, `kiln-apps/kiln-store`): the apt/flatpak command
  lines and the package model in `flow-store` mirror `kiln_store/backends.py`.
- **Algorithms**: civil-date conversion (`flow-common::time`) follows Howard
  Hinnant's public-domain `days_from_civil`/`civil_from_days`; SDF rounded
  rectangle formulas in the shader sketches are standard (Inigo Quilez style
  public formulas, written independently).
- **Liquid glass** is an original implementation. No Apple code, assets or
  measured Apple constants are used; "Liquid Glass" is Apple's trademark and is
  used here only to describe the visual goal.
- **MaterialYou**: original maths inspired by the idea of tonal palettes from a
  seed colour; no Google/Material code or tables are included. "Material You"
  is Google's name for their system.
- **Dependencies** (all permissive): tokio, serde, serde_json, toml, tracing,
  tracing-subscriber, thiserror, zbus/zvariant, argon2/password-hash,
  chacha20poly1305, png. See `deny.toml` and `Cargo.lock`.
- Third-party icon packs mentioned (Adwaita, Papirus, hicolor) are NOT bundled.
  `yt-dlp` (Unlicense) and `llama.cpp` (MIT) are external programs the user
  installs; Qwen model licences vary by size and are the user's responsibility.
