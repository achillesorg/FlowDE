import QtQuick
import FlowShell

Item {
    id: root
    property real progress: 0          // 0 = locked, 1 = unlocked (slid away)
    property date now: new Date()
    Timer { interval: 5000; running: true; repeat: true; onTriggered: root.now = new Date() }
    opacity: 1 - progress * 0.9
    y: -progress * height * 0.55
    visible: progress < 0.999

    Column {
        id: clock
        anchors { top: parent.top; topMargin: 110; horizontalCenter: parent.horizontalCenter }
        spacing: 2
        Text { anchors.horizontalCenter: parent.horizontalCenter; text: Qt.formatDate(root.now, "dddd, d MMMM")
            color: Theme.onWall; opacity: 0.92; font.pixelSize: 18; font.weight: Font.Medium; font.family: Theme.font }
        Text { anchors.horizontalCenter: parent.horizontalCenter; text: Qt.formatTime(root.now, "hh:mm")
            color: Theme.onWall; font.pixelSize: 112; font.weight: Theme.serif === "Inter" ? Font.Thin : Font.Light; font.family: Theme.serif }
    }

    // notifications from kiln-notifs (mock model until the daemon exists)
    ListView {
        id: nl
        anchors { top: clock.bottom; topMargin: 34; left: parent.left; right: parent.right; leftMargin: 16; rightMargin: 16 }
        height: 3 * 88; interactive: false; spacing: 10
        model: Services.dnd ? null : Services.notifications
        delegate: GlassPanel {
            required property int index
            required property string app
            required property string title
            required property string body
            visible: index < 3
            width: nl.width; height: visible ? 78 : 0; radius: 24
            Row {
                anchors.fill: parent; anchors.margins: 14; spacing: 12
                AppIcon { size: 40; showLabel: false; label: app; glass: false }
                Column {
                    width: parent.width - 60; anchors.verticalCenter: parent.verticalCenter; spacing: 2
                    Text { width: parent.width; elide: Text.ElideRight; font.family: Theme.font
                        text: title; color: Theme.text; font.pixelSize: 15; font.weight: Font.DemiBold }
                    Text { width: parent.width; elide: Text.ElideRight; font.family: Theme.font
                        text: body; color: Theme.textDim; font.pixelSize: 13 }
                }
            }
        }
    }

    Row {
        anchors { bottom: hint.top; bottomMargin: 26; left: parent.left; right: parent.right; leftMargin: 42; rightMargin: 42 }
        Item { width: parent.width / 2; height: 60; GlassIcon { size: 56; name: "Sleep" } }
        Item { width: parent.width / 2; height: 60; GlassIcon { size: 56; name: "Camera"; anchors.right: parent.right } }
    }
    Text { id: hint; anchors { bottom: parent.bottom; bottomMargin: 46; horizontalCenter: parent.horizontalCenter }
        text: "Swipe up to unlock"; color: Theme.onWall; opacity: 0.75; font.pixelSize: 13; font.family: Theme.font }
}
