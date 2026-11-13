import QtQuick
import FlowShell

Item {
    id: root
    // procedural wallpaper (no assets); animated phase is slow and off when reduce-motion
    property real phase: 0.6
    NumberAnimation on phase { from: 0.6; to: 6.88; duration: 90000; loops: Animation.Infinite; running: !Theme.reduceMotion && root.visible }
    ShaderEffect {
        anchors.fill: parent
        property real aspect: width / Math.max(1, height)
        property real phase: root.phase
        property color c0: Theme.wall0
        property color c1: Theme.wall1
        property color c2: Theme.wall2
        property color c3: Theme.wall3
        fragmentShader: "qrc:/FlowShell/shaders/wallpaper.frag.qsb"
    }
}
