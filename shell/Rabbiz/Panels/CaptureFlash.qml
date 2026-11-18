import Quickshell
import Quickshell.Wayland
import QtQuick
import Rabbiz.Services

// Camera-shutter flash: a full-screen white flash right after a screenshot is saved (so it is never
// part of the capture). Click-through; only exists while animating.
PanelWindow {
    id: flash

    anchors { top: true; bottom: true; left: true; right: true }
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "a21s-capture-flash"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    color: "transparent"
    mask: Region {}              // empty input region: touches fall through to the app below
    property bool active: false
    visible: active

    Connections {
        target: SystemControl
        function onScreenshotSerialChanged() { flash.active = true; shutter.restart() }
    }

    Rectangle {
        id: sheet
        anchors.fill: parent
        color: "#FFFFFF"
        opacity: 0
    }

    // "snap": near-instant rise, then a soft fade.
    SequentialAnimation {
        id: shutter
        onStopped: flash.active = false
        NumberAnimation { target: sheet; property: "opacity"; to: 0.92; duration: 45; easing.type: Easing.OutQuad }
        PauseAnimation { duration: 40 }
        NumberAnimation { target: sheet; property: "opacity"; to: 0; duration: 320; easing.type: Easing.InQuad }
    }
}
