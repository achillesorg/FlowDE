#version 440
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float aspect;
    float phase;
    vec4 c0;
    vec4 c1;
    vec4 c2;
    vec4 c3;
};
float blob(vec2 p, vec2 c, float r) { return exp(-dot(p - c, p - c) / (r * r)); }
void main() {
    vec2 uv = qt_TexCoord0;
    vec2 p = vec2(uv.x * aspect, uv.y);
    vec3 col = mix(c0.rgb, c1.rgb, smoothstep(0.0, 1.0, uv.y));
    col = mix(col, c2.rgb, blob(p, vec2(0.25 * aspect + 0.05 * sin(phase), 0.28), 0.34));
    col = mix(col, c3.rgb, blob(p, vec2(0.85 * aspect, 0.62 + 0.04 * cos(phase)), 0.40));
    col = mix(col, c2.rgb, 0.7 * blob(p, vec2(0.35 * aspect, 0.92), 0.30));
    // soft ribbons so edge lensing has structure to bend
    float rib = sin((p.x * 5.0 + p.y * 9.0) + phase) * 0.5 + 0.5;
    col += (c3.rgb - c0.rgb) * 0.10 * smoothstep(0.82, 1.0, rib);
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
