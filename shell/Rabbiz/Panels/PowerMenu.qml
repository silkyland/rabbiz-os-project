import Quickshell
import Quickshell.Wayland
import QtQuick
import QtQuick.Layouts
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets

// Overlay layer window owning:
//   1. Lock Screen (Panels/LockScreen.qml): shade-style clock, grouped notifications, actions
//   2. Power & Session Menu (Sleep, Lock, Reload Shell, Reboot, Power Off)
//   3. Pure white OLED Screen Torch overlay
//   4. Pure black Sleep Curtain (backlight off + double-tap/power-key wake)
//   5. Wayland IdleMonitors for session auto-sleep and quick lock-screen timeout
PanelWindow {
    id: overlay

    anchors { top: true; bottom: true; left: true; right: true }
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "a21s-overlay"
    WlrLayershell.keyboardFocus: (ShellState.powerMenuOpen || ShellState.lockScreenOpen || ShellState.sleeping)
        ? WlrKeyboardFocus.Exclusive
        : WlrKeyboardFocus.None
    color: "transparent"

    readonly property bool anyOpen: ShellState.powerMenuOpen
        || ShellState.lockScreenOpen
        || ShellState.torchOverlayOpen
        || ShellState.sleeping
    visible: anyOpen

    // ---- Wayland Idle Auto-Sleep Monitors --------------------------------
    IdleMonitor {
        id: sessionIdleMonitor
        enabled: ShellPrefs.sleepTimeoutSec > 0 && !ShellState.sleeping
        timeout: Math.max(15, ShellPrefs.sleepTimeoutSec)
        respectInhibitors: true
        onIsIdleChanged: {
            if (isIdle && !ShellState.sleeping)
                SystemControl.sleep()
        }
    }

    IdleMonitor {
        id: lockIdleMonitor
        enabled: ShellState.lockScreenOpen && !ShellState.sleeping && !ShellState.powerMenuOpen
        timeout: 25
        respectInhibitors: false
        onIsIdleChanged: {
            if (isIdle && ShellState.lockScreenOpen && !ShellState.sleeping && !ShellState.powerMenuOpen)
                SystemControl.sleep()
        }
    }

    // ---- 1) Lock Screen Overlay ------------------------------------------
    // The lock UI is its own component (Panels/LockScreen.qml); this layer only
    // decides when it is on screen and where the unlock/power actions go.
    LockScreen {
        id: lockScreen
        z: 1
        anchors.fill: parent
        visible: ShellState.lockScreenOpen && !ShellState.torchOverlayOpen && !ShellState.sleeping
        onUnlockRequested: SystemControl.unlockScreen()
        onPowerMenuRequested: ShellState.powerMenuOpen = true
    }

    // ---- 2) Power & Session Menu Overlay --------------------------------
    Item {
        id: powerLayer
        z: 10
        anchors.fill: parent
        visible: ShellState.powerMenuOpen && !ShellState.torchOverlayOpen && !ShellState.sleeping
        focus: visible
        Keys.onEscapePressed: ShellState.powerMenuOpen = false

        Rectangle {
            anchors.fill: parent
            color: Qt.alpha("#000000", 0.74)

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
                        { key: "screenshot", label: "Take Screenshot",  sub: "Save screen to Gallery",        icon: "photo_camera",       tint: "#A855F7" },
                        { key: "sleep",      label: "Sleep (Screen Off)", sub: "Lock + blank panel & powersave", icon: "bedtime",           tint: ShellPrefs.accentColor },
                        { key: "lock",       label: "Lock Screen",        sub: "Swipe-up lock overlay",          icon: "lock",               tint: Theme.text },
                        { key: "reload",     label: "Reload Shell UI",    sub: "Restart quickshell",             icon: "refresh",            tint: Theme.success },
                        { key: "reboot",     label: "Reboot Device",      sub: "Restart Linux kernel",           icon: "restart_alt",        tint: Theme.warn },
                        { key: "power",      label: "Power Off",          sub: "Shutdown Galaxy A21s",           icon: "power_settings_new", tint: Theme.danger }
                    ]

                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true
                        height: 68
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
                                if (modelData.key === "screenshot") {
                                    ShellState.powerMenuOpen = false
                                    SystemControl.takeScreenshot(320)
                                }
                                else if (modelData.key === "sleep") SystemControl.sleep()
                                else if (modelData.key === "lock") SystemControl.lockScreen()
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
        z: 25
        anchors.fill: parent
        visible: ShellState.torchOverlayOpen && !ShellState.sleeping
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

    // ---- 4) Pure-Black Sleep Curtain (absorbs pocket taps; double-tap or power key wakes) ----
    Rectangle {
        id: sleepLayer
        z: 50
        anchors.fill: parent
        visible: ShellState.sleeping
        color: "#000000"
        focus: visible

        Keys.onEscapePressed: SystemControl.wake()
        Keys.onSpacePressed: SystemControl.wake()
        Keys.onReturnPressed: SystemControl.wake()

        onVisibleChanged: {
            if (visible) {
                sleepMouse.lastTapMs = 0
                sleepLayer.forceActiveFocus()
            }
        }

        MouseArea {
            id: sleepMouse
            anchors.fill: parent
            property real lastTapMs: 0

            onClicked: {
                var now = Date.now()
                if (lastTapMs > 0 && (now - lastTapMs) < 480) {
                    lastTapMs = 0
                    SystemControl.wake()
                } else {
                    lastTapMs = now
                }
            }
            onPressAndHold: SystemControl.wake()
        }

        // Subtle AOD Wake & Unlock Fingerprint Ring when Screen Off
        Item {
            visible: ShellPrefs.fingerprintEnabled && ShellPrefs.fingerprintWakeUnlock
            anchors {
                horizontalCenter: parent.horizontalCenter
                bottom: parent.bottom
                bottomMargin: Theme.navZoneHeight + 76
            }
            width: 108
            height: 108

            Rectangle {
                anchors.centerIn: parent
                width: 76; height: 76; radius: 38
                color: aodFpMouse.pressed ? Qt.alpha(ShellPrefs.accentColor, 0.25) : "transparent"
                border.width: 1.5
                border.color: Qt.alpha(ShellPrefs.accentColor, aodFpMouse.pressed ? 0.85 : 0.34)
                MaterialIcon {
                    anchors.centerIn: parent
                    name: "fingerprint"
                    size: 36
                    color: Qt.alpha("#FFFFFF", aodFpMouse.pressed ? 0.90 : 0.38)
                }
            }

            MouseArea {
                id: aodFpMouse
                anchors.fill: parent
                onClicked: SystemControl.triggerFingerprintUnlock(true)
                onPressAndHold: SystemControl.triggerFingerprintUnlock(true)
            }
        }
    }
}
