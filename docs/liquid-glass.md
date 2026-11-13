# Liquid glass and interface themes

FlowDE's default look is a glass material: translucent surfaces that blur and
bend what is behind them, lit by a thin specular rim. **Requirement for the
future shell:** it must look like a real material, not a flat blur with a
white border. This is an original design and implementation; no Apple code,
assets or measured constants are used. The backend (this repo) provides the
resolved parameters; the compositor/shell renders them.

## What the material is made of

| Layer | Token(s) | Notes |
|---|---|---|
| Backdrop blur | `glass.blur_radius` (28 px regular, 12 px clear) | blur the backdrop at 1/2 to 1/4 resolution (dual Kawase) and upsample; cache per frame; damage-region only |
| Saturation lift | `glass.saturation` (1.6 / 1.3) | applied to the blurred backdrop |
| Lensing / refraction | `glass.refraction`, `glass.edge_displacement`, `glass.bezel` | the backdrop sample is displaced along the inward SDF normal inside the bezel, strongest at the edge (quadratic falloff), with a few percent chromatic dispersion |
| Tint | `glass.tint` (rgba) | adapts to content and ambient light (below) |
| Specular rim | `glass.specular`, `glass.rim_angle`, `glass.rim_width` | band hugging the edge, brightest where the normal faces the light (upper left), faint counter-rim opposite |
| Motion highlight | `glass.highlight_x/y`, `glass.motion` | tilt from the accelerometer shifts the light direction and highlight by up to 6 px; off with reduce motion or below Full |
| Hairline border | `glass.border` | keeps definition on bright backdrops |
| Shadow | `glass.shadow` | |
| Concentric corners | `radius.window/card/control/chip` | inner = outer - inset (never negative); window 32, card 22, control 14, chip 16 (capsule = height / 2) |

Variants: **regular** (frosted, legible anywhere) and **clear** (tint alpha about
0.1, stronger lensing, for media-rich backdrops; relies on the lens and rim for
definition). Schemes: light and dark. **Tinted**: pull the tint 35 % (25 % clear)
towards the accent colour.

## Adaptation (flow-theme, `resolve.rs`)

- **Ambient light** (lux from flow-sensors, log scaled to 0..1 over 0..~30 000
  lux): in bright light the tint alpha rises (up to +0.2 above half scale),
  specular falls by up to half, and the contrast target becomes 7:1; in a dark
  room specular is reduced 20 %.
- **Content** (`theme.set_content_luma`, supplied by the shell for what is
  behind a surface): the tint alpha is raised in 0.025 steps (cap 0.95) until
  the label/backdrop WCAG contrast reaches 4.5:1 (7:1 in bright light or
  increase-contrast). The predicted ratio is reported as `contrast.ratio`.
- **Accessibility**: reduce transparency forces Flat (opaque fill, no backdrop
  sampling); increase contrast caps at BlurOnly, tint >= 0.85, no specular or
  lens, 0.6 border; reduce motion removes the tilt highlight.
- **Motion**: `rim_angle = 315 deg - 40 deg * tilt_x`, highlight offset
  `(-tilt_x, tilt_y) * 12 px/g`, tilt clamped to +/-0.5 g.

## Power-aware quality ladder

Full lensing -> blur only -> flat. The target is the minimum of:

| Condition | Cap |
|---|---|
| reduce transparency | Flat |
| increase contrast | BlurOnly |
| GPU none / low (from DRM driver name) | Flat / BlurOnly |
| thermal hot or critical / warm | Flat / BlurOnly |
| low-power mode | BlurOnly |
| battery <= 10 % / <= 20 % (not charging) | Flat / BlurOnly |
| `theme.set_max_quality` (user/shell ceiling) | as set |

The governor steps down at once and steps up one level only after the better
target has been stable for 10 s. BlurOnly uses 60 % of the blur radius and a
static rim; Flat is a solid fill with no backdrop sampling and gives the
longest battery life.

## Other interface themes

| Theme | Character | Cost |
|---|---|---|
| `liquid-glass` (default) | above | blur + lens at Full |
| `minimalist` | white/black neutrals, one accent (text on accent picks black or white by contrast), 8 px radius, no shadow | flat |
| `material-you` | dynamic colour from a seed (wallpaper dominant colour via kiln-wall, or the accent), 16 px radius | flat |
| `anthropic-design` | Glaze Clay: cream `#F3EFE6`/ink `#2B2926`, terracotta `#D97757`, 1 px hairlines, 5 px controls, zero-blur offset shadow | flat |

All four emit the same colour roles (`canvas`, `surface`, `surface_raised`,
`surface_sunken`, `border`, `text_primary`, `text_secondary`, `accent`,
`accent_text`, `focus`, `danger`, `success`, `warning`, `shadow`, `highlight`,
`panel`, `ink`, `accent_soft`, `accent_soft_text`) so apps are written once.
App themes use the same machinery: `theme.set_app_override` sets per-app theme,
scheme and icon pack, and `flow theme tokens APP` resolves with it.

### Dynamic colour (MaterialYou)

Seed -> hue and chroma in LCh (CIE L*a*b*, D65). Tonal palettes at tones
0,10,...,100 (tone = L*; chroma reduced to stay inside sRGB). Palettes: primary
(chroma >= 48), secondary (16), tertiary (hue + 60, 24), neutral (4), neutral
variant (8), error (hue 25, 84). Light and dark schemes pick tones (for
example primary 40/80, on-primary 100/20, container 90/30). Medium and high
contrast move accent tones 10 or 20 steps toward the extremes. Tests assert
primary/on-primary, surface/on-surface, container pairs and error pairs reach
4.5:1 (standard) and 7:1 (high) for several seeds. This is a perceptual
approximation of HCT (CAM16 would be the upgrade); it will not match Google's
output exactly, by design.

## Icon packs

Independent of the interface theme. Discovery follows the freedesktop Icon Theme
Specification over `~/.icons`, `$XDG_DATA_HOME/icons`, `.../flowde/icon-packs`,
and each `$XDG_DATA_DIRS/icons` and `.../flowde/icon-packs`, with `Inherits`
fallback ending in hicolor. A pack declares optional `X-Flow-Style`
(glass/line/filled/ink), `X-Flow-License`, `X-Flow-Recolor=true`. Selection:
`appearance.icon_pack` (`auto` = the theme's default: flow-glass, flow-line,
flow-filled, glaze-ink), per-app override, fallback Adwaita then hicolor.
Candidates (none bundled or downloaded): an original Flow glass pack (future),
the Glaze ink-outline set at `glaze/assets/icons/ink` (the author's own, 3 weights,
could ship as `glaze-ink` with credit), Adwaita (LGPL-3.0/CC-BY-SA-3.0, fallback),
Papirus (GPL-3.0, optional, user installs it).

## Reference shader

`docs/liquid-glass/lens.wgsl` and `lens.glsl`: rounded-rect SDF, edge-normal
refraction with dispersion, saturation, tint, specular rim with counter-rim and
motion offset, hairline border, premultiplied output. **Not compiled or run
here** (no naga/glslang available; no GPU in the VM). Treat as a sketch.

## Shell requirements (for later)

- One backdrop blur per frame shared by all glass surfaces behind the same
  content; downsample 1/2 to 1/4; skip entirely at BlurOnly/Flat as appropriate
  (Flat never samples the backdrop).
- Re-render only damaged regions; stop the tilt highlight when the screen is
  static or the quality is below Full.
- Read tokens from `flow theme tokens` / `org.flowde.Theme1.GetTokens` and
  re-read on `theme.changed` (carries the icon pack too).
- Feed `theme.set_content_luma` (cheap luminance estimate of the region behind
  big surfaces such as the dock and sheets).
- GTK: `flow theme css gtk` gives `@define-color` plus classes; GTK has no
  backdrop-filter, so blur and lens are the compositor's job. Qt/QML and web
  views: flat tokens or `flow theme css vars`.

## What is not verified

No GPU rendering was run, so the numbers (blur radii, displacement, rim) are
design starting points, not tuned by eye. Contrast and quality-ladder logic is
unit tested; visual quality is not.
