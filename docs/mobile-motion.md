# FlowDE mobile motion tokens

Implemented in `shell/qml/Main.qml` (springs) and `shell/qml/AppPage.qml` (shared element). Desktop Glaze can reuse the numbers.

| Token | Value | Notes |
|---|---|---|
| App open/close | one progress `appP` 0..1, `SpringAnimation { spring: 4.2; damping: 0.38; epsilon: 0.001 }` | ~0.4 s response, damping ratio ~0.8. Starts on touch-up, never waits for the app. |
| Shared element | rect(icon) -> rect(screen); corner radius `r_icon * (1 - p)`; content fades in for p > 0.55; icon glyph fades out by p = 0.33 | Home scales to 0.94 and dims 28 % behind the app. |
| Close | bottom-edge drag sets `appP = 1 - dy / (0.6 * height)` (min 0.15); release: dy > 70 px or vy < -900 px/s -> home, else spring back to 1 | Finger-following, interruptible. |
| Lock slide | spring 3.2 / damping 0.32 | |
| Shade | spring 3.4 / damping 0.3 | pull from top 44 px strip, snap at 0.3 |
| Press scale | 0.92, spring 4 / damping 0.35 | |
| Reduce motion | every `Behavior.enabled = false` (instant), wallpaper animation off | |

Frame times (QEMU VM, llvmpipe, other agents building): home/lock about 17 ms (vsync-capped 60 Hz), shade with full glass about 50 ms (software GL bound, blur-only about 50 ms, flat about 17 ms). Real-GPU numbers not measured.
