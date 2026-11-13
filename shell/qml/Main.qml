import QtQuick
import QtQuick.Window
import FlowShell

Window {
    id: win
    visible: true
    width: 540; height: 1170
    color: "black"
    title: "FlowDE Shell"

    // "lock" | "home" | "switcher"
    property string page: StartScreen === "shade" || StartScreen === "search" ? "home" : StartScreen
    property real lockP: StartScreen === "lock" ? 0 : 1
    property real shadeP: StartScreen === "shade" ? 1 : 0
    property real switchP: StartScreen === "switcher" ? 1 : 0
    onPageChanged: { lockP = page === "lock" ? 0 : 1; switchP = page === "switcher" ? 1 : 0 }

    Behavior on lockP { enabled: !lockDrag.active; SpringAnimation { spring: 3.2; damping: 0.32; epsilon: 0.002; mass: 1; modulus: 0 } }
    Behavior on shadeP { enabled: !shadeDrag.active; SpringAnimation { spring: 3.4; damping: 0.3; epsilon: 0.002 } }
    Behavior on switchP { SpringAnimation { spring: 3.2; damping: 0.32; epsilon: 0.002 } }

    Component.onCompleted: {
        Theme.name = StartTheme
        Theme.dark = StartDark
        Theme.backdrop = bsrc
        Theme.backdropItem = wall
        Theme.glass = Services.glassQuality
        Theme.reduceTransparency = Services.reduceTransparency
        Theme.reduceMotion = Services.reduceMotion
        if (StartScreen === "search") home.searchOpen = true
    }
    Connections {
        target: Services
        function onChanged() {
            Theme.glass = Services.glassQuality
            Theme.reduceTransparency = Services.reduceTransparency
            Theme.reduceMotion = Services.reduceMotion
        }
        function onGesture(name) { win.gesture(name) }
        function onCommand(k, v) {
            if (k === "theme") Theme.name = v
            else if (k === "scheme") Theme.dark = (v === "dark")
            else if (k === "screen") {
                home.searchOpen = false
                if (v === "shade") win.shadeP = 1
                else if (v === "search") { win.page = "home"; win.shadeP = 0; home.searchOpen = true }
                else { win.shadeP = 0; win.page = v }
            }
        }
    }

    function gesture(name) {
        if (name === "home") { shadeP = 0; home.searchOpen = false; page = "home" }
        else if (name === "switcher") { shadeP = 0; page = "switcher" }
        else if (name === "shade" || name === "qs") shadeP = 1
        else if (name === "back") {
            if (shadeP > 0.5) shadeP = 0
            else if (home.searchOpen) home.searchOpen = false
            else if (page === "switcher") page = "home"
        }
    }

    Item {
        id: root
        anchors.fill: parent
        Wallpaper { id: wall; anchors.fill: parent }
        // one shared, quarter-resolution snapshot of the wallpaper feeds every glass panel
        ShaderEffectSource {
            id: bsrc
            sourceItem: wall; live: false; visible: false
            width: root.width; height: root.height
            textureSize: Qt.size(Math.max(1, root.width / 4), Math.max(1, root.height / 4))
            smooth: true
            Timer { interval: 1500; running: !Theme.reduceMotion && Theme.quality < 2; repeat: true; onTriggered: bsrc.scheduleUpdate() }
            Connections { target: Theme; function onWall0Changed() { bsrc.scheduleUpdate() } function onNameChanged() { bsrc.scheduleUpdate() } }
        }

        HomeScreen {
            id: home
            anchors.fill: parent
            anchors.topMargin: 0
            scale: 0.96 + 0.04 * win.lockP * (1 - win.switchP * 0.6)
            opacity: win.page === "switcher" ? 0.0 : win.lockP
            dim: 0.0
            onLaunched: (n) => { win.page = "switcher"; }  // no real foreground app yet: show it in the switcher
        }
        AppSwitcher {
            anchors.fill: parent
            opacity: win.switchP
            scale: 0.9 + 0.1 * win.switchP
            onPicked: (n) => win.page = "home"
            onClosedAll: win.page = "home"
        }
        LockScreen { id: lock; anchors.fill: parent; progress: win.lockP }

        // lock: swipe up
        DragHandler {
            id: lockDrag
            enabled: win.page === "lock"
            target: null; xAxis.enabled: false
            onTranslationChanged: if (active) win.lockP = Math.max(0, Math.min(1, -translation.y / (win.height * 0.5)))
            onActiveChanged: if (!active) win.page = (win.lockP > 0.35 ? "home" : "lock")
        }

        StatusBar { id: sbar; anchors { top: parent.top; left: parent.left; right: parent.right } z: 20 }
        // top-edge pull for shade
        Item {
            z: 21; anchors { top: parent.top; left: parent.left; right: parent.right } height: 44
            DragHandler {
                id: shadeDrag
                target: null; xAxis.enabled: false
                onTranslationChanged: if (active) win.shadeP = Math.max(0, Math.min(1, translation.y / (win.height * 0.4)))
                onActiveChanged: if (!active) win.shadeP = win.shadeP > 0.3 ? 1 : 0
            }
        }
        Shade {
            id: shade; z: 30
            width: parent.width; height: parent.height
            progress: win.shadeP
            // swipe up on shade closes it
            DragHandler {
                target: null; xAxis.enabled: false
                onTranslationChanged: if (active) win.shadeP = Math.max(0, Math.min(1, 1 + translation.y / (win.height * 0.4)))
                onActiveChanged: if (!active) win.shadeP = win.shadeP > 0.7 ? 1 : 0
            }
        }
        // status bar above shade too
        StatusBar { anchors { top: parent.top; left: parent.left; right: parent.right } z: 40; visible: win.shadeP > 0.01 }
        GestureBar {
            z: 50; anchors { bottom: parent.bottom; left: parent.left; right: parent.right }
            onHome: win.gesture("home")
            onSwitcher: win.gesture("switcher")
        }
    }
}
