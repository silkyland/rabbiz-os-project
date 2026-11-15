import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import Quickshell.Services.UPower
import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services

// Home screen — Stitch-designed layout:
// dynamic gradient wallpaper + soft ambient glow (no shaders),
// editorial split-tone clock + At-a-Glance split pill, 4-column utility grid,
// SteamOS Gaming hero card, search pill, frosted dock, and slide-up AppDrawer.
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
    WlrLayershell.keyboardFocus: ShellState.drawerOpen
        ? WlrKeyboardFocus.OnDemand
        : WlrKeyboardFocus.None
    color: Theme.bg

    SystemClock {
        id: clock
        precision: SystemClock.Minutes
    }

    Process {
        id: steamFallbackProc
        command: ["a21s-steam"]
    }

    Process {
        id: settingsProc
        command: ["a21s-settings"]
    }

    readonly property var wp: ShellPrefs.activeWallpaper
    readonly property string timeFormatStr: ShellPrefs.use24h ? "HH:mm" : "h:mm ap"
    readonly property string hourStr: Qt.formatDateTime(clock.date, ShellPrefs.use24h ? "HH" : "h")
    readonly property string minStr: Qt.formatDateTime(clock.date, "mm")
    readonly property string ampmStr: ShellPrefs.use24h ? "" : Qt.formatDateTime(clock.date, "ap").toUpperCase()

    readonly property bool batteryOk: UPower.displayDevice !== null
        && UPower.displayDevice.ready
        && UPower.displayDevice.isLaptopBattery
    readonly property string powerLabel: batteryOk
        ? ("Battery " + Math.round(UPower.displayDevice.percentage * 100) + "%")
        : "AC Power"
    readonly property string netLabel: Network.wifiConnected
        ? Network.activeSsid
        : (Network.wifiEnabled ? "Wi-Fi On" : "Offline")

    // ---- Matte architectural gradient wallpaper (no neon orbs / no shaders) ----
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: home.wp.top }
            GradientStop { position: 0.48; color: home.wp.mid }
            GradientStop { position: 1.0; color: home.wp.bottom }
        }
    }

    // Whisper-soft top light falloff for subtle depth
    Rectangle {
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
        }
        height: home.height * 0.35
        gradient: Gradient {
            GradientStop { position: 0.0; color: Qt.alpha(home.wp.orb1, 0.03) }
            GradientStop { position: 1.0; color: "transparent" }
        }
    }

    Rectangle {
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        height: home.height * 0.30
        gradient: Gradient {
            GradientStop { position: 0.0; color: "transparent" }
            GradientStop { position: 1.0; color: Qt.alpha("#000000", 0.48) }
        }
    }

    // ---- Gesture layer: swipe up on empty space opens the app drawer. ----
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

    // ---- Clock + Date + Stitch At-a-Glance Split Pill ----
    Item {
        id: clockBlock
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
            topMargin: Theme.statusBarHeight + parent.height * 0.045
            leftMargin: Theme.spaceL
            rightMargin: Theme.spaceL
        }
        height: clockCol.implicitHeight

        ColumnLayout {
            id: clockCol
            anchors.fill: parent
            spacing: Theme.spaceS

            // Style 1: Editorial
            RowLayout {
                visible: ShellPrefs.clockStyle === "editorial"
                Layout.alignment: Qt.AlignHCenter
                spacing: 4

                Text {
                    text: home.hourStr
                    color: Theme.text
                    font.pixelSize: 124
                    font.family: Theme.fontFamily
                    font.weight: Font.Light
                }
                Text {
                    text: ":"
                    color: Qt.alpha(ShellPrefs.accentColor, 0.75)
                    font.pixelSize: 112
                    font.family: Theme.fontFamily
                    font.weight: Font.ExtraLight
                }
                Text {
                    text: home.minStr
                    color: ShellPrefs.accentColor
                    font.pixelSize: 124
                    font.family: Theme.fontFamily
                    font.weight: Font.DemiBold
                }
                Text {
                    visible: home.ampmStr.length > 0
                    text: home.ampmStr
                    color: Theme.textDim
                    font.pixelSize: Theme.fontCaption
                    font.family: Theme.fontFamily
                    font.weight: Font.DemiBold
                    Layout.alignment: Qt.AlignBottom
                    Layout.bottomMargin: 22
                }
            }

            // Style 2: Classic
            Text {
                visible: ShellPrefs.clockStyle === "classic"
                Layout.alignment: Qt.AlignHCenter
                text: Qt.formatDateTime(clock.date, home.timeFormatStr)
                color: Theme.text
                font.pixelSize: 118
                font.family: Theme.fontFamily
                font.weight: Font.Light
            }

            // Style 3: Compact
            Text {
                visible: ShellPrefs.clockStyle === "compact"
                Layout.fillWidth: true
                text: Qt.formatDateTime(clock.date, home.timeFormatStr)
                color: Theme.text
                font.pixelSize: 84
                font.family: Theme.fontFamily
                font.weight: Font.DemiBold
            }

            Text {
                Layout.alignment: ShellPrefs.clockStyle === "compact"
                    ? Qt.AlignLeft : Qt.AlignHCenter
                text: Qt.formatDateTime(clock.date, "dddd, MMMM d")
                color: Theme.textDim
                font.pixelSize: Theme.fontBody
                font.family: Theme.fontFamily
                font.weight: Font.Medium
            }

            // Stitch-style split At-a-Glance pill
            Rectangle {
                Layout.alignment: ShellPrefs.clockStyle === "compact"
                    ? Qt.AlignLeft : Qt.AlignHCenter
                Layout.topMargin: Theme.spaceXS
                implicitWidth: glanceRow.implicitWidth + Theme.spaceM * 2
                implicitHeight: glanceRow.implicitHeight + Theme.spaceS + 4
                radius: Theme.radiusPill
                color: Qt.alpha(Theme.surfaceHigh, 0.68)
                border.width: 1
                border.color: Qt.alpha(Theme.text, 0.10)

                RowLayout {
                    id: glanceRow
                    anchors.centerIn: parent
                    spacing: Theme.spaceS

                    MaterialIcon {
                        name: Network.wifiConnected ? "wifi" : "wifi_off"
                        size: Theme.fontBody
                        color: Network.wifiConnected ? ShellPrefs.accentColor : Theme.textFaint
                    }

                    Text {
                        text: home.netLabel
                        color: Theme.text
                        font.pixelSize: Theme.fontStatus
                        font.family: Theme.fontFamily
                        font.weight: Font.Medium
                    }

                    Rectangle {
                        width: 1
                        height: 18
                        color: Qt.alpha(Theme.text, 0.18)
                    }

                    MaterialIcon {
                        name: "bolt"
                        size: Theme.fontBody
                        color: Theme.textDim
                    }

                    Text {
                        text: home.powerLabel
                        color: Theme.text
                        font.pixelSize: Theme.fontStatus
                        font.family: Theme.fontFamily
                        font.weight: Font.Medium
                    }
                }

                TapHandler {
                    onTapped: {
                        if (!settingsProc.running)
                            settingsProc.running = true
                    }
                }
            }
        }
    }

    // ---- Home Utility Apps Row (Files, Calculator, Clock, Notes) ----
    property var homeApps: []

    function buildApps() {
        homeApps = [
            DesktopEntries.byId("a21s-files")      || ({ name: "Files",      materialIcon: "folder" }),
            DesktopEntries.byId("a21s-calculator") || ({ name: "Calculator", materialIcon: "calculate" }),
            DesktopEntries.byId("a21s-clock")      || ({ name: "Clock",      materialIcon: "schedule" }),
            DesktopEntries.byId("a21s-notes")      || ({ name: "Notes",      materialIcon: "edit_note" })
        ];
        dockApps = [
            DesktopEntries.byId("a21s-dialer")   || ({ name: "Phone",    materialIcon: "call" }),
            DesktopEntries.byId("a21s-sms")      || ({ name: "Messages", materialIcon: "sms" }),
            DesktopEntries.byId("a21s-settings") || ({ name: "Settings", materialIcon: "settings" }),
            DesktopEntries.byId("a21s-steam")    || ({ name: "Browser",  materialIcon: "public" })
        ];
    }

    property var dockApps: []

    Component.onCompleted: buildApps()

    Connections {
        target: DesktopEntries
        function onApplicationsChanged() { home.buildApps(); }
    }

    Row {
        id: homeGridRow
        anchors {
            horizontalCenter: parent.horizontalCenter
            top: clockBlock.bottom
            topMargin: Theme.spaceL
        }
        spacing: Theme.spaceM + 4

        Repeater {
            model: home.homeApps
            delegate: AppIcon {
                required property var modelData
                tileSize: Theme.iconApp - 4
                width: tileSize + Theme.spaceM
                height: tileSize + Theme.fontCaption + Theme.spaceS
                entry: modelData
                labelVisible: true
            }
        }
    }

    // ---- Featured SteamOS Gaming Card ----
    Rectangle {
        id: steamCard
        visible: ShellPrefs.showSteamCard
        anchors {
            left: parent.left
            right: parent.right
            bottom: searchPill.top
            leftMargin: Theme.spaceM
            rightMargin: Theme.spaceM
            bottomMargin: Theme.spaceM
        }
        height: 164
        radius: Theme.radiusL
        color: Theme.card
        border.width: 1
        border.color: Theme.separator
        clip: true

        ColumnLayout {
            anchors {
                fill: parent
                margins: Theme.spaceM
            }
            spacing: Theme.spaceS

            // Top meta row
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceS

                Rectangle {
                    width: 40
                    height: 40
                    radius: 20
                    color: Theme.surfaceHigh
                    border.width: 1
                    border.color: Theme.separator

                    MaterialIcon {
                        anchors.centerIn: parent
                        name: "sports_esports"
                        size: 22
                        color: Theme.text
                    }
                }

                Text {
                    text: "STEAM OS"
                    color: Theme.textDim
                    font.pixelSize: Theme.fontStatus
                    font.family: Theme.fontFamily
                    font.weight: Font.DemiBold
                    font.letterSpacing: 1.5
                }

                Item { Layout.fillWidth: true }

                Rectangle {
                    implicitWidth: modeRow.implicitWidth + Theme.spaceM
                    implicitHeight: modeRow.implicitHeight + 10
                    radius: Theme.radiusPill
                    color: Theme.surface
                    border.width: 1
                    border.color: Theme.separator

                    Row {
                        id: modeRow
                        anchors.centerIn: parent
                        spacing: 6
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 7; height: 7; radius: 4
                            color: Theme.success
                        }
                        Text {
                            text: "Gamescope"
                            color: Theme.textDim
                            font.pixelSize: Theme.fontStatus - 2
                            font.family: Theme.fontFamily
                        }
                    }
                }
            }

            // Bottom action row
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceM

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2

                    Text {
                        Layout.fillWidth: true
                        text: "SteamOS Gaming"
                        color: Theme.text
                        font.pixelSize: Theme.fontTitle
                        font.family: Theme.fontFamily
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                    }

                    Text {
                        text: "Big Picture · Panfrost Vulkan"
                        color: Theme.textDim
                        font.pixelSize: Theme.fontStatus
                        font.family: Theme.fontFamily
                    }
                }

                Rectangle {
                    implicitWidth: playRow.implicitWidth + Theme.spaceL + 8
                    implicitHeight: Theme.iconM + 4
                    radius: Theme.radiusPill
                    color: ShellPrefs.accentColor

                    Row {
                        id: playRow
                        anchors.centerIn: parent
                        spacing: 6
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: "Play"
                            color: ShellPrefs.onAccentColor
                            font.pixelSize: Theme.fontBody
                            font.family: Theme.fontFamily
                            font.weight: Font.DemiBold
                        }
                        MaterialIcon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "play_arrow"
                            size: Theme.fontBody
                            color: ShellPrefs.onAccentColor
                        }
                    }
                }
            }
        }

        scale: steamTap.pressed ? 0.97 : 1.0
        Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

        TapHandler {
            id: steamTap
            onTapped: {
                var steamEntry = DesktopEntries.byId("a21s-steam")
                if (steamEntry && typeof steamEntry.execute === "function")
                    steamEntry.execute()
                else if (!steamFallbackProc.running)
                    steamFallbackProc.running = true
            }
        }
    }

    // ---- Search Pill ----
    Rectangle {
        id: searchPill
        anchors {
            left: parent.left
            right: parent.right
            bottom: dockContainer.top
            leftMargin: Theme.spaceM
            rightMargin: Theme.spaceM
            bottomMargin: Theme.spaceM
        }
        height: Theme.iconM + Theme.spaceS + 4
        radius: Theme.radiusPill
        color: Qt.alpha(Theme.surfaceHigh, 0.68)
        border.width: 1
        border.color: Qt.alpha(Theme.text, 0.11)

        RowLayout {
            anchors {
                fill: parent
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
            }
            spacing: Theme.spaceS

            MaterialIcon {
                name: "search"
                size: Theme.fontTitle
                color: ShellPrefs.accentColor
            }

            Text {
                Layout.fillWidth: true
                text: "Search apps, files & settings…"
                color: Theme.textDim
                font.pixelSize: Theme.fontCaption
                font.family: Theme.fontFamily
            }

            MaterialIcon {
                name: "apps"
                size: Theme.fontTitle - 2
                color: Theme.textDim
            }
        }

        scale: searchTap.pressed ? 0.97 : 1.0
        Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

        TapHandler {
            id: searchTap
            onTapped: ShellState.drawerOpen = true;
        }
    }

    // ---- Frosted Glass Dock ----
    Rectangle {
        id: dockContainer
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            leftMargin: Theme.spaceM
            rightMargin: Theme.spaceM
            bottomMargin: Theme.navZoneHeight + Theme.spaceS
        }
        height: Theme.iconApp + Theme.spaceM
        radius: Theme.radiusL + 10
        color: Qt.alpha(Theme.surfaceHigh, 0.72)
        border.width: 1
        border.color: Qt.alpha(Theme.text, 0.12)

        Row {
            anchors.centerIn: parent
            spacing: Theme.spaceL

            Repeater {
                model: home.dockApps
                delegate: AppIcon {
                    required property var modelData
                    tileSize: Theme.iconApp - 8
                    width: tileSize + Theme.spaceS
                    height: tileSize
                    entry: modelData
                    labelVisible: false
                }
            }
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
