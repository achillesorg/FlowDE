import QtQuick
import FlowShell

GlassPanel {
    id: root
    property string label: ""
    property string glyph: ""
    property bool active: false
    signal toggled
    radius: Math.min(width, height) * 0.36
    Rectangle { anchors.fill: parent; radius: root.radius; color: Theme.accent; opacity: root.active ? 0.85 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.animMs / 2 } } }
    Column {
        anchors.centerIn: parent; spacing: 4
        Text { anchors.horizontalCenter: parent.horizontalCenter; text: root.glyph; font.pixelSize: 22
            color: root.active ? "white" : Theme.text; font.family: Theme.font }
        Text { anchors.horizontalCenter: parent.horizontalCenter; text: root.label; font.pixelSize: 11
            color: root.active ? "white" : Theme.text; font.family: Theme.font }
    }
    scale: ma.pressed ? 0.94 : 1
    Behavior on scale { enabled: !Theme.reduceMotion; SpringAnimation { spring: 4; damping: 0.35; epsilon: 0.002 } }
    MouseArea { id: ma; anchors.fill: parent; onClicked: root.toggled() }
}
