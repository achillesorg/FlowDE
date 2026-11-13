# kiln-wall

KilnWall: per-output wallpaper config (light/dark, fit mode), rotation, dominant-colour seed from PNG/PPM pushed to flow-theme when `appearance.seed_from_wallpaper` is on.

**Status:** Config, decode (PNG, PPM), seed and push implemented. Painting is the compositor's job.

## TODO(feature)

- JPEG/WebP/AVIF decoding
- Per-output outputs list from the compositor
- Crossfade timing hints, dynamic (time-of-day) wallpapers
- Seed from a downscaled region, not the whole image
