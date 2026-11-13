import QtQuick
import FlowShell

Item {
    id: root
    height: 28
    signal home
    signal switcher
    Rectangle {
        id: pill
        width: 136; height: 5; radius: 3; color: Theme.text; opacity: 0.55
        anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 8 }
    }
    // swipe up fast = home, slow/held = switcher (the compositor does the same on real edge touches)
    DragHandler {
        id: drag
        target: null; yAxis.enabled: true; xAxis.enabled: false
        onActiveChanged: if (!active) {
            const dy = -translation.y
            if (dy > 60) { (centroid.velocity.y < -900) ? root.home() : root.switcher() }
        }
    }
}
