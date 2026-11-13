# FlowDE feature inbox

Interface theme: Liquid Glass, Minimalist, MaterialYou, Anthropic Design
Icon themes?
Lightweight-Medweight
App themes: Liquid Glass, Materialyou, Anthropic Deisgn
Custom Made apps
Camera
Calendar
Clock
Media Player
FlowDLP
FlowStore
KilnDesktop (like Dex)
NoxalAi (Assistant)
Plan more
Services:
KilnLock
KilnNotifs
KilnDM
KilnWall
NoxalAi (based on Qwen)


## Lead's mapping

Appended by the backend agent; the lines above are the user's and are untouched.

| Inbox line | Where it lives |
|---|---|
| Interface theme: Liquid Glass, Minimalist, MaterialYou, Anthropic Design | `flow-theme` (`themes.rs`, `material_you.rs`, `resolve.rs`), setting `appearance.theme`, docs/liquid-glass.md |
| Icon themes? (answer: icon packs) | `flow-theme::icons`, setting `appearance.icon_pack`, per-app override |
| Lightweight-Medweight | power-aware quality ladder, daemon budgets (docs/ARCHITECTURE.md, docs/BUDGETS.md) |
| App themes: Liquid Glass, Materialyou, Anthropic Design | per-app overrides in `flow-theme` (`theme.set_app_override`) |
| Camera | `flow-camera` (devices, permission prompts; capture TODO) |
| Calendar | `flow-calendar` |
| Clock | `flow-clock` |
| Media Player | `flow-media` (MPRIS2 aggregator, media keys) |
| FlowDLP | `flow-dlp` (yt-dlp style download service; drives the yt-dlp binary) |
| FlowStore | `flow-store` (apt and Flatpak jobs, shares design with Kiln Store) |
| KilnDesktop (like Dex) | `kiln-desktop-mode` |
| NoxalAi (Assistant), NoxalAi (based on Qwen) | `noxal-ai` |
| KilnLock | `kiln-lock` |
| KilnNotifs | `kiln-notifs` |
| KilnDM | `kiln-dm` (display manager backend, confirmed meaning) |
| KilnWall | `kiln-wall` |
| Plan more | not planned yet; see report "Questions" |
