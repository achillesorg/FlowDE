import QtQuick
import FlowShell

Item {
    id: root
    signal picked(string name)
    signal closedAll
    property var running: [
        { name: "Messages", c: "#4aa3ff" }, { name: "Browser", c: "#ff9f5a" },
        { name: "Camera", c: "#8e8e93" }, { name: "Files", c: "#5ac8a0" }, { name: "Noxal", c: "#b07cff" }]
    visible: opacity > 0.01
    ListView {
        id: list
        anchors { fill: parent; topMargin: 90; bottomMargin: 120 }
        orientation: ListView.Horizontal; spacing: 18; leftMargin: (width - cardW) / 2; rightMargin: leftMargin
        snapMode: ListView.SnapToItem; highlightRangeMode: ListView.StrictlyEnforceRange
        preferredHighlightBegin: leftMargin; preferredHighlightEnd: leftMargin + cardW
        readonly property real cardW: Math.min(root.width * 0.66, 340)
        model: root.running
        delegate: Item {
            id: card
            required property var modelData
            required property int index
            width: list.cardW; height: list.height
            // swipe up to close card (interruptible spring back)
            property real dy: 0
            y: dy
            opacity: 1 + Math.min(0, dy) / 500
            Behavior on dy { enabled: !dh.active; SpringAnimation { spring: 3; damping: 0.3; epsilon: 0.5 } }
            GlassPanel {
                anchors.fill: parent; radius: 30; flatOpaque: true
                // app snapshot placeholder
                Rectangle { anchors.fill: parent; anchors.margins: 10; anchors.topMargin: 54; radius: 22; color: card.modelData.c; opacity: 0.85 }
                Row { x: 16; y: 10; spacing: 10
                    AppIcon { size: 30; showLabel: false; label: card.modelData.name }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: card.modelData.name; color: Theme.text; font.pixelSize: 15; font.weight: Font.DemiBold; font.family: Theme.font } }
                MouseArea { anchors.fill: parent; onClicked: root.picked(card.modelData.name) }
                DragHandler { id: dh; target: null; xAxis.enabled: false
                    onTranslationChanged: card.dy = Math.min(0, translation.y)
                    onActiveChanged: if (!active) card.dy = (card.dy < -120) ? -list.height : 0 }
            }
        }
    }
    GlassPanel {
        anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 76 }
        width: 150; height: 44; radius: 22
        Text { anchors.centerIn: parent; text: "Close all"; color: Theme.text; font.pixelSize: 14; font.family: Theme.font }
        MouseArea { anchors.fill: parent; onClicked: { root.running = []; root.closedAll() } }
    }
}
