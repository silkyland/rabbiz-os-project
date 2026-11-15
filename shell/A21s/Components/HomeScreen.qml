import Quickshell
import Quickshell.Wayland
import QtQuick
import A21s.Theme
import A21s.Services

// Home screen: flat wallpaper + low-opacity decorative circles (no shaders),
// clock widget, bottom dock of pinned apps, and the slide-up AppDrawer.
// Lives on WlrLayer.Background so any app toplevel naturally covers it.
PanelWindow {
    id: home

    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Background
    // Only take keyboard focus (for the drawer search field) while the
    // drawer is open; otherwise never steal input from apps.
    WlrLayershell.keyboardFocus: ShellState.drawerOpen
        ? WlrKeyboardFocus.OnDemand
        : WlrKeyboardFocus.None
    color: Theme.bg

    SystemClock {
        id: clock
        precision: SystemClock.Minutes
    }

    // ---- Wallpaper decoration: two soft low-opacity circles. ----
    Rectangle {
        width: home.width
        height: home.width
        radius: width / 2
        color: Theme.primary
        opacity: 0.05
        x: home.width * 0.55
        y: -home.height * 0.3
    }
    Rectangle {
        width: home.height
        height: home.height
        radius: width / 2
        color: Theme.accent
        opacity: 0.05
        x: -home.width * 0.4
        y: home.height * 0.55
    }

    // ---- Gesture layer: swipe up on empty space opens the app drawer. ----
    // Sits below the interactive icons, so it only sees touches that miss them.
    MouseArea {
        id: homeSwipe
        anchors.fill: parent
        property real pressY: 0

        onPressed: (mouse) => { pressY = mouse.y; }
        onReleased: (mouse) => {
            if (pressY - mouse.y > Theme.navZoneHeight && !ShellState.drawerOpen)
                ShellState.drawerOpen = true;
        }
    }

    // ---- Clock + date (Android-style, upper third). ----
    // Positioners ignore anchors on children, so the two labels are anchored
    // directly instead of using a Column.
    Text {
        id: clockTime
        anchors {
            horizontalCenter: parent.horizontalCenter
            top: parent.top
            topMargin: parent.height * 0.16
        }
        text: Qt.formatDateTime(clock.date, "hh:mm")
        color: Theme.text
        font.pixelSize: Theme.fontHeadline * 3
        font.family: Theme.fontFamily
    }
    Text {
        anchors {
            horizontalCenter: parent.horizontalCenter
            top: clockTime.bottom
            topMargin: Theme.spaceXS
        }
        text: Qt.formatDateTime(clock.date, "EEEE, MMMM d")
        color: Theme.textDim
        font.pixelSize: Theme.fontBody
        font.family: Theme.fontFamily
    }

    // ---- Dock: pinned apps resolved from DesktopEntries. ----
    property var dockApps: []

    function buildDock() {
        dockApps = [
            DesktopEntries.byId("a21s-dialer"),
            DesktopEntries.byId("a21s-sms"),
            DesktopEntries.byId("a21s-settings"),
            // Placeholder until a real browser desktop entry ships.
            ({ name: "Browser", icon: "web-browser" })
        ];
    }

    Component.onCompleted: buildDock()

    Connections {
        target: DesktopEntries
        function onApplicationsChanged() { home.buildDock(); }
    }

    // Dock row — no labels, Android style.
    Row {
        id: dockRow
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: Theme.navZoneHeight + Theme.spaceS
        }
        spacing: Theme.spaceL

        Repeater {
            model: home.dockApps
            delegate: AppIcon {
                required property var modelData
                width: Theme.iconApp + Theme.spaceS
                height: Theme.iconApp + Theme.spaceXS
                entry: modelData
                labelVisible: false
            }
        }
    }

    // "All apps" handle above the dock — tap opens the app drawer.
    Rectangle {
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: dockRow.top
            bottomMargin: Theme.spaceM
        }
        width: Theme.navPillWidth
        height: Theme.iconS
        radius: Theme.radiusPill
        color: Theme.surfaceHigh

        Text {
            anchors.centerIn: parent
            text: "Apps"
            color: Theme.textDim
            font.pixelSize: Theme.fontCaption
            font.family: Theme.fontFamily
        }
        TapHandler {
            onTapped: ShellState.drawerOpen = true;
        }
    }

    // ---- App drawer + shell-state wiring. ----
    AppDrawer {
        anchors.fill: parent
        open: ShellState.drawerOpen
        onRequestClose: ShellState.drawerOpen = false
    }

    Connections {
        target: ShellState
        function onGoHome() { ShellState.drawerOpen = false; }
    }
}
