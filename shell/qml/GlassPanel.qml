import QtQuick
import FlowShell

// THE glass surface. Every surface in the shell is a GlassPanel.
// `backdrop` is a shared pre-downsampled ShaderEffectSource of the wallpaper (set by Main).
// Ladder: Theme.quality 0 = blur+lensing+specular, 1 = blur only, 2 = flat translucent rect.
Item {
    id: root
    property real radius: Theme.radius
    property real blurRadius: Theme.blurPx
    property real refraction: Theme.refractPx
    property real tintStrength: 0.0           // extra adaptive tint (0..1)
    property var backdrop: Theme.backdrop       // ShaderEffectSource
    property Item backdropItem: Theme.backdropItem // item whose coordinate space the source covers
    property bool flatOpaque: false
    default property alias content: holder.data

    readonly property bool useShader: Theme.quality < 2 && backdrop && backdropItem

    // flat fallback (also used by non-glass themes)
    Rectangle {
        anchors.fill: parent
        visible: !root.useShader
        radius: root.radius
        color: root.flatOpaque ? Theme.card : Theme.flatSurface
        border.width: Theme.name === "Minimalist" ? 1 : 0
        border.color: Theme.dark ? "#33ffffff" : "#22000000"
    }

    ShaderEffect {
        id: fx
        anchors.fill: parent
        visible: root.useShader
        property vector2d size: Qt.vector2d(width, height)
        property vector2d origin: {
            // re-evaluate when moved: depend on scene position through x/y of ancestors
            root.x; root.y; root.parent ? root.parent.x : 0; root.parent ? root.parent.y : 0; root.width;
            if (!root.backdropItem) return Qt.vector2d(0, 0)
            const p = root.mapToItem(root.backdropItem, 0, 0)
            return Qt.vector2d(p.x, p.y)
        }
        property vector2d backdropSize: root.backdropItem ? Qt.vector2d(root.backdropItem.width, root.backdropItem.height) : Qt.vector2d(1, 1)
        property real radius: root.radius
        property real blur: root.blurRadius
        property real refraction: root.refraction
        property real bezel: Theme.bezelPx
        property real rim: Theme.rim
        property real tintAmount: Math.min(1, Theme.tintA + root.tintStrength)
        property real quality: Theme.quality
        property real dark: Theme.dark ? 1 : 0
        property color tint: Qt.rgba(Theme.tint.r, Theme.tint.g, Theme.tint.b, Theme.tintA)
        property color rimColor: Theme.rimColor
        property var src: root.backdrop
        fragmentShader: "qrc:/FlowShell/shaders/glass.frag.qsb"
    }

    Item { id: holder; anchors.fill: parent }
}
