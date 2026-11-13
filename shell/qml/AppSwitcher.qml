import QtQuick
import FlowShell

Item {
    id: root
    signal picked(string name)
    signal closedAll
    signal closedOne(string name)
    property var running: []
    readonly property var colors: ["#D97757", "#E8E6DC", "#B0AEA5", "#788C5D", "#6A9BCC"]
    property var _unused: [
        { name: "Messages", c: "#D97757" }, { name: "Browser", c: "#E8E6DC" },
        { name: "Camera", c: "#B0AEA5" }, { name: "Files", c: "#788C5D" }, { name: "Noxal", c: "#6A9BCC" }]
    visible: opacity > 0.02 && running !== undefined
    ListView {
        id: list
        anchors { fill: parent; topMargin: 90; bottomMargin: 120 }
        orientation: ListView.Horizontal; spacing: 18; leftMargin: (width - cardW) / 2; rightMargin: leftMargin
        snapMode: ListView.SnapToItem; highlightRangeMode: ListView.StrictlyEnforceRange
        preferredHighlightBegin: leftMargin; preferredHighlightEnd: leftMargin + cardW
        readonly property real cardW: Math.min(root.width * 0.66, 340)
        model: root.running.map((n, i) => ({ name: n, c: root.colors[i % 5] }))
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
                Rectangle { anchors.fill: parent; anchors.margins: 10; anchors.topMargin: 54; radius: Theme.paper ? 10 : 22; color: card.modelData.c; opacity: 1 }
                Row { x: 16; y: 10; spacing: 10
                    AppIcon { size: 30; showLabel: false; label: card.modelData.name; glass: false }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: card.modelData.name; color: Theme.text; font.pixelSize: 15; font.weight: Font.DemiBold; font.family: Theme.font } }
                MouseArea { anchors.fill: parent; onClicked: root.picked(card.modelData.name) }
                DragHandler { id: dh; target: null; xAxis.enabled: false
                    onTranslationChanged: card.dy = Math.min(0, translation.y)
                    onActiveChanged: if (!active) { if (card.dy < -120) root.closedOne(card.modelData); else card.dy = 0 } }
            }
        }
    }
    GlassPanel {
        visible: root.running.length > 0
        anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 76 }
        width: 150; height: 44; radius: 22
        Text { visible: false; text: "" }
        Text { anchors.centerIn: parent; text: "Close all"; color: Theme.text; font.pixelSize: 14; font.family: Theme.font }
        MouseArea { anchors.fill: parent; onClicked: root.closedAll() }
    }
}
