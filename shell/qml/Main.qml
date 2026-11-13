import QtQuick
import QtQuick.Window
import FlowShell

Window {
    id: win
    visible: true
    width: 540; height: 1170
    color: "black"
    title: "FlowDE Shell"

    // "lock" | "home" | "switcher" | "app"
    property var openApps: []
    property string appName: ""
    property rect appRect: Qt.rect(240, 560, 62, 62)
    property bool appLaunching: false
    property bool appDrag: false
    property real appP: 0
    // response ~0.4 s, damping ratio ~0.85 (see docs/mobile-motion.md)
    Behavior on appP { enabled: !win.appDrag && !Theme.reduceMotion; SpringAnimation { spring: 4.2; damping: 0.38; epsilon: 0.001; mass: 1 } }
    // Starts on touch-up, never waits for the app: one progress value (appP) drives the shared element.
    function openApp(name, rect, launching) {
        if (openApps.indexOf(name) < 0) openApps = openApps.concat([name])
        appName = name; appRect = rect; appLaunching = !!launching; shadeP = 0; page = "app"
        if (launching) launchTimer.restart()
    }
    Timer { id: launchTimer; interval: 1600; onTriggered: if (win.appLaunching) win.gesture("home") }
    property string page: StartScreen === "shade" || StartScreen === "search" ? "home" : StartScreen
    property real lockP: StartScreen === "lock" ? 0 : 1
    property real shadeP: StartScreen === "shade" ? 1 : 0
    property real switchP: StartScreen === "switcher" ? 1 : 0
    onPageChanged: { lockP = page === "lock" ? 0 : 1; switchP = page === "switcher" ? 1 : 0; appP = page === "app" ? 1 : 0 }

    Behavior on lockP { enabled: !lockDrag.active; SpringAnimation { spring: 3.2; damping: 0.32; epsilon: 0.002; mass: 1; modulus: 0 } }
    Behavior on shadeP { enabled: !shadeDrag.active; SpringAnimation { spring: 3.4; damping: 0.3; epsilon: 0.002 } }
    Behavior on switchP { SpringAnimation { spring: 3.2; damping: 0.32; epsilon: 0.002 } }

    Component.onCompleted: {
        Theme.select(StartTheme)
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
            if (k === "theme") Theme.select(v)
            else if (k === "scheme") Theme.dark = (v === "dark")
            else if (k === "open") win.openApp(v, Qt.rect(win.width / 2 - 31, win.height * 0.2, 62, 62), false)
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
            else if (page === "switcher" || page === "app") page = "home"
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
            Connections { target: Theme; function onWallpaperSourceChanged() { Qt.callLater(bsrc.scheduleUpdate) } function onNameChanged() { Qt.callLater(bsrc.scheduleUpdate) } function onDarkChanged() { Qt.callLater(bsrc.scheduleUpdate) } }
            Timer { interval: 400; running: true; onTriggered: bsrc.scheduleUpdate() }
        }

        HomeScreen {
            id: home
            anchors.fill: parent
            anchors.topMargin: 0
            scale: (0.96 + 0.04 * win.lockP * (1 - win.switchP * 0.6)) * (1 - 0.06 * win.appP)
            opacity: win.page === "switcher" ? 0.0 : win.lockP
            dim: 0.28 * win.appP
            onLaunched: (n, r, real) => win.openApp(n, r, real)
        }
        AppSwitcher {
            anchors.fill: parent
            opacity: win.switchP
            scale: 0.9 + 0.1 * win.switchP
            running: win.openApps
            onPicked: (n) => win.openApp(n, Qt.rect(win.width / 2 - 31, win.height / 2 - 31, 62, 62), false)
            onClosedAll: { win.openApps = []; win.page = "home" }
            onClosedOne: (n) => win.openApps = win.openApps.filter(x => x !== n)
        }
        AppPage { id: appPage; anchors.fill: parent; name: win.appName; from: win.appRect; p: win.appP; sw: win.width; sh: win.height; launching: win.appLaunching; z: 15; onBack: win.gesture("home") }
        LockScreen { id: lock; anchors.fill: parent; progress: win.lockP }

        // lock: swipe up
        DragHandler {
            id: lockDrag
            enabled: win.page === "lock"
            target: null; xAxis.enabled: false
            onTranslationChanged: if (active) win.lockP = Math.max(0, Math.min(1, -translation.y / (win.height * 0.5)))
            onActiveChanged: if (!active) win.page = (win.lockP > 0.35 ? "home" : "lock")
        }

        StatusBar { id: sbar; anchors { top: parent.top; left: parent.left; right: parent.right } z: 20; ink: win.page === "app" ? Theme.text : Theme.onWall }
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
        StatusBar { anchors { top: parent.top; left: parent.left; right: parent.right } z: 40; visible: win.shadeP > 0.01; ink: Theme.text }
        GestureBar {
            z: 50; anchors { bottom: parent.bottom; left: parent.left; right: parent.right }
            ink: win.page === "app" ? Theme.text : Theme.onWall
            onDragged: (dy) => { if (win.page === "app") { win.appDrag = true; win.appP = Math.max(0.15, Math.min(1, 1 - dy / (win.height * 0.6))) } }
            onReleased: (dy, vy) => {
                if (win.page === "app") {
                    win.appDrag = false
                    if (dy > 70 || vy < -900) win.gesture("home"); else win.appP = 1
                } else if (dy > 60) { (vy < -900) ? win.gesture("home") : win.gesture("switcher") }
            }
        }
    }
}
