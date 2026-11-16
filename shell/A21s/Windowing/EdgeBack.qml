import Quickshell
import Quickshell.Wayland
import QtQuick
import A21s.Theme
import A21s.Services
import A21s.Widgets
import "Gesture.js" as G

// Android-style system back gesture: a thin invisible strip on each side edge, above every window
// (layer Overlay, so it also works over fullscreen apps such as Steam). Swipe inward past
// BACK_DIST -> ShellState.navAction("back").
//
// The round arrow chip lives on its own wide surface (a 22 px strip cannot show it), which is
// mapped ONLY while a back gesture is in flight: the pointer is already grabbed by the strip,
// so the chip can never steal a fresh touch from the app below.
Scope {
    id: root
    property int stripWidth: 22

    component Edge: PanelWindow {
        id: win
        property string side: "left"
        anchors { top: true; bottom: true; left: side === "left"; right: side === "right" }
        // leave the bottom gesture zone and the status area to the other surfaces
        margins { top: 80; bottom: 80 }
        implicitWidth: root.stripWidth
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "a21s-edge-" + side
        color: "transparent"

        MouseArea {
            anchors.fill: parent
            preventStealing: true
            property real startX: 0
            onPressed: (m) => {
                startX = m.x
                ShellState.gestureKind = "back"
                ShellState.gestureSide = win.side
                ShellState.gestureProgress = 0
                ShellState.gestureY = win.margins.top + m.y
            }
            onPositionChanged: (m) => {
                if (!pressed) return
                ShellState.gestureProgress = G.edgeProgress(win.side, m.x - startX)
                ShellState.gestureY = win.margins.top + m.y
            }
            onReleased: (m) => {
                var act = G.endEdge(win.side, m.x - startX)
                ShellState.gestureProgress = 0; ShellState.gestureKind = ""
                if (act === "back") ShellState.navAction("back")
            }
            onCanceled: { ShellState.gestureProgress = 0; ShellState.gestureKind = "" }
        }
    }

    Edge { side: "left" }
    Edge { side: "right" }

    // Display-only chip: full-height surface, visible only during a back gesture.
    component Chip: PanelWindow {
        id: chip
        property string side: "left"
        anchors { top: true; bottom: true; left: side === "left"; right: side === "right" }
        implicitWidth: 150
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "a21s-backchip-" + side
        color: "transparent"
        visible: ShellState.gestureKind === "back" && ShellState.gestureSide === side
                 && ShellState.gestureProgress > 0.02

        readonly property real travel: 70 * ShellState.gestureProgress
        readonly property real cx: side === "left" ? 14 + travel : width - 14 - travel

        Rectangle {
            width: 46; height: 46
            radius: 23
            x: chip.cx - width / 2
            y: ShellState.gestureY - height / 2

            color: ShellState.gestureProgress >= 1.0
                   ? ShellPrefs.accentColor : Qt.alpha(Theme.surfaceHigh, 0.92)
            border.width: 1
            border.color: Qt.alpha(ShellPrefs.accentColor, 0.5)

            MaterialIcon {
                anchors.centerIn: parent
                name: chip.side === "left" ? "arrow_back" : "arrow_forward"
                size: 24
                color: ShellState.gestureProgress >= 1.0 ? ShellPrefs.onAccentColor : Theme.text
            }
        }
    }

    Chip { side: "left" }
    Chip { side: "right" }
}
