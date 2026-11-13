import QtQuick
import FlowShell

// App icon + label. Glass lens tile in glass-icon themes (GlassIcon), flat tile otherwise.
Item {
    id: root
    property string label: ""
    property string icon: ""
    property bool showLabel: true
    property bool glass: true          // false: always a flat tile (notification rows etc.)
    property color labelColor: Theme.onWall
    property real size: 60
    property alias tile: gi
    signal clicked
    width: size + 16; height: size + (showLabel ? 28 : 0)
    scale: ma.pressed ? 0.92 : 1
    Behavior on scale { enabled: !Theme.reduceMotion; SpringAnimation { spring: 4; damping: 0.35; epsilon: 0.002 } }

    GlassIcon {
        id: gi
        size: root.size; name: root.label
        x: (root.width - root.size) / 2
        quality: root.glass ? Theme.iconQuality : 2
    }
    Text {
        visible: root.showLabel
        anchors { top: gi.bottom; topMargin: 7; horizontalCenter: parent.horizontalCenter }
        width: root.width + 8; horizontalAlignment: Text.AlignHCenter
        text: root.label; elide: Text.ElideRight
        color: root.labelColor; font.pixelSize: 12; font.family: Theme.font
    }
    MouseArea { id: ma; anchors.fill: parent; onClicked: root.clicked() }
}
