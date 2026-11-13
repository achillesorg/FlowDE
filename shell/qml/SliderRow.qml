import QtQuick
import FlowShell

GlassPanel {
    id: root
    property real value: 0.5
    property string glyph: ""
    signal moved(real v)
    radius: height / 2
    Rectangle {
        x: 0; width: Math.max(root.height, root.width * root.value); height: parent.height; radius: root.radius
        color: Theme.dark ? "#d0ffffff" : "#f0ffffff"; opacity: 0.9
    }
    Text { x: 18; anchors.verticalCenter: parent.verticalCenter; text: root.glyph; font.pixelSize: 18; color: "#3a3f4d" }
    MouseArea {
        anchors.fill: parent
        onPositionChanged: (m) => root.moved(Math.max(0, Math.min(1, m.x / width)))
        onPressed: (m) => root.moved(Math.max(0, Math.min(1, m.x / width)))
    }
}
