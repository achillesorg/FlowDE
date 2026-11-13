# FlowDE mobile shell: decisions (stage 1 prototype)

| # | Decision | Why |
|---|---|---|
| 1 | Shell = Qt 6.8 / Qt Quick (QML), CMake, `shell/` | Lead's stack decision; Debian 13 ships Qt 6.8.2 with `qsb`. |
| 2 | One `GlassPanel` component; one fragment shader `shaders/glass.frag` (GLSL 440 -> qsb: GLSL 100es/120/150) | Every surface (lock cards, dock, shade, tiles, sliders, search, switcher cards) is a GlassPanel, so the quality ladder and themes apply everywhere. |
| 3 | Backdrop = ONE shared quarter-resolution `ShaderEffectSource` of the procedural wallpaper, sampled by every panel (13-tap golden-angle blur + SDF-normal edge lensing + specular rim + adaptive tint + vibrancy) | Cheap on software GL: no per-panel offscreen passes. Limitation: glass blurs the *wallpaper*, not windows/icons stacked between (panels over app content would need a second backdrop pass or compositor-side blur; see open questions). |
| 4 | Quality ladder `full` (lensing) -> `blur` -> `flat`; `FLOWDE_GLASS=full|blur|flat`, runtime `{"set":"glass",...}`; `reduce-transparency` forces flat; non-glass themes are flat | Power-aware: a power daemon (flow-power) later flips the switch; no code change in QML. |
| 5 | Themes are token sets in `Theme.qml` (singleton): LiquidGlass (default), Minimalist, MaterialYou, AnthropicDesign; each light/dark; switch at runtime | FEATURES-INBOX. Tokens only (colours, radii, glass parameters); no per-theme QML forks. MaterialYou palette is hand-picked tonal values, not dynamic wallpaper extraction. |
| 6 | Springs (`SpringAnimation`, interruptible, retargetable) for lock slide, shade, switcher, press feedback; reduce-motion sets `Behavior.enabled=false` / 0 ms | Interruptible by construction (drag disables the Behavior). |
| 7 | Icons: ink-outline fallback (outline tile + initial). No icon pack loaded | Icon theme choice open (inbox: "Icon themes?"). |
| 8 | Wallpaper: procedural shader (`wallpaper.frag`) from theme colours | No assets/licence issues; gives the lens structure to bend. |
| 9 | Fonts: Inter (present in the VM) for all themes except AnthropicDesign (generic serif) | Open question for the user. |
| 10 | Services are mocked (`Services` QObject): wifi/bt/airplane/rotation/dnd/brightness/volume/signal; battery read from `/sys/class/power_supply` else 78 %; notifications mock list | kiln-notifs / flow-* D-Bus bridges are the next step; the property names are the intended binding surface. `FLOWDE_MOCK=0` reserved for the real bridge. |
| 11 | Shell control socket = shell is a `QLocalServer` at `$FLOWDE_SHELL_SOCKET` (JSON lines); compositor connects as client and pushes gestures | Lets tests/tools drive the shell without the compositor too. Protocol in `compositor/src/protocol.rs`. |
| 12 | Compositor = own lean Smithay 0.7 crate, NOT depending on fede-compositor/fede-wm | See `mobile-needs-from-fede.md`. |
| 13 | Shell is an xdg fullscreen toplevel with app_id `flowde-shell` (base layer); apps are fullscreen toplevels, one visible; "home/switcher/shade" gestures bring the shell forward | LayerShellQt is not installed; shade-over-app needs the shell as a layer-shell overlay (TODO). |
| 14 | Gesture recogniser is portable and unit-tested (6 tests): edge band 28 px, bottom fast=Home slow=Switcher, top-left Shade, top-right QuickSettings, side inward=Back | Policy kept out of Smithay code. |
| 15 | Mouse in the nested window emulates one finger | Makes the nested prototype drivable; real touch arrives as `wl_touch` when the parent forwards it. |
| 16 | Build: shell lightweight build (4 C++ files) ran with `nice` without the heavy lock when the lock was held for >10 min by another agent's workspace build; Rust builds always take the lock | Deviation, reported. |
