import QtQuick
import FlowShell

// Liquid-glass app icon (the ONLY glass in the default theme). A ShaderEffect lens tile reads the shared
// wallpaper source behind it (Theme.backdrop): refraction + dispersion + specular rim + dark inner edge +
// adaptive tint, soft contact shadow. Ladder: Theme.iconQuality 0 full lens -> 1 blur only -> 2 flat tile.
Item {
    id: root
    property real size: 60
    property string name: ""       // app name: picks a FeDe ink glyph, else the initial letter
    property int quality: Theme.iconQuality
    property bool pressed: false
    width: size; height: size

    // Every icon has a real 1.5px line glyph (FeDe/Glaze ink set + our own in assets-src); unknown apps get "app".
    function glyphFor(n) {
        const s = n.toLowerCase()
        const map = [[/messag|chat|sms/, "chat"], [/phone|dial/, "phone"], [/camera/, "camera"], [/photo|gallery|image/, "photos"],
                     [/calendar/, "calendar"], [/clock|alarm|timer/, "clock"], [/music|podcast|audio|sound|volume/, "music"],
                     [/map/, "maps"], [/note|text|document/, "notes"], [/weather/, "weather"], [/wallet|pay/, "wallet"],
                     [/files|folder|file roller/, "folder"], [/setting/, "settings"], [/terminal|foot|konsole/, "terminal"],
                     [/browser|firefox|chrom|web/, "browser"], [/noxal|assistant/, "noxal"], [/search/, "search"],
                     [/network|wi-?fi|bluetooth/, "network-wifi"], [/lock/, "lock"], [/sleep/, "sleep"], [/store/, "apps"],
                     [/notif/, "bell"], [/system|power|disk/, "power"]]
        for (const m of map) if (m[0].test(s)) return m[1]
        return "app"
    }
    readonly property string glyph: glyphFor(name)

    // contact shadow (stacked translucent rects: no blur pass)
    Repeater {
        model: 4
        Rectangle {
            required property int index
            x: -index * 1.6; y: 3 + index * 1.2; width: root.size + index * 3.2; height: root.size + index * 2
            radius: Theme.tileRadius + index * 1.5
            color: "black"; opacity: (Theme.dark ? 0.20 : 0.11) * (1 - index * 0.2)
            visible: root.quality < 2 || Theme.name === "AnthropicGlassIcons"
        }
    }

    // flat tile (quality 2, or other themes' icon style)
    Rectangle {
        anchors.fill: parent; visible: root.quality >= 2
        radius: Theme.tileRadius
        color: Theme.paper ? Theme.card : Qt.rgba(Theme.glassTint.r, Theme.glassTint.g, Theme.glassTint.b, 0.85)
        border.width: Theme.paper ? 1 : 0; border.color: Theme.hairline
    }

    ShaderEffect {
        anchors.fill: parent
        visible: root.quality < 2 && Theme.backdrop !== null
        property vector2d size: Qt.vector2d(width, height)
        property vector2d origin: {
            root.x; root.y; Theme.scrollTick; root.width; root.scale
            if (!Theme.backdropItem) return Qt.vector2d(0, 0)
            const p = root.mapToItem(Theme.backdropItem, 0, 0)
            return Qt.vector2d(p.x, p.y)
        }
        property vector2d backdropSize: Theme.backdropItem ? Qt.vector2d(Theme.backdropItem.width, Theme.backdropItem.height) : Qt.vector2d(1, 1)
        property real radius: Theme.tileRadius
        property real bezel: root.size * 0.36
        property real refraction: root.size * 0.42
        property real dispersion: 0.12
        property real zoom: 0.86
        property real quality: root.quality
        property real dark: Theme.dark ? 1 : 0
        property real rim: 0.95
        property color tint: Qt.rgba(Theme.glassTint.r, Theme.glassTint.g, Theme.glassTint.b, 0.34)
        property color rimColor: "#ffffff"
        property var src: Theme.backdrop
        fragmentShader: "qrc:/FlowShell/shaders/icon.frag.qsb"
    }

    Image {
        anchors.centerIn: parent
        width: root.size * 0.52; height: width
        sourceSize: Qt.size(96, 96); smooth: true
        source: "qrc:/FlowShell/assets/icons/" + root.glyph + "-" + Theme.glyphInk + ".svg"
        opacity: 0.94
    }
}
