import Quickshell
import Quickshell.Wayland
import QtQuick
import A21s.Theme
import A21s.Services
import A21s.Widgets
import "Gesture.js" as G

// Android-style system back gesture: a thin invisible strip on each side edge, above every window
// (layer Overlay, so it also works over fullscreen apps such as Steam). Swipe inward past
// BACK_DIST -> ShellState.navAction("back"). A round chip with an arrow follows the finger.
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
            onPressed: (m) => { startX = m.x; ShellState.gestureKind = "back"; ShellState.gestureProgress = 0 }
            onPositionChanged: (m) => { if (pressed) ShellState.gestureProgress = G.edgeProgress(win.side, m.x - startX) }
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
}
