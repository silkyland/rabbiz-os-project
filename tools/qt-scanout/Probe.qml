import QtQuick
import QtQuick.Effects

Window {
    id: window
    width: 720
    height: 1600
    visible: true
    visibility: Window.FullScreen
    title: "Rabbiz GPU blur probe"
    color: "#202838"
    property int frames: 0
    property bool animated: !Qt.application.arguments.includes("--static")
    property bool blurred: !Qt.application.arguments.includes("--sharp")
    Item {
        id: pattern
        anchors.fill: parent
        visible: false
        Repeater {
            model: 48
            Rectangle {
                required property int index
                x: (index % 6) * 120 + 15
                y: Math.floor(index / 6) * 200 + 55
                width: 90
                height: 90
                color: index % 2 ? "#ff8040" : "#40b0f0"
                NumberAnimation on rotation {
                    running: window.animated
                    from: 0; to: 360; duration: 2000; loops: Animation.Infinite
                }
            }
        }
    }
    MultiEffect {
        anchors.fill: pattern
        source: pattern
        blurEnabled: window.blurred
        blur: 1.0
        blurMax: 32
    }
    FrameAnimation { running: window.animated; onTriggered: ++window.frames }
    Timer {
        interval: 3000
        running: true
        onTriggered: window.contentItem.grabToImage(function(result) {
            const path = window.blurred ? "/tmp/a21s-blurred.png" : "/tmp/a21s-sharp.png"
            console.log("PROBE_CAPTURE=" + path + " saved=" + result.saveToFile(path))
        })
    }
    Timer {
        interval: 10000
        running: true
        onTriggered: { console.log("PROBE_FRAMES=" + window.frames + " duration_ms=10000"); Qt.quit() }
    }
}
