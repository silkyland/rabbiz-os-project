import Quickshell
import Quickshell.Wayland
import Quickshell.Services.UPower
import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services

// Overlay layer window owning:
//   1. Power & Session Menu (Lock, SteamOS, Reload Shell, Reboot, Power Off)
//   2. Android-style Swipe-Up Lock Screen with clock, status & media controls
//   3. Pure white OLED Screen Torch overlay
PanelWindow {
    id: overlay

    anchors { top: true; bottom: true; left: true; right: true }
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "a21s-overlay"
    WlrLayershell.keyboardFocus: (ShellState.powerMenuOpen || ShellState.lockScreenOpen)
        ? WlrKeyboardFocus.Exclusive
        : WlrKeyboardFocus.None
    color: "transparent"

    readonly property bool anyOpen: ShellState.powerMenuOpen || ShellState.lockScreenOpen || ShellState.torchOverlayOpen
    visible: anyOpen

    SystemClock {
        id: lockClock
        precision: SystemClock.Minutes
    }

    // ---- 1) Lock Screen Overlay ------------------------------------------
    Item {
        id: lockLayer
        anchors.fill: parent
        visible: ShellState.lockScreenOpen && !ShellState.torchOverlayOpen
        opacity: visible ? Math.max(0.1, 1.0 - Math.min(1.0, Math.abs(dragOffsetY) / 360.0)) : 0.0

        property real dragOffsetY: 0

        NumberAnimation {
            id: lockSnapBack
            target: lockLayer
            property: "dragOffsetY"
            to: 0
            duration: Theme.animMed
            easing.type: Easing.OutQuint
        }

        onVisibleChanged: {
            if (visible) {
                dragOffsetY = 0
                lockContent.forceActiveFocus()
            }
        }

        // Wallpaper-matched dark glass backdrop
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0.0; color: Qt.alpha(ShellPrefs.activeWallpaper.top, 0.96) }
                GradientStop { position: 0.5; color: Qt.alpha(ShellPrefs.activeWallpaper.mid, 0.94) }
                GradientStop { position: 1.0; color: Qt.alpha("#000000", 0.97) }
            }

            Image {
                anchors.fill: parent
                source: ShellPrefs.activeWallpaper.image || ""
                fillMode: Image.PreserveAspectCrop
                opacity: 0.42
                smooth: true
                visible: status === Image.Ready
            }
        }

        Item {
            id: lockContent
            anchors.fill: parent
            y: Math.min(0, lockLayer.dragOffsetY)
            focus: true
            Keys.onEscapePressed: ShellState.lockScreenOpen = false

            MouseArea {
                anchors.fill: parent
                property real pressY: 0
                onPressed: (mouse) => {
                    lockSnapBack.stop()
                    pressY = mouse.y
                }
                onPositionChanged: (mouse) => {
                    if (pressed)
                        lockLayer.dragOffsetY = Math.min(0, mouse.y - pressY)
                }
                onReleased: {
                    if (lockLayer.dragOffsetY < -140)
                        ShellState.lockScreenOpen = false
                    else
                        lockSnapBack.restart()
                }
            }

            ColumnLayout {
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                    topMargin: Theme.statusBarHeight + 84
                    leftMargin: Theme.spaceL
                    rightMargin: Theme.spaceL
                }
                spacing: Theme.spaceS

                MaterialIcon {
                    Layout.alignment: Qt.AlignHCenter
                    name: "lock"
                    size: 28
                    color: Qt.alpha(Theme.textDim, 0.85)
                }

                RowLayout {
                    Layout.alignment: Qt.AlignHCenter
                    spacing: 4

                    Text {
                        text: Qt.formatDateTime(lockClock.date, ShellPrefs.use24h ? "HH" : "h")
                        color: Theme.text
                        font.pixelSize: 132
                        font.family: Theme.fontFamily
                        font.weight: Font.Light
                    }
                    Text {
                        text: ":"
                        color: Qt.alpha(ShellPrefs.accentColor, 0.78)
                        font.pixelSize: 118
                        font.family: Theme.fontFamily
                        font.weight: Font.ExtraLight
                    }
                    Text {
                        text: Qt.formatDateTime(lockClock.date, "mm")
                        color: ShellPrefs.accentColor
                        font.pixelSize: 132
                        font.family: Theme.fontFamily
                        font.weight: Font.DemiBold
                    }
                }

                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: Qt.formatDateTime(lockClock.date, "dddd, MMMM d")
                    color: Theme.textDim
                    font.pixelSize: Theme.fontBody
                    font.family: Theme.fontFamily
                    font.weight: Font.Medium
                }

                // Lock Screen Status Pill
                Rectangle {
                    Layout.alignment: Qt.AlignHCenter
                    Layout.topMargin: Theme.spaceS
                    implicitWidth: lockStatusRow.implicitWidth + Theme.spaceM * 2
                    implicitHeight: 40
                    radius: Theme.radiusPill
                    color: Qt.alpha(Theme.surfaceHigh, 0.72)
                    border.width: 1
                    border.color: Qt.alpha(Theme.text, 0.12)

                    RowLayout {
                        id: lockStatusRow
                        anchors.centerIn: parent
                        spacing: Theme.spaceS

                        MaterialIcon {
                            name: SystemControl.airplaneMode ? "flight" : (Network.wifiConnected ? "wifi" : "wifi_off")
                            size: 20
                            color: ShellPrefs.accentColor
                        }
                        Text {
                            text: SystemControl.airplaneMode ? "Airplane"
                                : (Network.wifiConnected ? Network.activeSsid : "Offline")
                            color: Theme.text
                            font.pixelSize: Theme.fontStatus
                            font.family: Theme.fontFamily
                        }
                        Rectangle { width: 1; height: 14; color: Qt.alpha(Theme.text, 0.18) }
                        Text {
                            text: "CPU " + SystemControl.cpuPercent + "%"
                            color: Theme.textDim
                            font.pixelSize: Theme.fontStatus
                            font.family: Theme.fontFamily
                        }
                    }
                }

                // Compact Lock Screen Media Controls
                Rectangle {
                    Layout.fillWidth: true
                    Layout.topMargin: Theme.spaceL
                    visible: Media.isPlaying || Media.hasRealPlayer
                    implicitHeight: 84
                    radius: Theme.radiusL
                    color: Qt.alpha(Theme.card, 0.88)
                    border.width: 1
                    border.color: Qt.alpha(ShellPrefs.accentColor, 0.35)

                    RowLayout {
                        anchors { fill: parent; margins: Theme.spaceM }
                        spacing: Theme.spaceM

                        MaterialIcon {
                            name: "music_note"
                            size: 28
                            color: ShellPrefs.accentColor
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            Text {
                                Layout.fillWidth: true
                                text: Media.title
                                color: Theme.text
                                font.pixelSize: Theme.fontCaption + 2
                                font.family: Theme.fontFamily
                                font.weight: Font.DemiBold
                                elide: Text.ElideRight
                            }
                            Text {
                                Layout.fillWidth: true
                                text: Media.artist
                                color: Theme.textDim
                                font.pixelSize: Theme.fontStatus - 1
                                font.family: Theme.fontFamily
                                elide: Text.ElideRight
                            }
                        }
                        Rectangle {
                            width: 44; height: 44
                            radius: Media.isPlaying ? 12 : 22
                            color: ShellPrefs.accentColor
                            Behavior on radius { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }
                            MaterialIcon {
                                anchors.centerIn: parent
                                name: Media.isPlaying ? "pause" : "play_arrow"
                                size: 24; color: ShellPrefs.onAccentColor
                            }
                            TapHandler { onTapped: Media.togglePlaying() }
                        }
                    }
                }
            }

            // Bottom Unlock Handle
            ColumnLayout {
                anchors {
                    horizontalCenter: parent.horizontalCenter
                    bottom: parent.bottom
                    bottomMargin: Theme.navZoneHeight + Theme.spaceM
                }
                spacing: Theme.spaceS

                MaterialIcon {
                    Layout.alignment: Qt.AlignHCenter
                    name: "keyboard_arrow_up"
                    size: 32
                    color: Theme.textDim
                }

                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: "Swipe up or tap to unlock"
                    color: Theme.textDim
                    font.pixelSize: Theme.fontCaption
                    font.family: Theme.fontFamily
                }

                Rectangle {
                    Layout.alignment: Qt.AlignHCenter
                    width: 160; height: 6; radius: 3
                    color: Qt.alpha(Theme.text, 0.65)
                }

                TapHandler {
                    onTapped: ShellState.lockScreenOpen = false
                }
            }
        }
    }

    // ---- 2) Power & Session Menu Overlay --------------------------------
    Item {
        id: powerLayer
        anchors.fill: parent
        visible: ShellState.powerMenuOpen && !ShellState.torchOverlayOpen
        focus: visible
        Keys.onEscapePressed: ShellState.powerMenuOpen = false

        Rectangle {
            anchors.fill: parent
            color: Qt.alpha("#000000", 0.72)

            MouseArea {
                anchors.fill: parent
                onClicked: ShellState.powerMenuOpen = false
            }
        }

        Rectangle {
            id: powerDialog
            anchors.centerIn: parent
            width: Math.min(parent.width - Theme.spaceL * 2, 500)
            implicitHeight: powerCol.implicitHeight + Theme.spaceL * 2
            radius: Theme.radiusL
            color: Qt.alpha(Theme.surface, 0.96)
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.14)
            scale: ShellState.powerMenuOpen ? 1.0 : 0.88
            opacity: ShellState.powerMenuOpen ? 1.0 : 0.0

            Behavior on scale { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }
            Behavior on opacity { NumberAnimation { duration: Theme.animFast } }

            ColumnLayout {
                id: powerCol
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                    margins: Theme.spaceL
                }
                spacing: Theme.spaceM

                RowLayout {
                    Layout.fillWidth: true
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Text {
                            text: "Power & Session"
                            color: Theme.text
                            font.pixelSize: Theme.fontTitle
                            font.family: Theme.fontFamily
                            font.weight: Font.DemiBold
                        }
                        Text {
                            text: "CPU " + SystemControl.cpuPercent + "% · RAM " + SystemControl.memUsedText
                            color: Theme.textDim
                            font.pixelSize: Theme.fontStatus
                            font.family: Theme.fontFamily
                        }
                    }

                    Rectangle {
                        width: 40; height: 40; radius: 20
                        color: Theme.surfaceHigh
                        MaterialIcon {
                            anchors.centerIn: parent
                            name: "close"
                            size: 22; color: Theme.textDim
                        }
                        TapHandler {
                            onTapped: ShellState.powerMenuOpen = false
                        }
                    }
                }

                Repeater {
                    model: [
                        { key: "lock",    label: "Lock Screen",       sub: "Swipe-up lock overlay", icon: "lock",               tint: Theme.text },
                        { key: "steam",   label: "SteamOS Mode",      sub: "Launch Big Picture",    icon: "sports_esports",     tint: ShellPrefs.accentColor },
                        { key: "reload",  label: "Reload Shell UI",   sub: "Restart quickshell",    icon: "refresh",            tint: Theme.success },
                        { key: "reboot",  label: "Reboot Device",     sub: "Restart Linux kernel",  icon: "restart_alt",        tint: Theme.warn },
                        { key: "power",   label: "Power Off",         sub: "Shutdown Galaxy A21s",  icon: "power_settings_new", tint: Theme.danger }
                    ]

                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true
                        height: 70
                        radius: Theme.radiusM
                        color: rowTap.pressed ? Theme.surfaceHigh : Theme.card
                        border.width: 1
                        border.color: modelData.key === "power"
                            ? Qt.alpha(Theme.danger, 0.35)
                            : Qt.alpha(Theme.text, 0.08)
                        scale: rowTap.pressed ? 0.96 : 1.0
                        Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

                        RowLayout {
                            anchors {
                                fill: parent
                                leftMargin: Theme.spaceM
                                rightMargin: Theme.spaceM
                            }
                            spacing: Theme.spaceM

                            Rectangle {
                                width: 44; height: 44; radius: 22
                                color: Qt.alpha(modelData.tint, 0.16)
                                MaterialIcon {
                                    anchors.centerIn: parent
                                    name: modelData.icon
                                    size: 24; color: modelData.tint
                                }
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 1
                                Text {
                                    text: modelData.label
                                    color: Theme.text
                                    font.pixelSize: Theme.fontBody
                                    font.family: Theme.fontFamily
                                    font.weight: Font.Medium
                                }
                                Text {
                                    text: modelData.sub
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontStatus - 1
                                    font.family: Theme.fontFamily
                                }
                            }

                            MaterialIcon {
                                name: "chevron_right"
                                size: 22; color: Theme.textFaint
                            }
                        }

                        TapHandler {
                            id: rowTap
                            onTapped: {
                                if (modelData.key === "lock") SystemControl.lockScreen()
                                else if (modelData.key === "steam") SystemControl.switchToSteamOS()
                                else if (modelData.key === "reload") SystemControl.reloadShell()
                                else if (modelData.key === "reboot") SystemControl.reboot()
                                else if (modelData.key === "power") SystemControl.powerOff()
                            }
                        }
                    }
                }
            }
        }
    }

    // ---- 3) Screen Torch Overlay (OLED fallback when no camera flash LED exists) ----
    Rectangle {
        z: 20
        anchors.fill: parent
        visible: ShellState.torchOverlayOpen
        color: "#FFFFFF"

        Rectangle {
            anchors {
                horizontalCenter: parent.horizontalCenter
                bottom: parent.bottom
                bottomMargin: Theme.navZoneHeight + Theme.spaceXL
            }
            implicitWidth: torchRow.implicitWidth + Theme.spaceL * 1.4
            implicitHeight: 56
            radius: Theme.radiusPill
            color: Qt.alpha("#09090B", 0.86)
            border.width: 1
            border.color: Qt.alpha("#FFFFFF", 0.25)

            RowLayout {
                id: torchRow
                anchors.centerIn: parent
                spacing: Theme.spaceS

                MaterialIcon {
                    Layout.alignment: Qt.AlignVCenter
                    name: "flashlight_on"
                    size: 24
                    color: Theme.warn
                }
                Text {
                    Layout.alignment: Qt.AlignVCenter
                    text: "Screen Torch On · Tap anywhere to turn off"
                    color: "#FFFFFF"
                    font.pixelSize: Theme.fontCaption
                    font.family: Theme.fontFamily
                    font.weight: Font.Medium
                }
            }
        }

        TapHandler {
            onTapped: SystemControl.setFlashlight(false)
        }
    }
}
