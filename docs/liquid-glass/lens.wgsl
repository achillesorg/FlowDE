// FlowDE liquid-glass lens + specular pass: ORIGINAL reference sketch (MIT).
// Not tuned, not verified on hardware. Inputs are the tokens served by
// flow-theme (glass.* / quality). Run this pass only when quality == "full";
// for "blur_only" skip the lens (refraction = 0) and keep tint + static rim;
// for "flat" draw a solid rounded rectangle and skip the backdrop sample.
//
// Pipeline: 1) compositor renders the backdrop to a texture and blurs it
// (dual Kawase or separable Gaussian at blur_radius, downsampled 1/2..1/4);
// 2) this fragment shader runs per glass surface with the blurred texture.

struct Glass {
  size_px: vec2<f32>,          // surface size in px
  corner_px: f32,              // outer corner radius (concentric radii: radius.window etc.)
  blur_radius_px: f32,         // already applied to the backdrop texture; used for sampling margin
  refraction: f32,             // 0..1   glass.refraction
  edge_disp_px: f32,           // glass.edge_displacement
  bezel_px: f32,               // glass.bezel
  specular: f32,               // 0..1   glass.specular
  rim_angle: f32,              // radians, clockwise from up (glass.rim_angle)
  rim_width_px: f32,           // glass.rim_width
  highlight_off: vec2<f32>,    // px, motion highlight (glass.highlight_x/y)
  saturation: f32,             // glass.saturation
  tint: vec4<f32>,             // glass.tint (rgba, straight alpha)
  border: vec4<f32>,           // glass.border
  backdrop_origin_px: vec2<f32>, // top-left of this surface inside the backdrop texture
  backdrop_size_px: vec2<f32>,
};

@group(0) @binding(0) var<uniform> g: Glass;
@group(0) @binding(1) var backdrop: texture_2d<f32>;   // blurred backdrop
@group(0) @binding(2) var samp: sampler;

// Signed distance to a rounded rectangle centred at the origin.
fn sd_round_rect(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
  let q = abs(p) - half + vec2<f32>(r);
  return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// Outward unit normal of the SDF (central differences; cheap and robust).
fn sd_normal(p: vec2<f32>, half: vec2<f32>, r: f32) -> vec2<f32> {
  let e = 0.75;
  let nx = sd_round_rect(p + vec2<f32>(e, 0.0), half, r) - sd_round_rect(p - vec2<f32>(e, 0.0), half, r);
  let ny = sd_round_rect(p + vec2<f32>(0.0, e), half, r) - sd_round_rect(p - vec2<f32>(0.0, e), half, r);
  let n = vec2<f32>(nx, ny);
  let l = length(n);
  return select(vec2<f32>(0.0, -1.0), n / l, l > 1e-5);
}

fn luma(c: vec3<f32>) -> f32 { return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)); }

// Light direction in surface space (y down): rim_angle clockwise from up.
fn light_dir(angle: f32) -> vec2<f32> { return vec2<f32>(sin(angle), -cos(angle)); }

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>, @location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
  let half = g.size_px * 0.5;
  let p = (uv - vec2<f32>(0.5)) * g.size_px;          // px from centre
  let d = sd_round_rect(p, half, g.corner_px);          // < 0 inside
  // Anti-aliased coverage of the shape (1 px edge).
  let cover = clamp(0.5 - d, 0.0, 1.0);
  if (cover <= 0.0) { return vec4<f32>(0.0); }

  let n = sd_normal(p, half, g.corner_px);              // outward normal
  // Depth into the bezel: 0 at the edge, 1 at bezel_px inside.
  let t = clamp(-d / max(g.bezel_px, 1.0), 0.0, 1.0);

  // --- Lens: displace the backdrop sample along the inward normal, strongest at the
  // rim and falling off quadratically, like the curved edge of a glass slab.
  let profile = (1.0 - t) * (1.0 - t);
  let disp = -n * (g.edge_disp_px * g.refraction * profile);

  // Backdrop texel position for this fragment.
  let base = g.backdrop_origin_px + uv * g.size_px;
  // Slight dispersion: R, G, B sample at marginally different strengths (<= 6% of disp).
  let k = 0.06;
  let pr = (base + disp * (1.0 + k)) / g.backdrop_size_px;
  let pg = (base + disp) / g.backdrop_size_px;
  let pb = (base + disp * (1.0 - k)) / g.backdrop_size_px;
  var col = vec3<f32>(
    textureSampleLevel(backdrop, samp, pr, 0.0).r,
    textureSampleLevel(backdrop, samp, pg, 0.0).g,
    textureSampleLevel(backdrop, samp, pb, 0.0).b);

  // --- Saturation lift of the blurred backdrop (frosted look), then tint.
  let l = luma(col);
  col = mix(vec3<f32>(l), col, g.saturation);
  col = mix(col, g.tint.rgb, g.tint.a);

  // --- Specular rim: a thin band hugging the edge, brightest where the normal faces
  // the light, with a faint counter-rim on the opposite side (internal reflection).
  let L = light_dir(g.rim_angle);
  let Lm = normalize(L + g.highlight_off / 40.0);        // motion nudges the light
  let facing = dot(n, Lm);                               // -1..1
  let band = 1.0 - smoothstep(0.0, g.rim_width_px * 2.0, -d);   // 1 at the edge, 0 inside
  let main_rim = pow(max(facing, 0.0), 3.0);
  let back_rim = 0.35 * pow(max(-facing, 0.0), 3.0);
  let spec = g.specular * band * (main_rim + back_rim);
  col = col + vec3<f32>(spec);

  // --- Hairline border (kept for definition on bright backdrops).
  let edge = 1.0 - smoothstep(0.0, 1.0, -d);
  col = mix(col, g.border.rgb, g.border.a * edge);

  // Premultiplied output.
  return vec4<f32>(col * cover, cover);
}
