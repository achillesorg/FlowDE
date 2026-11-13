pragma Singleton
import QtQuick

// Design tokens. Four sets (LiquidGlass default, Minimalist, MaterialYou, AnthropicDesign),
// each with light/dark. Switchable at runtime: Theme.name = "Minimalist"; Theme.dark = true.
QtObject {
    id: t
    property string name: "LiquidGlass"
    property bool dark: false
    property bool reduceTransparency: false
    property bool reduceMotion: false
    property string glass: "full"          // power ladder: full | blur | flat
    property var backdrop: null       // shared downsampled wallpaper source (set by Main)
    property var backdropItem: null
    readonly property var names: ["LiquidGlass", "Minimalist", "MaterialYou", "AnthropicDesign"]

    readonly property var sets: ({
        LiquidGlass: {
            glass: true, radius: 28, tileRadius: 16, blur: 22, refraction: 20, bezel: 26, rim: 0.85,
            light: { text: "#12151c", dim: "#5c6272", accent: "#2f7bff", tint: "#ffffff", tintA: 0.14, rim: "#ffffff", flat: "#d8dfeaE6",
                     w0: "#bcd3ff", w1: "#ffd6e8", w2: "#6ea8ff", w3: "#ffb48a", icon: "#2b3040", card: "#ffffff" },
            dark:  { text: "#f4f6fb", dim: "#a2a9ba", accent: "#5ea0ff", tint: "#10131c", tintA: 0.22, rim: "#ffffff", flat: "#1c2130E6",
                     w0: "#0b1030", w1: "#2a1040", w2: "#2459c8", w3: "#c4457a", icon: "#e8ecf6", card: "#141824" } },
        Minimalist: {
            glass: false, radius: 14, tileRadius: 10, blur: 0, refraction: 0, bezel: 1, rim: 0,
            light: { text: "#111111", dim: "#777777", accent: "#111111", tint: "#ffffff", tintA: 0.0, rim: "#000000", flat: "#f6f6f6F2",
                     w0: "#fafafa", w1: "#ececec", w2: "#f2f2f2", w3: "#e4e4e4", icon: "#111111", card: "#ffffff" },
            dark:  { text: "#f2f2f2", dim: "#8a8a8a", accent: "#f2f2f2", tint: "#000000", tintA: 0.0, rim: "#ffffff", flat: "#141414F2",
                     w0: "#050505", w1: "#101010", w2: "#0a0a0a", w3: "#1a1a1a", icon: "#f2f2f2", card: "#161616" } },
        MaterialYou: {
            glass: false, radius: 32, tileRadius: 26, blur: 0, refraction: 0, bezel: 1, rim: 0,
            light: { text: "#1d1b20", dim: "#49454f", accent: "#6750a4", tint: "#ffffff", tintA: 0.0, rim: "#ffffff", flat: "#ece6f0FF",
                     w0: "#f7f2fa", w1: "#e8def8", w2: "#d0bcff", w3: "#efb8c8", icon: "#4a4458", card: "#f3edf7" },
            dark:  { text: "#e6e0e9", dim: "#cac4d0", accent: "#d0bcff", tint: "#000000", tintA: 0.0, rim: "#ffffff", flat: "#2b2930FF",
                     w0: "#141218", w1: "#211f26", w2: "#381e72", w3: "#4a2532", icon: "#e8def8", card: "#211f26" } },
        AnthropicDesign: {
            glass: false, radius: 20, tileRadius: 14, blur: 0, refraction: 0, bezel: 1, rim: 0,
            light: { text: "#141413", dim: "#6b6a64", accent: "#d97757", tint: "#ffffff", tintA: 0.0, rim: "#ffffff", flat: "#faf9f5F5",
                     w0: "#f0eee6", w1: "#e8e6dc", w2: "#ebdbbc", w3: "#e4b8a4", icon: "#141413", card: "#faf9f5" },
            dark:  { text: "#faf9f5", dim: "#b0aea5", accent: "#d97757", tint: "#141413", tintA: 0.0, rim: "#ffffff", flat: "#262624F5",
                     w0: "#141413", w1: "#1f1e1d", w2: "#3a2a22", w3: "#5a2f22", icon: "#faf9f5", card: "#262624" } }
    })
    readonly property var cur: sets[name] || sets.LiquidGlass
    readonly property var c: dark ? cur.dark : cur.light

    readonly property color text: c.text
    readonly property color textDim: c.dim
    readonly property color accent: c.accent
    readonly property color iconInk: c.icon
    readonly property color card: c.card
    readonly property color flatSurface: c.flat
    readonly property color wall0: c.w0
    readonly property color wall1: c.w1
    readonly property color wall2: c.w2
    readonly property color wall3: c.w3
    readonly property real radius: cur.radius
    readonly property real tileRadius: cur.tileRadius

    // effective glass level: 0 full lensing, 1 blur only, 2 flat.
    // reduce-transparency forces flat; themes without glass are flat.
    readonly property int quality: (reduceTransparency || !cur.glass) ? 2
                                    : glass === "flat" ? 2 : glass === "blur" ? 1 : 0
    readonly property color tint: c.tint
    readonly property real tintA: c.tintA
    readonly property color rimColor: c.rim
    readonly property real blurPx: cur.blur
    readonly property real refractPx: cur.refraction
    readonly property real bezelPx: cur.bezel
    readonly property real rim: cur.rim
    readonly property int animMs: reduceMotion ? 0 : 280
    readonly property string font: name === "AnthropicDesign" ? "serif" : "Inter"
}
