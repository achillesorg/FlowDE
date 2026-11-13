import QtQuick
import FlowShell

// Ink-outline icon fallback: rounded outline + glyph. Used until an icon pack is chosen.
Item {
    id: root
    property string label: ""
    property string icon: ""
    property bool showLabel: true
    property real size: 60
    signal clicked
    width: size + 16; height: size + (showLabel ? 28 : 0)
    scale: ma.pressed ? 0.92 : 1
    Behavior on scale { enabled: !Theme.reduceMotion; SpringAnimation { spring: 4; damping: 0.35; epsilon: 0.002 } }

    Rectangle {
        id: box
        width: root.size; height: root.size; x: (root.width - root.size) / 2
        radius: Theme.tileRadius
        color: Theme.dark ? "#14ffffff" : "#55ffffff"
        border.width: 1.6
        border.color: Theme.iconInk
        Text {
            anchors.centerIn: parent
            text: root.label.length ? root.label.charAt(0).toUpperCase() : "?"
            color: Theme.iconInk
            font.pixelSize: root.size * 0.46
            font.weight: Font.Light
            font.family: Theme.font
        }
        // small ink flourish: inset corner tick
        Rectangle { width: root.size * 0.22; height: 1.6; color: Theme.iconInk; x: root.size * 0.14; y: root.size * 0.84; opacity: 0.7 }
    }
    Text {
        visible: root.showLabel
        anchors { top: box.bottom; topMargin: 6; horizontalCenter: parent.horizontalCenter }
        width: root.width + 8; horizontalAlignment: Text.AlignHCenter
        text: root.label; elide: Text.ElideRight
        color: Theme.text; font.pixelSize: 12; font.family: Theme.font
    }
    MouseArea { id: ma; anchors.fill: parent; onClicked: root.clicked() }
}
