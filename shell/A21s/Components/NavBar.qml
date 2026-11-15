import Quickshell
import Quickshell.Wayland
import QtQuick
import A21s.Theme
import A21s.Services

// Bottom gesture strip (Android-style nav pill zone).
//  - swipe up past the zone height and release quickly -> go home
//    (minimize the focused toplevel, emit ShellState.goHome)
//  - swipe up and hold ~Theme.animSlow before releasing    -> open TaskSwitcher
PanelWindow {
    id: bar

    // The task switcher is its own full-screen layer surface, instantiated
    // here so the gesture can drive it directly.
    TaskSwitcher {
        id: switcher
    }

    // Public handle: true while the recent-apps switcher is open.
    property alias switcherOpen: switcher.open

    // Go home: hide the switcher, clear the focused app id, minimize the
    // activated toplevel (compositor permitting), then notify the shell.
    function goHome() {
        bar.switcherOpen = false;
        ShellState.activeAppId = "";
        var tops = ToplevelManager.toplevels ? ToplevelManager.toplevels.values : [];
        for (var i = 0; i < tops.length; ++i) {
            if (tops[i] && tops[i].activated)
                tops[i].minimized = true;
        }
        ShellState.goHome();
    }

    // Cycle to previous/next open toplevel when swiping horizontally on the pill.
    function switchAdjacentApp(delta) {
        var tops = ToplevelManager.toplevels ? ToplevelManager.toplevels.values : [];
        if (tops.length < 2)
            return;
        var activeIdx = -1;
        for (var i = 0; i < tops.length; ++i) {
            if (tops[i] && tops[i].activated) {
                activeIdx = i;
                break;
            }
        }
        var nextIdx = activeIdx === -1
            ? 0
            : (activeIdx + delta + tops.length) % tops.length;
        var target = tops[nextIdx];
        if (target) {
            target.minimized = false;
            target.activate();
            ShellState.activeAppId = target.appId || "";
        }
    }

    anchors {
        bottom: true
        left: true
        right: true
    }
    implicitHeight: Theme.navZoneHeight
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Top
    color: "transparent"

    // Home indicator pill.
    Rectangle {
        // Cycle to previous/next open toplevel when swiping horizontally on the pill.
    function switchAdjacentApp(delta) {
        var tops = ToplevelManager.toplevels ? ToplevelManager.toplevels.values : [];
        if (tops.length < 2)
            return;
        var activeIdx = -1;
        for (var i = 0; i < tops.length; ++i) {
            if (tops[i] && tops[i].activated) {
                activeIdx = i;
                break;
            }
        }
        var nextIdx = activeIdx === -1
            ? 0
            : (activeIdx + delta + tops.length) % tops.length;
        var target = tops[nextIdx];
        if (target) {
            target.minimized = false;
            target.activate();
            ShellState.activeAppId = target.appId || "";
        }
    }

    anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: Theme.spaceS
        }
        width: Theme.navPillWidth
        height: Theme.navPillHeight
        radius: Theme.radiusPill
        color: gesture.pressed || bar.switcherOpen ? Theme.text : Theme.textFaint

        Behavior on color {
            ColorAnimation {
                duration: Theme.animFast
            }
        }
    }

    // Gesture zone.
    MouseArea {
        id: gesture
        anchors.fill: parent

        property real pressX: 0
        property real pressY: 0
        property real lastY: 0
        property bool held: false

        onPressed: (mouse) => {
            pressX = mouse.x;
            pressY = mouse.y;
            lastY = mouse.y;
            held = false;
            holdTimer.restart();
        }
        onPositionChanged: (mouse) => {
            lastY = mouse.y;
        }
        onReleased: (mouse) => {
            holdTimer.stop();
            var dy = pressY - mouse.y;
            var dx = mouse.x - pressX;
            // Short, quick flick up -> home. A held gesture already opened
            // the switcher and must not also trigger home.
            if (!held && !bar.switcherOpen) {
                if (dy > Theme.navZoneHeight) {
                    bar.goHome();
                } else if (Math.abs(dx) > Theme.navZoneHeight * 1.4 && Math.abs(dx) > Math.abs(dy) * 1.5) {
                    bar.switchAdjacentApp(dx > 0 ? 1 : -1);
                }
            }
            held = false;
        }
        onCanceled: {
            holdTimer.stop();
            held = false;
        }
    }

    // Hold ~animSlow while lifted past the zone height -> recent apps.
    Timer {
        id: holdTimer
        interval: Theme.animSlow
        onTriggered: {
            if (gesture.pressed && gesture.pressY - gesture.lastY > Theme.navZoneHeight) {
                gesture.held = true;
                bar.switcherOpen = true;
            }
        }
    }
}
