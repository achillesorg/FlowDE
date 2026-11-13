#version 440
// FlowDE Liquid-Glass material (original implementation, no Apple code/assets).
// backdrop blur + edge lensing (refraction along the rounded-rect SDF normal)
// + specular rim + adaptive tint. quality: 0 = full, 1 = blur only.
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 size;          // panel size px
    vec2 origin;        // panel origin in backdrop px
    vec2 backdropSize;  // backdrop size px
    float radius;
    float blur;         // blur radius px
    float refraction;   // lens displacement px at the very edge
    float bezel;        // lens band width px
    float rim;          // specular strength 0..1
    float tintAmount;   // adaptive tint strength
    float quality;      // 0 full, 1 blur only
    float dark;         // 0 light, 1 dark
    vec4 tint;
    vec4 rimColor;
};
layout(binding = 1) uniform sampler2D src;

float sdRound(vec2 p, vec2 half_, float r, out vec2 n) {
    vec2 q = abs(p) - (half_ - r);
    vec2 s = vec2(p.x < 0.0 ? -1.0 : 1.0, p.y < 0.0 ? -1.0 : 1.0);
    if (q.x > 0.0 && q.y > 0.0) n = normalize(q) * s;
    else n = (q.x > q.y) ? vec2(s.x, 0.0) : vec2(0.0, s.y);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
vec3 fetch(vec2 px) { return texture(src, clamp(px / backdropSize, vec2(0.002), vec2(0.998))).rgb; }

void main() {
    vec2 p = qt_TexCoord0 * size;
    vec2 n;
    float d = sdRound(p - size * 0.5, size * 0.5, radius, n);
    float cover = smoothstep(0.7, -0.7, d);
    if (cover <= 0.0) { fragColor = vec4(0.0); return; }

    vec2 base = origin + p;
    float depth = clamp(-d / max(bezel, 1.0), 0.0, 1.0);
    float lens = (1.0 - depth);
    lens = lens * lens * lens;
    vec2 disp = (quality < 0.5) ? -n * lens * refraction : vec2(0.0);

    // 13-tap golden-angle blur on the (pre-downsampled) backdrop
    vec3 acc = fetch(base + disp) * 1.0;
    float wsum = 1.0;
    for (int i = 1; i < 13; i++) {
        float fi = float(i);
        float a = fi * 2.399963;
        float r = blur * sqrt(fi / 12.0);
        float w = 1.0 - 0.5 * (fi / 12.0);
        acc += fetch(base + disp + vec2(cos(a), sin(a)) * r) * w;
        wsum += w;
    }
    vec3 c = acc / wsum;

    // vibrancy + adaptive tint (stronger where backdrop is bright in light mode / dark in dark mode)
    float lum = dot(c, vec3(0.299, 0.587, 0.114));
    c = mix(vec3(lum), c, 1.25);
    float adapt = mix(lum, 1.0 - lum, dark);
    c = mix(c, tint.rgb, clamp(tint.a + tintAmount * adapt, 0.0, 0.95));

    if (quality < 0.5) {
        // specular rim: key light from top-left, soft fill bottom-right
        vec2 L = normalize(vec2(-0.55, -0.83));
        float edge = exp(-max(-d, 0.0) / 1.6);
        float key = pow(max(dot(n, L), 0.0), 1.6);
        float fill = 0.4 * pow(max(dot(n, -L), 0.0), 1.6);
        c += rimColor.rgb * rim * edge * (key + fill);
        // inner refractive glow along the bezel
        c += rimColor.rgb * rim * 0.10 * lens * (0.4 + key);
    }
    // hairline border for all glass levels
    float hair = smoothstep(1.2, 0.0, abs(d + 0.6));
    c = mix(c, rimColor.rgb, hair * 0.28);

    fragColor = vec4(c, 1.0) * cover * qt_Opacity;
}
