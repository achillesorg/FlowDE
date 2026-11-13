import QtQuick
import FlowShell

// Surface component used by every panel. In the default theme (anthropic-glass-icons) surfaces are FLAT
// PAPER: 1px hairline stroke, small radius, no blur, no shadow. In the LiquidGlass theme the same component
// runs the glass shader (blur + lensing + specular + adaptive tint). Ladder: Theme.quality 0 full, 1 blur, 2 flat.
Item {
    id: root
    property real radius: Theme.radius
    property real blurRadius: Theme.blurPx
    property real refraction: Theme.refractPx
    property real tintStrength: 0.0
    property var backdrop: Theme.backdrop
    property Item backdropItem: Theme.backdropItem
    property bool flatOpaque: false
    property bool outline: true
    default property alias content: holder.data

    readonly property bool useShader: Theme.quality < 2 && backdrop && backdropItem

    Rectangle {
        anchors.fill: parent
        visible: !root.useShader
        radius: root.radius
        color: root.flatOpaque ? Theme.card : Theme.flatSurface
        border.width: root.outline && Theme.paper ? 1 : 0
        border.color: Theme.hairline
    }

    ShaderEffect {
        id: fx
        anchors.fill: parent
        visible: root.useShader
        property vector2d size: Qt.vector2d(width, height)
        property vector2d origin: {
            root.x; root.y; root.parent ? root.parent.x : 0; root.parent ? root.parent.y : 0; root.width; Theme.scrollTick
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
