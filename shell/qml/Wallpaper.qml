import QtQuick
import FlowShell

Item {
    id: root
    // procedural wallpaper (no assets); animated phase is slow and off when reduce-motion
    property real phase: 0.6
    NumberAnimation on phase { from: 0.6; to: 6.88; duration: 90000; loops: Animation.Infinite; running: !Theme.reduceMotion && Theme.wallpaperSource === "" && root.visible }
    Image {
        anchors.fill: parent; visible: Theme.wallpaperSource !== ""
        source: Theme.wallpaperSource; fillMode: Image.PreserveAspectCrop; asynchronous: false; smooth: true
        sourceSize: Qt.size(root.width * 2, root.height * 2)
    }
    ShaderEffect {
        anchors.fill: parent; visible: Theme.wallpaperSource === ""
        property real aspect: width / Math.max(1, height)
        property real phase: root.phase
        property color c0: Theme.wall0
        property color c1: Theme.wall1
        property color c2: Theme.wall2
        property color c3: Theme.wall3
        fragmentShader: "qrc:/FlowShell/shaders/wallpaper.frag.qsb"
    }
}
