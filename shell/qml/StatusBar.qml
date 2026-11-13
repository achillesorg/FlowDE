import QtQuick
import FlowShell

Item {
    id: root
    height: 40
    property date now: new Date()
    Timer { interval: 10000; running: true; repeat: true; onTriggered: root.now = new Date() }
    Text {
        x: 28; anchors.verticalCenter: parent.verticalCenter
        text: Qt.formatTime(root.now, "hh:mm"); color: Theme.text
        font.pixelSize: 15; font.weight: Font.DemiBold; font.family: Theme.font
    }
    Row {
        anchors { right: parent.right; rightMargin: 24; verticalCenter: parent.verticalCenter }
        spacing: 9
        // signal bars
        Row {
            spacing: 2; anchors.verticalCenter: parent.verticalCenter
            Repeater { model: 4
                Rectangle { width: 3.5; height: 5 + index * 3; radius: 1; anchors.bottom: parent.bottom
                    color: Theme.text; opacity: (Services.airplane ? 0 : index < Services.signal) ? 1 : 0.28 } }
        }
        // wifi: three arcs
        Canvas {
            width: 18; height: 14; anchors.verticalCenter: parent.verticalCenter
            property color ink: Theme.text
            property bool on: Services.wifi && !Services.airplane
            onInkChanged: requestPaint(); onOnChanged: requestPaint()
            onPaint: {
                const c = getContext("2d"); c.clearRect(0, 0, width, height)
                c.strokeStyle = ink; c.fillStyle = ink; c.lineWidth = 2; c.lineCap = "round"
                c.globalAlpha = on ? 1 : 0.28
                for (let i = 1; i <= 3; i++) { c.beginPath(); c.arc(9, 13, i * 4, -Math.PI * 0.75, -Math.PI * 0.25); c.stroke() }
                c.beginPath(); c.arc(9, 12.5, 1.4, 0, 6.3); c.fill()
            }
        }
        // battery
        Row {
            spacing: 4; anchors.verticalCenter: parent.verticalCenter
            Text { text: Services.battery; color: Theme.text; font.pixelSize: 12; anchors.verticalCenter: parent.verticalCenter; font.family: Theme.font }
            Rectangle {
                width: 26; height: 13; radius: 4; color: "transparent"; border.width: 1.4; border.color: Theme.text
                anchors.verticalCenter: parent.verticalCenter
                Rectangle { x: 2; y: 2; height: parent.height - 4; width: (parent.width - 4) * Services.battery / 100; radius: 2
                    color: Services.charging ? "#34c759" : (Services.battery < 20 ? "#ff453a" : Theme.text) }
            }
        }
    }
}
