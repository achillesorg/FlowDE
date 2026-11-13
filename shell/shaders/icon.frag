#version 440
// FlowDE glass ICON material (original). A small liquid-glass lens tile:
//  * refraction: backdrop sampled along the squircle-SDF normal inside the bezel (strong at the edge)
//    plus a slight overall magnification (zoom < 1)
//  * chromatic dispersion: R/G/B taps at slightly different displacements
//  * specular rim top-left (key light), weaker bottom-right reflection, thin darker inner edge
//  * adaptive tint: tint strength follows backdrop luminance
// quality 0 = full lens, 1 = blur only (no lens, no dispersion).
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 size;
    vec2 origin;
    vec2 backdropSize;
    float radius;
    float bezel;
    float refraction;
    float dispersion;
    float zoom;
    float quality;
    float dark;
    float rim;
    vec4 tint;
    vec4 rimColor;
};
layout(binding = 1) uniform sampler2D src;

float sdRound(vec2 p, vec2 h, float r, out vec2 n) {
    vec2 q = abs(p) - (h - r);
    vec2 s = vec2(p.x < 0.0 ? -1.0 : 1.0, p.y < 0.0 ? -1.0 : 1.0);
    if (q.x > 0.0 && q.y > 0.0) n = normalize(q) * s;
    else n = (q.x > q.y) ? vec2(s.x, 0.0) : vec2(0.0, s.y);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
vec3 fetch(vec2 px) { return texture(src, clamp(px / backdropSize, vec2(0.002), vec2(0.998))).rgb; }

void main() {
    vec2 p = qt_TexCoord0 * size;
    vec2 c = size * 0.5;
    vec2 n;
    float d = sdRound(p - c, c, radius, n);
    float cover = smoothstep(0.8, -0.8, d);
    if (cover <= 0.0) { fragColor = vec4(0.0); return; }

    float depth = clamp(-d / max(bezel, 1.0), 0.0, 1.0);
    float lens = pow(1.0 - depth, 2.2);
    vec2 base = origin + c + (p - c) * ((quality < 0.5) ? zoom : 1.0);
    vec3 col;
    if (quality < 0.5) {
        vec2 off = -n * lens * refraction;
        // 3-tap dispersion, each with a little 4-tap frost
        vec3 acc = vec3(0.0);
        for (int k = 0; k < 4; k++) {
            vec2 j = vec2(cos(1.57 * float(k) + 0.7), sin(1.57 * float(k) + 0.7)) * 1.1;
            acc.r += fetch(base + off * (1.0 + dispersion) + j).r;
            acc.g += fetch(base + off + j).g;
            acc.b += fetch(base + off * (1.0 - dispersion) + j).b;
        }
        col = acc / 4.0;
    } else {
        vec3 acc = fetch(base);
        float w = 1.0;
        for (int k = 1; k < 9; k++) {
            float a = float(k) * 2.399963;
            float r = 5.0 * sqrt(float(k) / 8.0);
            acc += fetch(base + vec2(cos(a), sin(a)) * r);
            w += 1.0;
        }
        col = acc / w;
    }

    float lum = dot(col, vec3(0.299, 0.587, 0.114));
    float adapt = mix(lum, 1.0 - lum, dark);
    // Qt passes colors premultiplied: un-premultiply the tint
    col = mix(col, tint.rgb / max(tint.a, 0.001), clamp(tint.a + 0.22 * adapt, 0.0, 0.8));
    // soft vertical sheen (lighter top, deeper bottom)
    col += (0.07 * (1.0 - qt_TexCoord0.y) - 0.02 * qt_TexCoord0.y) * (dark > 0.5 ? 0.6 : 1.0);

    vec2 L = normalize(vec2(-0.55, -0.83));
    float kd = max(dot(n, L), 0.0);
    float kb = max(dot(n, -L), 0.0);
    float inside = max(-d, 0.0);
    if (quality < 0.5) {
        // caustic: light gathers on the shadow side inside the bezel
        col += rimColor.rgb * 0.10 * lens * pow(kb, 1.5);
    }
    // thin darker inner edge
    float ring = exp(-pow((inside - 2.6) / 1.1, 2.0));
    col *= 1.0 - ring * (0.08 + 0.14 * (1.0 - kd));
    // specular rim (top-left) + faint opposite reflection
    float edge = exp(-inside / 1.3);
    col += rimColor.rgb * rim * edge * (pow(kd, 1.4) * 1.0 + 0.30 * pow(kb, 1.6));
    // broader soft highlight just inside the top-left edge
    col += rimColor.rgb * rim * 0.16 * exp(-inside / 7.0) * pow(kd, 2.0);

    fragColor = vec4(clamp(col, 0.0, 1.0), 1.0) * cover * qt_Opacity;
}
