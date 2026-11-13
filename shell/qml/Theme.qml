pragma Singleton
import QtQuick

// Design tokens. Default: "AnthropicGlassIcons" (alias anthropic-glass-icons): flat Kiln cut-paper wallpaper,
// flat paper surfaces with hairline strokes, serif clock, Liquid-Glass ICONS only.
// Others stay selectable: LiquidGlass (full-surface glass), Minimalist, MaterialYou, AnthropicDesign.
// Runtime: Theme.select("minimalist"); Theme.dark = true.
QtObject {
    id: t
    property string name: "AnthropicGlassIcons"
    property bool dark: false
    property bool reduceTransparency: false
    property bool reduceMotion: false
    property string glass: "full"          // power ladder: full | blur | flat
    property var backdrop: null            // shared downsampled wallpaper source (set by Main)
    property var backdropItem: null
    property int scrollTick: 0              // bumped while pages scroll so lens origins re-evaluate
    readonly property var names: ["AnthropicGlassIcons", "LiquidGlass", "Minimalist", "MaterialYou", "AnthropicDesign"]

    function select(n) {
        const k = String(n).toLowerCase().replace(/[-_ ]/g, "")
        for (const x of names) if (x.toLowerCase() === k) { name = x; console.log("theme ->", x); return true }
        console.log("theme unknown:", n)
        return false
    }

    readonly property var sets: ({
        AnthropicGlassIcons: {
            surfGlass: false, iconGlass: true, radius: 14, tileRadius: 17, blur: 0, refraction: 0, bezel: 1, rim: 0,
            wall: { light: "qrc:/FlowShell/assets/wallpapers/kiln-light.png", dark: "qrc:/FlowShell/assets/wallpapers/night-paper-dark.png" },
            light: { text: "#141413", dim: "#6b6a64", accent: "#D97757", tint: "#FAF9F5", tintA: 0.0, rim: "#ffffff", flat: "#FAF9F5F7",
                     hair: "#3d3929", onWall: "#FAF9F5", w0: "#D97757", w1: "#D97757", w2: "#D97757", w3: "#D97757", icon: "#2B2A26", card: "#FAF9F5",
                     glassTint: "#FAF9F5", glyph: "dark" },
            dark:  { text: "#FAF9F5", dim: "#b0aea5", accent: "#D97757", tint: "#262624", tintA: 0.0, rim: "#ffffff", flat: "#262624F7",
                     hair: "#FAF9F5", onWall: "#FAF9F5", w0: "#1d1c1a", w1: "#1d1c1a", w2: "#1d1c1a", w3: "#1d1c1a", icon: "#FAF9F5", card: "#262624",
                     glassTint: "#3a3835", glyph: "light" } },
        LiquidGlass: {
            surfGlass: true, iconGlass: true, radius: 28, tileRadius: 16, blur: 22, refraction: 20, bezel: 26, rim: 0.85,
            wall: null,
            light: { text: "#12151c", dim: "#5c6272", accent: "#2f7bff", tint: "#ffffff", tintA: 0.14, rim: "#ffffff", flat: "#d8dfeaE6", hair: "#12151c",
                     onWall: "#12151c", w0: "#bcd3ff", w1: "#ffd6e8", w2: "#6ea8ff", w3: "#ffb48a", icon: "#2b3040", card: "#ffffff", glassTint: "#ffffff", glyph: "dark" },
            dark:  { text: "#f4f6fb", dim: "#a2a9ba", accent: "#5ea0ff", tint: "#10131c", tintA: 0.22, rim: "#ffffff", flat: "#1c2130E6", hair: "#f4f6fb",
                     onWall: "#f4f6fb", w0: "#0b1030", w1: "#2a1040", w2: "#2459c8", w3: "#c4457a", icon: "#e8ecf6", card: "#141824", glassTint: "#20263a", glyph: "light" } },
        Minimalist: {
            surfGlass: false, iconGlass: false, radius: 14, tileRadius: 10, blur: 0, refraction: 0, bezel: 1, rim: 0, wall: null,
            light: { text: "#111111", dim: "#777777", accent: "#111111", tint: "#ffffff", tintA: 0.0, rim: "#000000", flat: "#f6f6f6F2", hair: "#111111",
                     onWall: "#111111", w0: "#fafafa", w1: "#ececec", w2: "#f2f2f2", w3: "#e4e4e4", icon: "#111111", card: "#ffffff", glassTint: "#ffffff", glyph: "dark" },
            dark:  { text: "#f2f2f2", dim: "#8a8a8a", accent: "#f2f2f2", tint: "#000000", tintA: 0.0, rim: "#ffffff", flat: "#141414F2", hair: "#f2f2f2",
                     onWall: "#f2f2f2", w0: "#050505", w1: "#101010", w2: "#0a0a0a", w3: "#1a1a1a", icon: "#f2f2f2", card: "#161616", glassTint: "#222222", glyph: "light" } },
        MaterialYou: {
            surfGlass: false, iconGlass: false, radius: 32, tileRadius: 26, blur: 0, refraction: 0, bezel: 1, rim: 0, wall: null,
            light: { text: "#1d1b20", dim: "#49454f", accent: "#6750a4", tint: "#ffffff", tintA: 0.0, rim: "#ffffff", flat: "#ece6f0FF", hair: "#1d1b20",
                     onWall: "#1d1b20", w0: "#f7f2fa", w1: "#e8def8", w2: "#d0bcff", w3: "#efb8c8", icon: "#4a4458", card: "#f3edf7", glassTint: "#ffffff", glyph: "dark" },
            dark:  { text: "#e6e0e9", dim: "#cac4d0", accent: "#d0bcff", tint: "#000000", tintA: 0.0, rim: "#ffffff", flat: "#2b2930FF", hair: "#e6e0e9",
                     onWall: "#e6e0e9", w0: "#141218", w1: "#211f26", w2: "#381e72", w3: "#4a2532", icon: "#e8def8", card: "#211f26", glassTint: "#2b2930", glyph: "light" } },
        AnthropicDesign: {
            surfGlass: false, iconGlass: false, radius: 20, tileRadius: 14, blur: 0, refraction: 0, bezel: 1, rim: 0, wall: null,
            light: { text: "#141413", dim: "#6b6a64", accent: "#d97757", tint: "#ffffff", tintA: 0.0, rim: "#ffffff", flat: "#faf9f5F5", hair: "#141413",
                     onWall: "#141413", w0: "#f0eee6", w1: "#e8e6dc", w2: "#ebdbbc", w3: "#e4b8a4", icon: "#141413", card: "#faf9f5", glassTint: "#faf9f5", glyph: "dark" },
            dark:  { text: "#faf9f5", dim: "#b0aea5", accent: "#d97757", tint: "#141413", tintA: 0.0, rim: "#ffffff", flat: "#262624F5", hair: "#faf9f5",
                     onWall: "#faf9f5", w0: "#141413", w1: "#1f1e1d", w2: "#3a2a22", w3: "#5a2f22", icon: "#faf9f5", card: "#262624", glassTint: "#262624", glyph: "light" } }
    })
    readonly property var cur: sets[name] || sets.AnthropicGlassIcons
    readonly property var c: dark ? cur.dark : cur.light

    readonly property color text: c.text
    readonly property color textDim: c.dim
    readonly property color accent: c.accent
    readonly property color iconInk: c.icon
    readonly property color card: c.card
    readonly property color flatSurface: c.flat
    readonly property color hairline: Qt.rgba(Qt.color(c.hair).r, Qt.color(c.hair).g, Qt.color(c.hair).b, dark ? 0.22 : 0.20)
    readonly property color onWall: c.onWall
    readonly property color wall0: c.w0
    readonly property color wall1: c.w1
    readonly property color wall2: c.w2
    readonly property color wall3: c.w3
    readonly property string wallpaperSource: cur.wall ? (dark ? cur.wall.dark : cur.wall.light) : ""
    readonly property real radius: cur.radius
    readonly property real tileRadius: cur.tileRadius

    // surface glass level: 0 full lensing, 1 blur only, 2 flat. Forced flat by reduce-transparency or flat-surface themes.
    readonly property int quality: (reduceTransparency || !cur.surfGlass) ? 2 : glass === "flat" ? 2 : glass === "blur" ? 1 : 0
    // icon glass level: same ladder, but only for themes with glass icons.
    readonly property int iconQuality: (reduceTransparency || !cur.iconGlass) ? 2 : glass === "flat" ? 2 : glass === "blur" ? 1 : 0
    readonly property color tint: c.tint
    readonly property color glassTint: c.glassTint
    readonly property string glyphInk: c.glyph          // "dark" ink glyphs on light glass, "light" on dark glass
    readonly property real tintA: c.tintA
    readonly property color rimColor: c.rim
    readonly property real blurPx: cur.blur
    readonly property real refractPx: cur.refraction
    readonly property real bezelPx: cur.bezel
    readonly property real rim: cur.rim
    readonly property int animMs: reduceMotion ? 0 : 280
    readonly property bool paper: name === "AnthropicGlassIcons" || name === "AnthropicDesign" || name === "Minimalist"
    readonly property string font: "Inter"
    readonly property string serif: (name === "AnthropicGlassIcons" || name === "AnthropicDesign") ? "Noto Serif" : "Inter"
}
