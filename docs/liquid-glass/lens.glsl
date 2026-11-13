// FlowDE liquid-glass lens + specular pass, GLSL ES 3.00 port of lens.wgsl.
// ORIGINAL reference sketch (MIT), untuned. See lens.wgsl for the commentary.
#version 300 es
precision highp float;

uniform vec2  u_size;          // px
uniform float u_corner;        // px
uniform float u_refraction;    // 0..1
uniform float u_edge_disp;     // px
uniform float u_bezel;         // px
uniform float u_specular;      // 0..1
uniform float u_rim_angle;     // radians, clockwise from up
uniform float u_rim_width;     // px
uniform vec2  u_highlight_off; // px
uniform float u_saturation;
uniform vec4  u_tint;
uniform vec4  u_border;
uniform vec2  u_backdrop_origin;
uniform vec2  u_backdrop_size;
uniform sampler2D u_backdrop;  // blurred backdrop

in vec2 v_uv;
out vec4 o_color;

float sdRoundRect(vec2 p, vec2 half_, float r) {
  vec2 q = abs(p) - half_ + vec2(r);
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

vec2 sdNormal(vec2 p, vec2 half_, float r) {
  const float e = 0.75;
  vec2 n = vec2(
    sdRoundRect(p + vec2(e, 0.0), half_, r) - sdRoundRect(p - vec2(e, 0.0), half_, r),
    sdRoundRect(p + vec2(0.0, e), half_, r) - sdRoundRect(p - vec2(0.0, e), half_, r));
  float l = length(n);
  return l > 1e-5 ? n / l : vec2(0.0, -1.0);
}

void main() {
  vec2 half_ = u_size * 0.5;
  vec2 p = (v_uv - 0.5) * u_size;
  float d = sdRoundRect(p, half_, u_corner);
  float cover = clamp(0.5 - d, 0.0, 1.0);
  if (cover <= 0.0) { o_color = vec4(0.0); return; }

  vec2 n = sdNormal(p, half_, u_corner);
  float t = clamp(-d / max(u_bezel, 1.0), 0.0, 1.0);
  float profile = (1.0 - t) * (1.0 - t);
  vec2 disp = -n * (u_edge_disp * u_refraction * profile);

  vec2 base = u_backdrop_origin + v_uv * u_size;
  const float k = 0.06;
  vec3 col = vec3(
    texture(u_backdrop, (base + disp * (1.0 + k)) / u_backdrop_size).r,
    texture(u_backdrop, (base + disp) / u_backdrop_size).g,
    texture(u_backdrop, (base + disp * (1.0 - k)) / u_backdrop_size).b);

  float l = dot(col, vec3(0.2126, 0.7152, 0.0722));
  col = mix(vec3(l), col, u_saturation);
  col = mix(col, u_tint.rgb, u_tint.a);

  vec2 L = vec2(sin(u_rim_angle), -cos(u_rim_angle));
  vec2 Lm = normalize(L + u_highlight_off / 40.0);
  float facing = dot(n, Lm);
  float band = 1.0 - smoothstep(0.0, u_rim_width * 2.0, -d);
  float spec = u_specular * band * (pow(max(facing, 0.0), 3.0) + 0.35 * pow(max(-facing, 0.0), 3.0));
  col += vec3(spec);

  float edge = 1.0 - smoothstep(0.0, 1.0, -d);
  col = mix(col, u_border.rgb, u_border.a * edge);
  o_color = vec4(col * cover, cover);
}
