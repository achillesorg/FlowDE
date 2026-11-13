import QtQuick
import FlowShell

Item {
    id: root
    property bool searchOpen: false
    property string query: ""
    property real dim: 0
    readonly property int perPage: 20
    readonly property int pages: Math.max(1, Math.ceil(Apps.count / perPage))
    readonly property var dockNames: ["Phone", "Messages", "Browser", "Camera"]
    signal launched(string name)

    function launch(row, name) { Apps.launch(row); root.launched(name) }

    ListView {
        id: pager
        anchors { top: parent.top; topMargin: 74; left: parent.left; right: parent.right; bottom: dock.top; bottomMargin: 56 }
        orientation: ListView.Horizontal; snapMode: ListView.SnapOneItem; highlightRangeMode: ListView.StrictlyEnforceRange
        clip: false; model: root.pages; boundsBehavior: Flickable.StopAtBounds
        opacity: root.searchOpen ? 0.12 : 1; Behavior on opacity { NumberAnimation { duration: Theme.animMs } }
        scale: root.searchOpen ? 0.94 : 1; Behavior on scale { enabled: !Theme.reduceMotion; SpringAnimation { spring: 3; damping: 0.3; epsilon: 0.002 } }
        delegate: Item {
            id: page
            required property int index
            width: pager.width; height: pager.height
            Grid {
                anchors.horizontalCenter: parent.horizontalCenter
                columns: 4; rowSpacing: 14; columnSpacing: 0
                Repeater {
                    model: Math.min(root.perPage, Apps.count - page.index * root.perPage)
                    delegate: AppIcon {
                        required property int index
                        readonly property var app: Apps.get(page.index * root.perPage + index)
                        width: pager.width / 4; size: 60
                        label: app.name; icon: app.icon
                        onClicked: root.launch(page.index * root.perPage + index, app.name)
                    }
                }
            }
        }
        // pull down => search
        DragHandler { id: pull; target: null; yAxis.enabled: true; xAxis.enabled: false
            onActiveChanged: if (!active && translation.y > 70 && Math.abs(translation.x) < 60) root.searchOpen = true }
    }

    Row {
        anchors { horizontalCenter: parent.horizontalCenter; bottom: dock.top; bottomMargin: 20 }
        spacing: 8; visible: !root.searchOpen
        Repeater { model: root.pages
            Rectangle { width: 7; height: 7; radius: 4; color: Theme.text; opacity: index === pager.currentIndex ? 0.9 : 0.3 } }
    }

    GlassPanel {
        id: dock
        height: 92; radius: 34
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; leftMargin: 16; rightMargin: 16; bottomMargin: 38 }
        Row {
            anchors.centerIn: parent
            Repeater { model: root.dockNames
                AppIcon { required property string modelData; label: modelData; showLabel: false; size: 58; width: (dock.width - 16) / 4
                    onClicked: root.launched(modelData) } }
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
                    visible: root.query.length === 0 ? index < 6 : app.name.toLowerCase().indexOf(root.query.toLowerCase()) >= 0
                    height: visible ? 50 : 0; width: results.width
                    Row { anchors.fill: parent; spacing: 12; anchors.leftMargin: 8
                        AppIcon { size: 34; showLabel: false; label: app.name; anchors.verticalCenter: parent.verticalCenter }
                        Text { text: app.name; color: Theme.text; font.pixelSize: 16; anchors.verticalCenter: parent.verticalCenter; font.family: Theme.font } }
                    MouseArea { anchors.fill: parent; onClicked: { root.searchOpen = false; root.launch(index, app.name) } }
                }
            }
        }
        // tap outside closes
        MouseArea { anchors.fill: parent; z: -1; onClicked: root.searchOpen = false }
    }
    Rectangle { anchors.fill: parent; color: "black"; opacity: root.dim; z: 5 }
}
