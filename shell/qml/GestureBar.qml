import QtQuick
import FlowShell

Item {
    id: root
    height: 28
    property color ink: Theme.onWall
    signal home
    signal switcher
    signal dragged(real dy)
    signal released(real dy, real vy)
    Rectangle {
        id: pill
        width: 136; height: 5; radius: 3; color: root.ink; opacity: 0.55
        anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 8 }
    }
    // swipe up fast = home, slow/held = switcher (the compositor does the same on real edge touches)
    DragHandler {
        id: drag
        target: null; yAxis.enabled: true; xAxis.enabled: false
        onTranslationChanged: if (active) root.dragged(-translation.y)
        onActiveChanged: if (!active) root.released(-translation.y, centroid.velocity.y)
    }
}
