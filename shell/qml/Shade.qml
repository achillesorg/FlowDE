import QtQuick
import FlowShell

// Notification shade + quick settings. progress 0..1 slides it down from the top.
Item {
    id: root
    property real progress: 0
    visible: progress > 0.001
    y: -(1 - progress) * height
    GlassPanel {
        anchors.fill: parent; radius: 0; blurRadius: Theme.blurPx * 1.4; refraction: 0; tintStrength: 0.08
        flatOpaque: true
        // bottom edge radius cap
    }
    Column {
        anchors { fill: parent; topMargin: 56; leftMargin: 18; rightMargin: 18; bottomMargin: 44 }
        spacing: 14
        Grid {
            width: parent.width; columns: 3; spacing: 12
            readonly property real tw: (width - 24) / 3
            Toggle { width: parent.tw; height: 78; label: "Wi-Fi"; glyph: "◠"; active: Services.wifi; onToggled: Services.wifi = !Services.wifi }
            Toggle { width: parent.tw; height: 78; label: "Bluetooth"; glyph: "ᛒ"; active: Services.bluetooth; onToggled: Services.bluetooth = !Services.bluetooth }
            Toggle { width: parent.tw; height: 78; label: "Airplane"; glyph: "✈"; active: Services.airplane; onToggled: Services.airplane = !Services.airplane }
            Toggle { width: parent.tw; height: 78; label: "Rotation"; glyph: "↻"; active: Services.rotationLock; onToggled: Services.rotationLock = !Services.rotationLock }
            Toggle { width: parent.tw; height: 78; label: "Do not disturb"; glyph: "☾"; active: Services.dnd; onToggled: Services.dnd = !Services.dnd }
            Toggle { width: parent.tw; height: 78; label: Theme.dark ? "Dark" : "Light"; glyph: "◐"; active: Theme.dark; onToggled: Theme.dark = !Theme.dark }
        }
        SliderRow { width: parent.width; height: 52; glyph: "☀"; value: Services.brightness; onMoved: (v) => Services.brightness = v }
        SliderRow { width: parent.width; height: 52; glyph: "♪"; value: Services.volume; onMoved: (v) => Services.volume = v }
        Item { width: parent.width; height: 26
            Text { text: "Notifications"; color: Theme.text; font.pixelSize: 15; font.weight: Font.DemiBold; anchors.verticalCenter: parent.verticalCenter; font.family: Theme.font }
            Text { anchors { right: parent.right; verticalCenter: parent.verticalCenter } text: "Clear"; color: Theme.accent; font.pixelSize: 14; font.family: Theme.font
                MouseArea { anchors.fill: parent; anchors.margins: -10; onClicked: Services.notifications.clear() } } }
        ListView {
            id: nl
            width: parent.width; height: parent.height - 78 * 2 - 12 - 52 * 2 - 26 - 14 * 4 - 16
            spacing: 10; clip: true; model: Services.notifications
            delegate: GlassPanel {
                required property int index
                required property string app
                required property string title
                required property string body
                required property string time
                width: nl.width; height: 74; radius: 22
                Row { anchors.fill: parent; anchors.margins: 12; spacing: 12
                    AppIcon { size: 38; showLabel: false; label: app }
                    Column { width: parent.width - 54; anchors.verticalCenter: parent.verticalCenter; spacing: 2
                        Row { width: parent.width
                            Text { width: parent.width - 40; text: title; elide: Text.ElideRight; color: Theme.text; font.pixelSize: 15; font.weight: Font.DemiBold; font.family: Theme.font }
                            Text { text: time; color: Theme.textDim; font.pixelSize: 12; font.family: Theme.font } }
                        Text { width: parent.width; text: body; elide: Text.ElideRight; color: Theme.textDim; font.pixelSize: 13; font.family: Theme.font } }
                }
                MouseArea { anchors.fill: parent; onClicked: Services.notifications.dismiss(index) }
            }
        }
    }
    Rectangle { anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 14 }
        width: 44; height: 5; radius: 3; color: Theme.text; opacity: 0.5 }
}
