import Quickshell
import Quickshell.Wayland
import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services

// Android-style bottom navigation bar:
//   - Left button:  Back (closes switcher/shade/drawer, or closes focused app)
//   - Center pill:  Tap OR swipe up -> Home (minimizes all open windows)
//                   Swipe up & hold -> Recent apps (TaskSwitcher)
//                   Swipe left/right -> Switch adjacent open app
//   - Right button: Recents (toggles TaskSwitcher)
PanelWindow {
    id: bar

    TaskSwitcher {
        id: switcher
    }

    property alias switcherOpen: switcher.open

    // Go home: close overlays/drawers, minimize ALL open app toplevels so
    // the HomeScreen beneath is always revealed, and trigger reverse morph.
    function goHome() {
        var hadOpenApp = ShellState.activeAppId.length > 0;
        var tops = ToplevelManager.toplevels ? ToplevelManager.toplevels.values : [];
        for (var i = 0; i < tops.length; ++i) {
            if (tops[i]) {
                if (!tops[i].minimized)
                    hadOpenApp = true;
                tops[i].minimized = true;
            }
        }
        bar.switcherOpen = false;
        ShellState.drawerOpen = false;
        ShellState.shadeProgress = 0;
        ShellState.powerMenuOpen = false;
        ShellState.activeAppId = "";
        if (hadOpenApp)
            ShellState.triggerCloseMorph();
        ShellState.goHome();
        bar.expanded = false;
    }

    // Back action: dismiss shell overlays first; otherwise close the active
    // (or top non-minimized) app toplevel.
    function goBack() {
        if (bar.switcherOpen) {
            bar.switcherOpen = false;
            return;
        }
        if (ShellState.shadeProgress > 0) {
            ShellState.shadeProgress = 0;
            return;
        }
        if (ShellState.drawerOpen) {
            ShellState.drawerOpen = false;
            return;
        }
        var tops = ToplevelManager.toplevels ? ToplevelManager.toplevels.values : [];
        var target = null;
        for (var i = 0; i < tops.length; ++i) {
            var t = tops[i];
            if (!t)
                continue;
            if (t.activated) {
                target = t;
                break;
            }
            if (!t.minimized && target === null)
                target = t;
        }
        if (target) {
            target.close();
            bar.expanded = false;
        }
    }

    function toggleRecents() {
        if (ShellState.shadeProgress > 0)
            ShellState.shadeProgress = 0;
        if (ShellState.drawerOpen)
            ShellState.drawerOpen = false;
        bar.switcherOpen = !bar.switcherOpen;
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
        collapseTimer.restart();
    }

    // Immersive mode: the button row stays hidden behind a thin gesture
    // pill until the user taps or swipes up inside the bottom zone, then
    // auto-collapses again after a few seconds. While an app is open the
    // collapsed surface shrinks to a 24px gesture strip (matching labwc's
    // bottom <margin>) so apps get almost the whole screen.
    property bool expanded: false

    function showTemporarily() {
        bar.expanded = true;
        collapseTimer.restart();
    }

    onSwitcherOpenChanged: {
        if (bar.switcherOpen)
            bar.expanded = true;
        collapseTimer.restart();
    }

    // Opening an app always drops the bar back to the slim gesture strip.
    Connections {
        target: ShellState
        function onAppOpenChanged() {
            if (ShellState.appOpen)
                bar.expanded = false;
        }
    }

    anchors {
        bottom: true
        left: true
        right: true
    }
    // Collapsed + app open -> 24px gesture strip only; otherwise the full
    // 56px row (expanded or on the home screen, where the strip is free).
    implicitHeight: (ShellState.appOpen && !bar.expanded) ? 24 : 56
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Top
    color: "transparent"

    // Collapsed gesture zone: tap or swipe up reveals the button row.
    // Covers the whole bar but only exists while collapsed.
    MouseArea {
        anchors.fill: parent
        enabled: !bar.expanded
        preventStealing: true
        property real pressY: 0
        onPressed: (mouse) => { pressY = mouse.y; }
        onReleased: bar.showTemporarily()
    }

    // Slim gesture hint, always rendered (brighter when expanded).
    Rectangle {
        visible: !bar.expanded
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: ShellState.appOpen ? 9 : 10
        }
        width: 140
        height: 6
        radius: Theme.radiusPill
        color: Qt.alpha(Theme.text, 0.45)
    }

    Timer {
        id: collapseTimer
        interval: 5000
        onTriggered: bar.expanded = false
    }

    Rectangle {
        anchors.fill: parent
        visible: bar.expanded
        color: Qt.alpha(Theme.bg, 0.82)

        // Subtle top hairline divider
        Rectangle {
            anchors {
                top: parent.top
                left: parent.left
                right: parent.right
            }
            height: 1
            color: Qt.alpha(Theme.text, 0.08)
        }

        RowLayout {
            anchors.fill: parent
            spacing: 0

            // Left: Android Back button
            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true

                Rectangle {
                    anchors.centerIn: parent
                    width: 84
                    height: 40
                    radius: Theme.radiusPill
                    color: backTap.pressed ? Qt.alpha(Theme.text, 0.14) : "transparent"

                    MaterialIcon {
                        anchors.centerIn: parent
                        name: "arrow_back"
                        size: 28
                        color: backTap.pressed ? Theme.text : Theme.textDim
                    }
                }

                TapHandler {
                    id: backTap
                    onTapped: bar.goBack()
                }
            }

            // Center: Home pill (tap OR swipe up -> Home; swipe up & hold -> Recents;
            // swipe left/right -> adjacent open app)
            Item {
                Layout.preferredWidth: Theme.navPillWidth + 64
                Layout.fillHeight: true

                Rectangle {
                    id: homePill
                    anchors.centerIn: parent
                    width: Theme.navPillWidth
                    height: 10
                    radius: Theme.radiusPill
                    color: gesture.pressed || bar.switcherOpen
                        ? ShellPrefs.accentColor
                        : Qt.alpha(Theme.text, 0.72)
                    scale: gesture.pressed ? 0.94 : 1.0

                    Behavior on color { ColorAnimation { duration: Theme.animFast } }
                    Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }
                }

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
                        if (!held && !bar.switcherOpen) {
                            if (dy > 18) {
                                bar.goHome();
                            } else if (Math.abs(dx) > 48 && Math.abs(dx) > Math.abs(dy) * 1.4) {
                                bar.switchAdjacentApp(dx > 0 ? 1 : -1);
                            } else if (Math.abs(dx) < 16 && Math.abs(dy) < 16) {
                                bar.goHome();
                            }
                        }
                        held = false;
                    }
                    onCanceled: {
                        holdTimer.stop();
                        held = false;
                    }
                }
            }

            // Right: Android Recents button
            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true

                Rectangle {
                    anchors.centerIn: parent
                    width: 84
                    height: 40
                    radius: Theme.radiusPill
                    color: recentsTap.pressed || bar.switcherOpen
                        ? Qt.alpha(Theme.text, 0.14)
                        : "transparent"

                    Rectangle {
                        anchors.centerIn: parent
                        width: 20
                        height: 20
                        radius: 5
                        color: "transparent"
                        border.width: 2.5
                        border.color: recentsTap.pressed || bar.switcherOpen
                            ? ShellPrefs.accentColor
                            : Theme.textDim
                    }
                }

                TapHandler {
                    id: recentsTap
                    onTapped: bar.toggleRecents()
                }
            }
        }
    }

    // Hold while swiping up -> Recent apps switcher
    Timer {
        id: holdTimer
        interval: Theme.animSlow
        onTriggered: {
            if (gesture.pressed && gesture.pressY - gesture.lastY > 18) {
                gesture.held = true;
                bar.switcherOpen = true;
            }
        }
    }
}
