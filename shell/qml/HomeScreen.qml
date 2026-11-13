import QtQuick
import FlowShell

Item {
    id: root
    property bool searchOpen: false
    property string query: ""
    property real dim: 0
    // curated home: 12 core apps + 4 dock apps (short names). Real .desktop apps stay reachable via search.
    readonly property var core: ["Photos", "Calendar", "Clock", "Music", "Maps", "Notes", "Files", "Weather", "Terminal", "Noxal", "Wallet", "Settings"]
    readonly property var dockNames: ["Phone", "Messages", "Browser", "Camera"]
    signal launched(string name, rect r, bool real)

    // Terminal/Files/Browser really launch a .desktop app when one exists; everything else opens a built-in page.
    readonly property var externalOk: []   // decision: no desktop apps on the phone; every app is a built-in FlowDE page
    function launchByName(name, r) {
        let real = false
        if (externalOk.indexOf(name) >= 0) {
            for (let i = 0; i < Apps.count; i++) {
                const a = Apps.get(i)
                if (a.exec !== "" && a.name.toLowerCase().indexOf(name.toLowerCase()) >= 0) { Apps.launch(i); real = true; break }
            }
        }
        root.launched(name, r, real)
    }

    // today card (flat paper)
    GlassPanel {
        id: today
        x: 16; y: 66; width: parent.width - 32; height: 104; radius: Theme.radius
        opacity: root.searchOpen ? 0.1 : 1
        Column {
            anchors { left: parent.left; leftMargin: 18; verticalCenter: parent.verticalCenter }
            spacing: 2
            Text { text: Qt.formatDate(new Date(), "dddd d MMMM"); color: Theme.textDim; font.pixelSize: 13; font.family: Theme.font }
            Text { text: "Design review in 15 min"; color: Theme.text; font.pixelSize: 22; font.family: Theme.serif }
            Text { text: "Room 4  /  video link attached"; color: Theme.textDim; font.pixelSize: 13; font.family: Theme.font }
        }
    }

    Grid {
        id: grid
        anchors { top: today.bottom; topMargin: 34; horizontalCenter: parent.horizontalCenter }
        columns: 4; rowSpacing: 38; columnSpacing: 0
        opacity: root.searchOpen ? 0.12 : 1; Behavior on opacity { NumberAnimation { duration: Theme.animMs } }
        scale: root.searchOpen ? 0.94 : 1; Behavior on scale { enabled: !Theme.reduceMotion; SpringAnimation { spring: 3; damping: 0.3; epsilon: 0.002 } }
        Repeater {
            model: root.core
            AppIcon {
                required property string modelData
                width: (root.width - 24) / 4; size: 62
                label: modelData
                onClicked: { const q = tile.mapToItem(root, 0, 0); root.launchByName(modelData, Qt.rect(q.x, q.y, size, size)) }
            }
        }
    }
    // pull down => search
    DragHandler { target: null; yAxis.enabled: true; xAxis.enabled: false
        onActiveChanged: if (!active && translation.y > 70 && Math.abs(translation.x) < 60) root.searchOpen = true }

    Item {
        id: dock
        height: 92
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; leftMargin: 16; rightMargin: 16; bottomMargin: 38 }
        // glass themes: glass surface; image-wallpaper paper theme: hairline outline only (icons refract the wallpaper through it)
        GlassPanel { anchors.fill: parent; radius: 30; visible: Theme.wallpaperSource === ""; outline: true }
        Rectangle { anchors.fill: parent; radius: Theme.radius + 8; color: "transparent"; visible: Theme.wallpaperSource !== ""
            border.width: 1; border.color: Qt.rgba(Theme.onWall.r, Theme.onWall.g, Theme.onWall.b, 0.5) }
        Row {
            anchors.centerIn: parent
            Repeater { model: root.dockNames
                AppIcon { required property string modelData; label: modelData; showLabel: false; size: 58; width: (dock.width - 16) / 4
                    onClicked: { const q = tile.mapToItem(root, 0, 0); root.launchByName(modelData, Qt.rect(q.x, q.y, size, size)) } } }
        }
    }

    // search overlay
    Item {
        anchors.fill: parent; visible: opacity > 0.01; opacity: root.searchOpen ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.animMs } }
        GlassPanel {
            id: bar
            y: 86; x: 20; width: parent.width - 40; height: 54; radius: 27
            TextInput { id: input; anchors.fill: parent; anchors.leftMargin: 46; anchors.rightMargin: 20
                verticalAlignment: TextInput.AlignVCenter; color: Theme.text; font.pixelSize: 17; font.family: Theme.font
                text: root.query; onTextChanged: root.query = text; focus: root.searchOpen; clip: true
                Text { visible: !input.text.length; text: "Search apps"; color: Theme.textDim; font: input.font; anchors.verticalCenter: parent.verticalCenter } }
            Text { x: 18; anchors.verticalCenter: parent.verticalCenter; text: "⌕"; font.pixelSize: 22; color: Theme.textDim }
        }
        GlassPanel {
            x: 20; y: 156; width: parent.width - 40; height: Math.min(results.contentHeight + 16, 420); radius: 26
            ListView { id: results; anchors.fill: parent; anchors.margins: 8; clip: true
                model: Apps.count
                delegate: Item {
                    required property int index
                    readonly property var app: Apps.get(index)
                    readonly property bool allowed: root.core.concat(root.dockNames, root.externalOk).indexOf(app.name) >= 0   // phone-ready apps only
                    visible: allowed && (root.query.length === 0 || app.name.toLowerCase().indexOf(root.query.toLowerCase()) >= 0)
                    height: visible ? 50 : 0; width: results.width
                    Row { anchors.fill: parent; spacing: 12; anchors.leftMargin: 8
                        AppIcon { size: 34; showLabel: false; label: app.name; glass: false; anchors.verticalCenter: parent.verticalCenter }
                        Text { text: app.name; color: Theme.text; font.pixelSize: 16; anchors.verticalCenter: parent.verticalCenter; font.family: Theme.font } }
                    MouseArea { anchors.fill: parent; onClicked: { root.searchOpen = false; root.launchByName(app.name, Qt.rect(root.width / 2 - 30, root.height / 2 - 30, 60, 60)) } }
                }
            }
        }
        // tap outside closes
        MouseArea { anchors.fill: parent; z: -1; onClicked: root.searchOpen = false }
    }
    Rectangle { anchors.fill: parent; color: "black"; opacity: root.dim; z: 5; visible: opacity > 0.005 }
}
