# What the mobile compositor needs from the FeDe repo

`compositor/` does NOT depend on `fede-compositor` / `fede-wm` by path (yet). Reasons, after reading
`crates/fede-compositor` (8.8k lines) and `docs/compositor.md`:

* `fede-compositor::State` is built around the desktop window manager: `WindowEntry` (SSD frames,
  title bars, animations, foreign-toplevel handles), `wm::WindowManager` (workspaces, snapping,
  tiling, Alt+Tab). A phone needs "one fullscreen app + shell" and touch, none of which can be
  switched on without editing that code. `fede-wm` (snap geometry, tiling, keybindings) is
  desktop policy and unused by a single-app compositor.
* Input is pointer/keyboard only (`input.rs` has no `InputEvent::Touch*`, no wl_touch seat
  capability, no edge-gesture recogniser).
* The fede crate is Linux-gated at the Smithay level; fine, but its `State` is not generic.

Meanwhile `compositor/` is a lean 700-line Smithay 0.7 compositor whose protocol handlers follow the
same anvil-derived layout as fede (credited in `compositor/src/main.rs`). The edge-gesture
recogniser and JSON protocol live in a portable lib (`compositor/src/{gesture,protocol}.rs`, unit tested).

## Changes that would let FeDe's compositor be reused (not done: FeDe repo is read-only for me)

1. **Seat touch capability + touch events**: `seat.add_touch()` and handling of
   `InputEvent::{TouchDown,TouchMotion,TouchUp,TouchCancel,TouchFrame}` in `input.rs`, routed through
   a pluggable "gesture filter" hook (`trait TouchFilter { fn down/motion/up -> Verdict }`) before
   delivery to clients. Mobile's `flowde_compositor::gesture::Recognizer` implements it.
2. **A `Policy` trait around `WindowManager`** with a `SingleAppFullscreen` implementation: new
   toplevel -> fullscreen at output logical size; only the active window mapped; `show_shell()`.
   Today `apply_layout()` hard-wires the desktop policy.
3. **Output rotation**: `Output::change_current_state(.., Some(Transform), ..)` plus re-`apply_layout()`
   and input coordinate transform (`Transform::invert().transform_point_in`), triggered from the control
   socket (`rotate`) and later from an IIO accelerometer (flow-sensors).
4. **Frame decoration off switch** (`ssd=false` always, `Insets::NONE`) as a compositor option.
5. **Control socket**: new messages `touch`, `swipe`, `rotate`, `state` (see `compositor/src/protocol.rs`)
   and server-push events (`gesture`, `rotation`) so shells need not own a second socket.
6. **ext-session-lock** is already in fede (`handlers/session_lock.rs`); mobile would reuse it as is for
   kiln-lock. Not wired in `compositor/` yet.
7. **Layer-shell overlay layer for the shade** and keyboard-interactivity handling (Qt shell via
   LayerShellQt). `compositor/` accepts wlr-layer-shell surfaces (Space renders them) but the prototype
   shell is a plain xdg fullscreen toplevel (app_id `flowde-shell`).

If FeDe takes 1-5, `compositor/` collapses into a ~150-line crate (policy + recogniser + main) and the
duplicated handler code disappears.
