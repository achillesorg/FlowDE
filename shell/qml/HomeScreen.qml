import QtQuick
import QtQuick.Layouts
import FlowShell

Item {
    id: root
    property bool searchOpen: false
    property string query: ""
    property real dim: 0
    // curated home: 12 core apps + 4 dock apps (short names). Real .desktop apps stay reachable via search.
    readonly property var core: ["Photos", "Calendar", "Clock", "Music", "Maps", "Notes", "Files", "Weather", "Terminal", "Noxal", "Wallet", "Settings"]
    readonly property var dockNames: ["Phone", "Messages", "Browser", "Camera"]
    // Glaze Mobile: flat, responsive live-tile home. All existing app launch routes remain intact.
    readonly property var metroTiles: [
        {name:"Phone", icon:"phone", span:2, accent:true},
        {name:"Messages", icon:"chat", span:2, accent:false},
        {name:"Photos", icon:"photos", span:2, accent:false},
        {name:"Calendar", icon:"calendar", span:2, accent:true},
        {name:"Music", icon:"music", span:1, accent:false},
        {name:"Clock", icon:"clock", span:1, accent:false},
        {name:"Weather", icon:"weather", span:2, accent:false},
        {name:"Camera", icon:"camera", span:2, accent:true},
        {name:"Maps", icon:"maps", span:1, accent:false},
        {name:"Notes", icon:"notes", span:1, accent:false},
        {name:"Files", icon:"folder", span:2, accent:false},
        {name:"Settings", icon:"settings", span:2, accent:false},
        {name:"Browser", icon:"browser", span:2, accent:true},
        {name:"Noxal", icon:"noxal", span:1, accent:false},
        {name:"Wallet", icon:"wallet", span:1, accent:false},
        {name:"Terminal", icon:"terminal", span:2, accent:false}
    ]
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

    Flickable {
        id: metroHome
        visible: Theme.name === "GlazeMobile"
        anchors { fill: parent; leftMargin: 20; rightMargin: 20; topMargin: 60; bottomMargin: 44 }
        contentWidth: width
        contentHeight: metroColumn.implicitHeight + 16
        clip: true
        interactive: !root.searchOpen
        opacity: root.searchOpen ? 0.1 : 1
        Behavior on opacity { NumberAnimation { duration: Theme.animMs } }
        Column {
            id: metroColumn
            width: metroHome.width
            spacing: 16
            Text { text: "GLAZE MOBILE"; color: Theme.accent; font.family: Theme.font; font.pixelSize: 14; font.weight: Font.Bold; font.letterSpacing: 2 }
            Text { text: "start"; color: Theme.text; font.family: Theme.font; font.pixelSize: 58; font.weight: Font.Light }
            Text { text: Qt.formatDate(new Date(), "dddd, d MMMM"); color: Theme.textDim; font.family: Theme.font; font.pixelSize: 15 }
            Item { width: 1; height: 3 }
            GridLayout {
                id: metroGrid
                width: parent.width
                columns: 4
                columnSpacing: 8
                rowSpacing: 8
                Repeater {
                    model: root.metroTiles
                    Rectangle {
                        id: metroTile
                        required property var modelData
                        Layout.columnSpan: modelData.span
                        Layout.preferredWidth: (metroGrid.width - 24) * modelData.span / 4 + 8 * (modelData.span - 1)
                        Layout.preferredHeight: 105
                        radius: 2
                        color: modelData.accent ? Theme.accent : Theme.card
                        scale: metroTap.pressed ? 0.965 : 1
                        Behavior on scale { enabled: !Theme.reduceMotion; NumberAnimation { duration: 130; easing.type: Easing.OutCubic } }
                        Image {
                            x: 13; y: 13; width: 34; height: 34
                            source: "qrc:/FlowShell/assets/icons/" + modelData.icon + "-" + (modelData.accent ? "dark" : Theme.glyphInk) + ".svg"
                            sourceSize: Qt.size(68, 68)
                            fillMode: Image.PreserveAspectFit
                        }
                        Text {
                            x: 13; y: parent.height - height - 10
                            width: parent.width - 26
                            text: modelData.name
                            elide: Text.ElideRight
                            color: modelData.accent ? "#1c1b19" : Theme.text
                            font.family: Theme.font; font.pixelSize: 15; font.weight: Font.Medium
                        }
                        MouseArea {
                            id: metroTap
                            anchors.fill: parent
                            onClicked: {
                                const pos = metroTile.mapToItem(root, 0, 0)
                                root.launchByName(modelData.name, Qt.rect(pos.x, pos.y, metroTile.width, metroTile.height))
                            }
                        }
                    }
                }
            }
            Text {
                text: "all apps  →"; color: Theme.textDim; font.family: Theme.font
                font.pixelSize: 17; topPadding: 12; bottomPadding: 24
                MouseArea { anchors.fill: parent; onClicked: root.searchOpen = true }
            }
        }
    }

    // Existing home layout stays selectable with the other themes.
    // today card (flat paper)
    GlassPanel {
        id: today
        visible: Theme.name !== "GlazeMobile"
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
        visible: Theme.name !== "GlazeMobile"
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
        visible: Theme.name !== "GlazeMobile"
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
