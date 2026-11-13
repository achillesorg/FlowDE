import QtQuick
import FlowShell

// Built-in app page (prototype stubs in the paper style). Opens from the tapped icon with a spring
// (scale/translate from `origin`), closes back to it. Real data comes later from flow-clock/flow-calendar/flow-settings.
Item {
    id: root
    property string name: ""
    property rect from: Qt.rect(240, 560, 62, 62)   // exact on-screen rect of the pressed icon tile
    property real fromRadius: Theme.tileRadius
    signal back
    property bool launching: false                   // real app being started: placeholder until its window maps
    property real p: 0                               // ONE driven progress 0 (icon) .. 1 (full screen); spring/finger-driven by Main
    property real sw: 540
    property real sh: 1170
    visible: p > 0.005

    // the shared element: same position, size, corner radius as the icon, morphing to full screen
    x: from.x * (1 - p); y: from.y * (1 - p)
    width: from.width + (sw - from.width) * p
    height: from.height + (sh - from.height) * p
    clip: true

    Rectangle {
        anchors.fill: parent
        radius: root.fromRadius * (1 - root.p)
        // icon tile colour -> paper
        color: Theme.card
        opacity: Math.min(1, root.p * 3)
        border.width: 1; border.color: Theme.hairline
    }
    // the icon itself (glyph included) stays in the top-left of the morphing rect and fades as content arrives
    GlassIcon {
        size: root.from.width; name: root.name
        opacity: 1 - Math.min(1, root.p * 3)
        x: (root.width - size) / 2; y: (root.height - size) / 2
    }

    // content: fixed full-screen layout, revealed once the shape is mostly open
    Item {
        id: content
        width: root.sw; height: root.sh
        opacity: Math.max(0, Math.min(1, (root.p - 0.55) * 3))
        visible: opacity > 0.01
    // header
    Item {
        id: head
        anchors { top: content.top; left: content.left; right: content.right; topMargin: 44 }
        height: 64
        GlassPanel {
            x: 16; anchors.verticalCenter: parent.verticalCenter; width: 40; height: 40; radius: Theme.radius
            Text { anchors.centerIn: parent; text: "‹"; font.pixelSize: 26; color: Theme.text; anchors.verticalCenterOffset: -2 }
            MouseArea { anchors.fill: parent; anchors.margins: -8; onClicked: root.back() }
        }
        Text { anchors.centerIn: parent; text: root.name; color: Theme.text; font.pixelSize: 24; font.family: Theme.serif }
        GlassIcon { size: 36; name: root.name; anchors { right: parent.right; rightMargin: 16; verticalCenter: parent.verticalCenter } }
        Rectangle { anchors { bottom: parent.bottom; left: parent.left; right: parent.right } height: 1; color: Theme.hairline }
    }

    Item {
        id: body
        anchors { top: head.bottom; left: content.left; right: content.right; bottom: content.bottom; bottomMargin: 36 }

        Loader {
            anchors.fill: parent
            sourceComponent: {
                switch (root.name) {
                case "Clock": return clockC
                case "Calendar": return calC
                case "Notes": return notesC
                case "Settings": return settingsC
                case "Phone": return phoneC
                case "Noxal": return noxalC
                default: return emptyC
                }
            }
        }
    }

    }

    component Row2: Item {
        id: r
        property string title: ""
        property string sub: ""
        property bool toggle: false
        property bool on: false
        signal clicked
        width: parent ? parent.width : 0; height: 62
        Column { x: 20; anchors.verticalCenter: parent.verticalCenter; spacing: 2
            Text { text: r.title; color: Theme.text; font.pixelSize: 16; font.family: Theme.font }
            Text { visible: r.sub !== ""; text: r.sub; color: Theme.textDim; font.pixelSize: 12; font.family: Theme.font } }
        Rectangle { visible: r.toggle; anchors { right: parent.right; rightMargin: 20; verticalCenter: parent.verticalCenter }
            width: 46; height: 26; radius: 13; color: r.on ? Theme.accent : "transparent"; border.width: 1; border.color: r.on ? Theme.accent : Theme.hairline
            Rectangle { width: 20; height: 20; radius: 10; y: 3; x: r.on ? 23 : 3; color: r.on ? "#FAF9F5" : Theme.textDim
                Behavior on x { enabled: !Theme.reduceMotion; SpringAnimation { spring: 5; damping: 0.4; epsilon: 0.5 } } } }
        Rectangle { anchors { bottom: parent.bottom; left: parent.left; right: parent.right; leftMargin: 20 } height: 1; color: Theme.hairline }
        MouseArea { anchors.fill: parent; onClicked: r.clicked() }
    }

    Component { id: clockC
        Item {
            property date now: new Date()
            Timer { interval: 1000; running: true; repeat: true; onTriggered: parent.now = new Date() }
            Text { id: big; anchors { top: parent.top; topMargin: 40; horizontalCenter: parent.horizontalCenter }
                text: Qt.formatTime(parent.now, "hh:mm:ss"); color: Theme.text; font.pixelSize: 64; font.family: Theme.serif; font.weight: Font.Light }
            Text { anchors { top: big.bottom; horizontalCenter: parent.horizontalCenter } text: Qt.formatDate(parent.now, "dddd, d MMMM"); color: Theme.textDim; font.pixelSize: 15; font.family: Theme.font }
            Column { anchors { top: big.bottom; topMargin: 70; left: parent.left; right: parent.right }
                Text { x: 20; text: "Alarms (mock; flow-clock pending)"; color: Theme.textDim; font.pixelSize: 12; font.family: Theme.font; bottomPadding: 8 }
                Row2 { title: "07:00"; sub: "Weekdays"; toggle: true; on: true; onClicked: on = !on }
                Row2 { title: "22:30"; sub: "Wind down"; toggle: true; on: false; onClicked: on = !on }
                Row2 { title: "Timer"; sub: "10 min" }
            }
        } }
    Component { id: calC
        Column {
            Text { x: 20; topPadding: 18; bottomPadding: 8; text: Qt.formatDate(new Date(), "dddd d MMMM") + "  (mock; flow-calendar pending)"; color: Theme.textDim; font.pixelSize: 12; font.family: Theme.font }
            Row2 { title: "Design review"; sub: "15:00 - 15:45  Room 4" }
            Row2 { title: "Dinner with Mara"; sub: "20:00  Table booked" }
            Row2 { title: "Plan FlowDE stage 2"; sub: "All day" }
        } }
    Component { id: notesC
        Item {
            Flickable { anchors.fill: parent; anchors.margins: 20; contentHeight: te.height; clip: true
                TextEdit { id: te; width: parent.width; height: Math.max(implicitHeight, 400); color: Theme.text; font.pixelSize: 17; font.family: Theme.serif; wrapMode: TextEdit.Wrap
                    text: "Kiln shell ideas\n\n- flat paper surfaces\n- glass only for icons\n"; selectByMouse: true } }
        } }
    Component { id: settingsC
        Column {
            Text { x: 20; topPadding: 18; bottomPadding: 8; text: "Live prototype settings (flow-settings keys pending)"; color: Theme.textDim; font.pixelSize: 12; font.family: Theme.font }
            Row2 { title: "Dark mode"; toggle: true; on: Theme.dark; onClicked: Theme.dark = !Theme.dark }
            Row2 { title: "Theme"; sub: Theme.name + "  (tap to switch)"; onClicked: { const i = Theme.names.indexOf(Theme.name); Theme.select(Theme.names[(i + 1) % Theme.names.length]) } }
            Row2 { title: "Glass icons quality"; sub: Services.glassQuality; onClicked: Services.glassQuality = (Services.glassQuality === "full" ? "blur" : Services.glassQuality === "blur" ? "flat" : "full") }
            Row2 { title: "Reduce transparency"; toggle: true; on: Services.reduceTransparency; onClicked: Services.reduceTransparency = !Services.reduceTransparency }
            Row2 { title: "Reduce motion"; toggle: true; on: Services.reduceMotion; onClicked: Services.reduceMotion = !Services.reduceMotion }
            Row2 { title: "Do not disturb"; toggle: true; on: Services.dnd; onClicked: Services.dnd = !Services.dnd }
        } }
    Component { id: phoneC
        Item {
            property string num: ""
            Text { id: nl; anchors { top: parent.top; topMargin: 24; horizontalCenter: parent.horizontalCenter } text: parent.num.length ? parent.num : " "; color: Theme.text; font.pixelSize: 34; font.family: Theme.serif }
            Grid { anchors { top: nl.bottom; topMargin: 30; horizontalCenter: parent.horizontalCenter } columns: 3; spacing: 14
                Repeater { model: ["1","2","3","4","5","6","7","8","9","*","0","#"]
                    GlassPanel { required property string modelData; width: 76; height: 76; radius: 38
                        Text { anchors.centerIn: parent; text: modelData; font.pixelSize: 26; color: Theme.text; font.family: Theme.font }
                        MouseArea { anchors.fill: parent; onClicked: parent.parent.parent.parent.num += modelData } } } }
        } }
    Component { id: noxalC
        Item {
            Column { anchors { top: parent.top; topMargin: 20; left: parent.left; right: parent.right; margins: 20 } spacing: 12
                Rectangle { width: parent.width * 0.8; height: t1.height + 24; radius: Theme.radius; color: "transparent"; border.width: 1; border.color: Theme.hairline
                    Text { id: t1; x: 14; y: 12; width: parent.width - 28; wrapMode: Text.Wrap; text: "Hi, I am Noxal. Ask me to summarise your notifications or plan your day. (prototype: no model attached)"; color: Theme.text; font.pixelSize: 15; font.family: Theme.font } } }
            GlassPanel { anchors { bottom: parent.bottom; left: parent.left; right: parent.right; margins: 16 } height: 48; radius: 24
                Text { x: 20; anchors.verticalCenter: parent.verticalCenter; text: "Ask Noxal"; color: Theme.textDim; font.pixelSize: 15; font.family: Theme.font } }
        } }
    Component { id: emptyC
        Column { anchors.centerIn: parent; spacing: 18
            GlassIcon { size: 84; name: root.name; anchors.horizontalCenter: parent.horizontalCenter }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: root.name; color: Theme.text; font.pixelSize: 26; font.family: Theme.serif }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: "Prototype page. Nothing here yet."; color: Theme.textDim; font.pixelSize: 14; font.family: Theme.font } } }
}
