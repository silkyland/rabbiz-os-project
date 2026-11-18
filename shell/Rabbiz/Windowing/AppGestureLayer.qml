import QtQuick
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets

// Base Android-style edge-back gesture overlay for all rabbiz-shell apps.
// Uses thin left/right edge strips (edgeWidth px) so center ListViews,
// Flickables, and sliders are never blocked.
Item {
    id: root

    property int edgeWidth: 36
    property int triggerDistance: 72
    property bool enabled: true

    signal backTriggered()

    // Internal drag state
    property bool _dragging: false
    property bool _fromLeft: true
    property real _progress: 0.0   // 0..1

    readonly property bool _readyToTrigger: _progress >= 1.0

    // Left edge strip
    MouseArea {
        anchors {
            left: parent.left
            top: parent.top
            bottom: parent.bottom
        }
        width: root.edgeWidth
        enabled: root.enabled
        preventStealing: true

        property real startX: 0

        onPressed: (mouse) => {
            startX = mouse.x
            root._fromLeft = true
            root._dragging = true
            root._progress = 0.0
        }
        onPositionChanged: (mouse) => {
            if (!pressed) return
            var dx = Math.max(0, mouse.x - startX)
            root._progress = Math.min(1.0, dx / root.triggerDistance)
        }
        onReleased: {
            if (root._readyToTrigger)
                root.backTriggered()
            root._dragging = false
            root._progress = 0.0
        }
        onCanceled: {
            root._dragging = false
            root._progress = 0.0
        }
    }

    // Right edge strip
    MouseArea {
        anchors {
            right: parent.right
            top: parent.top
            bottom: parent.bottom
        }
        width: root.edgeWidth
        enabled: root.enabled
        preventStealing: true

        property real startX: 0

        onPressed: (mouse) => {
            startX = mouse.x
            root._fromLeft = false
            root._dragging = true
            root._progress = 0.0
        }
        onPositionChanged: (mouse) => {
            if (!pressed) return
            var dx = Math.max(0, startX - mouse.x)
            root._progress = Math.min(1.0, dx / root.triggerDistance)
        }
        onReleased: {
            if (root._readyToTrigger)
                root.backTriggered()
            root._dragging = false
            root._progress = 0.0
        }
        onCanceled: {
            root._dragging = false
            root._progress = 0.0
        }
    }

    // Predictive-back pill indicator
    Rectangle {
        id: backPill
        visible: root._dragging && root._progress > 0.05
        width: 52 + root._progress * 16
        height: 52
        radius: 26
        y: (root.height - height) / 2
        x: root._fromLeft
            ? (-width * 0.4 + root._progress * 36)
            : (root.width - width * 0.6 - root._progress * 36)

        color: root._readyToTrigger ? ShellPrefs.accentColor : Theme.surfaceHigh
        border.width: 1
        border.color: Qt.alpha(Theme.text, 0.16)
        opacity: Math.min(1.0, root._progress * 1.4)

        Behavior on color { ColorAnimation { duration: Theme.animFast } }

        MaterialIcon {
            anchors.centerIn: parent
            name: root._fromLeft ? "arrow_back" : "arrow_forward"
            size: Theme.iconS - 4
            color: root._readyToTrigger ? Theme.primaryContent : Theme.text
        }
    }
}
