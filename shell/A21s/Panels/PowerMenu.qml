import Quickshell
import Quickshell.Wayland
import Quickshell.Services.UPower
import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services
import A21s.Widgets

// Overlay layer window owning:
//   1. Android-style Swipe-Up Lock Screen (clock, notifications, media, torch/power corners)
//   2. Power & Session Menu (Sleep, Lock, SteamOS, Reload Shell, Reboot, Power Off)
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

    readonly property bool batteryOk: UPower.displayDevice !== null
        && UPower.displayDevice.ready
        && UPower.displayDevice.isLaptopBattery
    readonly property int batteryPct: batteryOk
        ? Math.round(UPower.displayDevice.percentage * 100) : 0
    readonly property bool batteryCharging: batteryOk
        && (!UPower.onBattery
            || UPower.displayDevice.state === UPowerDeviceState.Charging
            || UPower.displayDevice.state === UPowerDeviceState.PendingCharge
            || UPower.displayDevice.state === UPowerDeviceState.FullyCharged)

    SystemClock {
        id: lockClock
        precision: SystemClock.Minutes
    }

    NotificationServer {
        id: lockNotifs
    }

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
    Item {
        id: lockLayer
        z: 1
        anchors.fill: parent
        visible: ShellState.lockScreenOpen && !ShellState.torchOverlayOpen && !ShellState.sleeping

        property real dragOffsetY: 0
        readonly property real unlockProgress: Math.min(1.0, Math.max(0.0, -dragOffsetY / 260.0))

        opacity: visible ? Math.max(0.12, 1.0 - unlockProgress * 0.88) : 0.0

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
                GradientStop { position: 0.0; color: Qt.alpha(ShellPrefs.activeWallpaper.top, 0.97) }
                GradientStop { position: 0.5; color: Qt.alpha(ShellPrefs.activeWallpaper.mid, 0.95) }
                GradientStop { position: 1.0; color: Qt.alpha("#000000", 0.98) }
            }

            Image {
                anchors.fill: parent
                source: ShellPrefs.activeWallpaper.image || ""
                fillMode: Image.PreserveAspectCrop
                opacity: 0.45
                smooth: true
                scale: 1.0 + lockLayer.unlockProgress * 0.06
                visible: status === Image.Ready
            }
        }

        Item {
            id: lockContent
            anchors.fill: parent
            y: Math.min(0, lockLayer.dragOffsetY)
            scale: 1.0 - lockLayer.unlockProgress * 0.04
            focus: true
            Keys.onEscapePressed: SystemControl.unlockScreen()

            // Background swipe-up unlock gesture area
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
                    if (lockLayer.dragOffsetY < -130)
                        SystemControl.unlockScreen()
                    else
                        lockSnapBack.restart()
                }
            }

            // Lock Screen Top Status Bar
            RowLayout {
                id: lockTopBar
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                    topMargin: Math.round(Theme.statusBarHeight * 0.35)
                    leftMargin: Theme.spaceL
                    rightMargin: Theme.spaceL
                }
                spacing: Theme.spaceS

                MaterialIcon {
                    name: SystemControl.airplaneMode ? "flight"
                        : (Network.wifiConnected ? "wifi" : "signal_cellular_alt")
                    size: 20
                    color: Theme.textDim
                }
                Text {
                    text: SystemControl.airplaneMode ? "Airplane Mode"
                        : (Telephony.operatorName || (Network.wifiConnected ? Network.activeSsid : "A21s Linux"))
                    color: Theme.textDim
                    font.pixelSize: Theme.fontStatus
                    font.family: Theme.fontFamily
                    font.weight: Font.Medium
                }

                Item { Layout.fillWidth: true }

                MaterialIcon {
                    visible: SystemControl.dndEnabled
                    name: "do_not_disturb_on"
                    size: 18
                    color: ShellPrefs.accentColor
                }
                MaterialIcon {
                    visible: SystemControl.flashlightOn
                    name: "flashlight_on"
                    size: 18
                    color: Theme.warn
                }
                Row {
                    visible: overlay.batteryOk
                    spacing: 4
                    MaterialIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: overlay.batteryCharging ? "battery_charging_full"
                            : (overlay.batteryPct > 80 ? "battery_full"
                            : (overlay.batteryPct > 35 ? "battery_std" : "battery_alert"))
                        size: 20
                        color: overlay.batteryCharging ? Theme.success
                            : (overlay.batteryPct <= 20 ? Theme.danger : Theme.text)
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: overlay.batteryPct + "%"
                        color: Theme.text
                        font.pixelSize: Theme.fontStatus
                        font.family: Theme.fontFamily
                        font.weight: Font.Medium
                    }
                }
            }

            // Main Clock + Status + Notifications + Media Column
            ColumnLayout {
                anchors {
                    left: parent.left
                    right: parent.right
                    top: lockTopBar.bottom
                    topMargin: 44
                    leftMargin: Theme.spaceL
                    rightMargin: Theme.spaceL
                }
                spacing: Theme.spaceM

                // Lock Icon & Editorial Split-Tone Clock
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceXS

                    MaterialIcon {
                        Layout.alignment: Qt.AlignHCenter
                        name: "lock"
                        size: 28
                        color: Qt.alpha(ShellPrefs.accentColor, 0.92)
                    }

                    RowLayout {
                        Layout.alignment: Qt.AlignHCenter
                        spacing: 4

                        Text {
                            text: Qt.formatDateTime(lockClock.date, ShellPrefs.use24h ? "HH" : "h")
                            color: Theme.text
                            font.pixelSize: 126
                            font.family: Theme.fontFamily
                            font.weight: Font.Light
                        }
                        Text {
                            text: ":"
                            color: Qt.alpha(ShellPrefs.accentColor, 0.78)
                            font.pixelSize: 112
                            font.family: Theme.fontFamily
                            font.weight: Font.ExtraLight
                        }
                        Text {
                            text: Qt.formatDateTime(lockClock.date, "mm")
                            color: ShellPrefs.accentColor
                            font.pixelSize: 126
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
                }

                // Lock Screen Status & Auto-Sleep Pill
                Rectangle {
                    Layout.alignment: Qt.AlignHCenter
                    implicitWidth: lockStatusRow.implicitWidth + Theme.spaceM * 2
                    implicitHeight: 38
                    radius: Theme.radiusPill
                    color: Qt.alpha(Theme.surfaceHigh, 0.72)
                    border.width: 1
                    border.color: Qt.alpha(Theme.text, 0.12)

                    RowLayout {
                        id: lockStatusRow
                        anchors.centerIn: parent
                        spacing: Theme.spaceS

                        MaterialIcon {
                            name: SystemControl.airplaneMode ? "flight" : (Network.wifiConnected ? "wifi" : (SystemControl.cpifLoaded ? "signal_cellular_alt" : "wifi_off"))
                            size: 18
                            color: ShellPrefs.accentColor
                        }
                        Text {
                            text: SystemControl.airplaneMode ? "Airplane"
                                : (Network.wifiConnected ? Network.activeSsid : (Telephony.operatorName || "Offline"))
                            color: Theme.text
                            font.pixelSize: Theme.fontStatus - 1
                            font.family: Theme.fontFamily
                        }
                        Rectangle {
                            visible: SystemControl.fingerprintSupported && ShellPrefs.fingerprintEnabled
                            width: 1; height: 14; color: Qt.alpha(Theme.text, 0.18)
                        }
                        MaterialIcon {
                            visible: SystemControl.fingerprintSupported && ShellPrefs.fingerprintEnabled
                            name: "fingerprint"
                            size: 16
                            color: Theme.success
                        }
                        Rectangle { width: 1; height: 14; color: Qt.alpha(Theme.text, 0.18) }
                        Text {
                            text: "CPU " + SystemControl.cpuPercent + "%"
                            color: Theme.textDim
                            font.pixelSize: Theme.fontStatus - 1
                            font.family: Theme.fontFamily
                        }
                        Rectangle { width: 1; height: 14; color: Qt.alpha(Theme.text, 0.18) }
                        Text {
                            text: ShellPrefs.sleepTimeoutSec === 0
                                ? "Awake"
                                : ("Sleep " + (ShellPrefs.sleepTimeoutSec >= 60
                                    ? Math.round(ShellPrefs.sleepTimeoutSec / 60) + "m"
                                    : ShellPrefs.sleepTimeoutSec + "s"))
                            color: Theme.textDim
                            font.pixelSize: Theme.fontStatus - 1
                            font.family: Theme.fontFamily
                        }
                    }
                }

                // Interactive Lock Screen Media Player Card
                Rectangle {
                    Layout.fillWidth: true
                    visible: Media.isPlaying || Media.hasRealPlayer
                    implicitHeight: 92
                    radius: Theme.radiusL
                    color: Qt.alpha(Theme.card, 0.88)
                    border.width: 1
                    border.color: Qt.alpha(ShellPrefs.accentColor, 0.35)

                    RowLayout {
                        anchors { fill: parent; margins: Theme.spaceM }
                        spacing: Theme.spaceM

                        Rectangle {
                            Layout.preferredWidth: 52
                            Layout.preferredHeight: 52
                            radius: Theme.radiusM
                            color: Qt.alpha(ShellPrefs.accentColor, 0.16)
                            clip: true

                            Image {
                                id: lockMediaArt
                                anchors.fill: parent
                                source: Media.artUrl
                                fillMode: Image.PreserveAspectCrop
                                visible: status === Image.Ready
                            }
                            MaterialIcon {
                                anchors.centerIn: parent
                                visible: lockMediaArt.status !== Image.Ready
                                name: "music_note"
                                size: 26
                                color: ShellPrefs.accentColor
                            }
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

                        Row {
                            spacing: 8
                            Rectangle {
                                width: 40; height: 40; radius: 20
                                color: Theme.surfaceHigh
                                MaterialIcon {
                                    anchors.centerIn: parent
                                    name: "skip_previous"
                                    size: 20; color: Theme.text
                                }
                                TapHandler { onTapped: Media.previous() }
                            }
                            Rectangle {
                                width: 46; height: 46
                                radius: Media.isPlaying ? 14 : 23
                                color: ShellPrefs.accentColor
                                Behavior on radius { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }
                                MaterialIcon {
                                    anchors.centerIn: parent
                                    name: Media.isPlaying ? "pause" : "play_arrow"
                                    size: 24; color: ShellPrefs.onAccentColor
                                }
                                TapHandler { onTapped: Media.togglePlaying() }
                            }
                            Rectangle {
                                width: 40; height: 40; radius: 20
                                color: Theme.surfaceHigh
                                MaterialIcon {
                                    anchors.centerIn: parent
                                    name: "skip_next"
                                    size: 20; color: Theme.text
                                }
                                TapHandler { onTapped: Media.next() }
                            }
                        }
                    }
                }

                // Live Lock Screen Notifications Preview (up to 3 items)
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceS

                    RowLayout {
                        Layout.fillWidth: true
                        visible: lockNotifs.count > 0
                        Text {
                            Layout.fillWidth: true
                            text: lockNotifs.count + (lockNotifs.count === 1 ? " Notification" : " Notifications")
                            color: Theme.textDim
                            font.pixelSize: Theme.fontStatus - 1
                            font.family: Theme.fontFamily
                            font.weight: Font.Medium
                        }
                        Rectangle {
                            implicitWidth: lockClearLbl.implicitWidth + Theme.spaceM
                            implicitHeight: 28
                            radius: Theme.radiusPill
                            color: Qt.alpha(Theme.surfaceHigh, 0.78)
                            border.width: 1
                            border.color: Qt.alpha(Theme.text, 0.12)
                            Text {
                                id: lockClearLbl
                                anchors.centerIn: parent
                                text: "Clear"
                                color: Theme.text
                                font.pixelSize: Theme.fontStatus - 3
                                font.family: Theme.fontFamily
                            }
                            TapHandler { onTapped: lockNotifs.dismissAll() }
                        }
                    }

                    Repeater {
                        model: lockNotifs.trackedNotifications
                        delegate: Rectangle {
                            id: lockNotifCard
                            required property var modelData
                            required property int index
                            visible: index < 3
                            Layout.fillWidth: true
                            implicitHeight: visible ? notifInner.implicitHeight + Theme.spaceM : 0
                            radius: Theme.radiusM
                            color: Qt.alpha(Theme.card, 0.84)
                            border.width: 1
                            border.color: Qt.alpha(Theme.text, 0.10)

                            RowLayout {
                                id: notifInner
                                anchors {
                                    left: parent.left
                                    right: parent.right
                                    verticalCenter: parent.verticalCenter
                                    leftMargin: Theme.spaceM
                                    rightMargin: Theme.spaceM
                                }
                                spacing: Theme.spaceM

                                Rectangle {
                                    width: 38; height: 38; radius: 12
                                    color: Qt.alpha(ShellPrefs.accentColor, 0.18)
                                    Text {
                                        anchors.centerIn: parent
                                        text: ((lockNotifCard.modelData && lockNotifCard.modelData.appName && lockNotifCard.modelData.appName.length > 0)
                                               ? lockNotifCard.modelData.appName.charAt(0) : "N").toUpperCase()
                                        color: ShellPrefs.accentColor
                                        font.pixelSize: Theme.fontCaption
                                        font.weight: Font.DemiBold
                                    }
                                }

                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 2
                                    RowLayout {
                                        Layout.fillWidth: true
                                        Text {
                                            Layout.fillWidth: true
                                            text: (lockNotifCard.modelData && lockNotifCard.modelData.summary)
                                                ? lockNotifCard.modelData.summary
                                                : ((lockNotifCard.modelData && lockNotifCard.modelData.appName) || "Notification")
                                            color: Theme.text
                                            font.pixelSize: Theme.fontCaption
                                            font.family: Theme.fontFamily
                                            font.weight: Font.DemiBold
                                            elide: Text.ElideRight
                                        }
                                        Text {
                                            text: lockNotifs.timeAgo(lockNotifCard.modelData)
                                            color: Theme.textFaint
                                            font.pixelSize: Theme.fontStatus - 3
                                            font.family: Theme.fontFamily
                                        }
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        visible: text.length > 0
                                        text: lockNotifCard.modelData ? (lockNotifCard.modelData.body || "") : ""
                                        color: Theme.textDim
                                        font.pixelSize: Theme.fontStatus - 2
                                        font.family: Theme.fontFamily
                                        maximumLineCount: 2
                                        elide: Text.ElideRight
                                        wrapMode: Text.Wrap
                                    }
                                }

                                Rectangle {
                                    width: 30; height: 30; radius: 15
                                    color: Qt.alpha(Theme.surfaceHigh, 0.85)
                                    MaterialIcon {
                                        anchors.centerIn: parent
                                        name: "close"
                                        size: 16; color: Theme.textDim
                                    }
                                    TapHandler {
                                        onTapped: {
                                            if (lockNotifCard.modelData)
                                                lockNotifCard.modelData.dismiss()
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Text {
                        visible: lockNotifs.count > 3
                        Layout.alignment: Qt.AlignHCenter
                        text: "+" + (lockNotifs.count - 3) + " more notifications inside shade"
                        color: Theme.textDim
                        font.pixelSize: Theme.fontStatus - 2
                        font.family: Theme.fontFamily
                    }

                    // Subtle empty pill when no notifications
                    Rectangle {
                        visible: lockNotifs.count === 0
                        Layout.alignment: Qt.AlignHCenter
                        implicitWidth: emptyNotifRow.implicitWidth + Theme.spaceM * 1.6
                        implicitHeight: 34
                        radius: Theme.radiusPill
                        color: Qt.alpha(Theme.surfaceHigh, 0.45)
                        border.width: 1
                        border.color: Qt.alpha(Theme.text, 0.08)

                        Row {
                            id: emptyNotifRow
                            anchors.centerIn: parent
                            spacing: 6
                            MaterialIcon {
                                anchors.verticalCenter: parent.verticalCenter
                                name: "notifications_none"
                                size: 16; color: Theme.textDim
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: "No new notifications"
                                color: Theme.textDim
                                font.pixelSize: Theme.fontStatus - 2
                                font.family: Theme.fontFamily
                            }
                        }
                    }
                }
            }

            // Bottom Lock Screen Controls: Torch Corner + Unlock/Sleep Center + Power Corner
            RowLayout {
                anchors {
                    left: parent.left
                    right: parent.right
                    bottom: parent.bottom
                    bottomMargin: Theme.navZoneHeight + Theme.spaceM
                    leftMargin: Theme.spaceL
                    rightMargin: Theme.spaceL
                }
                spacing: Theme.spaceM

                // Bottom-Left: Flashlight Quick Corner
                Rectangle {
                    Layout.alignment: Qt.AlignBottom
                    width: 58; height: 58; radius: 29
                    color: SystemControl.flashlightOn
                        ? ShellPrefs.accentColor
                        : Qt.alpha(Theme.surfaceHigh, 0.78)
                    border.width: 1
                    border.color: SystemControl.flashlightOn
                        ? ShellPrefs.accentColor
                        : Qt.alpha(Theme.text, 0.16)
                    scale: torchCornerTap.pressed ? 0.90 : 1.0
                    Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

                    MaterialIcon {
                        anchors.centerIn: parent
                        name: SystemControl.flashlightOn ? "flashlight_on" : "flashlight_off"
                        size: 26
                        color: SystemControl.flashlightOn ? ShellPrefs.onAccentColor : Theme.text
                    }
                    TapHandler {
                        id: torchCornerTap
                        onTapped: SystemControl.setFlashlight(!SystemControl.flashlightOn)
                    }
                }

                // Bottom-Center: Sleep Pill + Interactive Fingerprint Pad + Swipe-Up Unlock
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignBottom
                    spacing: Theme.spaceM

                    // Biometric Fingerprint Scanner Pad (EgisTech ET520)
                    Item {
                        visible: ShellPrefs.fingerprintEnabled
                        Layout.alignment: Qt.AlignHCenter
                        width: 112
                        height: 112

                        Timer {
                            id: fpScanHoldTimer
                            interval: 180
                            onTriggered: SystemControl.triggerFingerprintUnlock(true)
                        }

                        // Expanding sonar ripple ring while scanning or unlocking
                        Rectangle {
                            id: fpRipple
                            anchors.centerIn: parent
                            width: 84; height: 84; radius: 42
                            color: "transparent"
                            border.width: 2
                            border.color: SystemControl.fpState === "success" ? Theme.success : ShellPrefs.accentColor
                            scale: SystemControl.fpState === "scanning" ? 1.38
                                 : (SystemControl.fpState === "success" ? 1.48 : 1.0)
                            opacity: SystemControl.fpState === "scanning" ? 0.55
                                   : (SystemControl.fpState === "success" ? 0.0 : 0.0)
                            Behavior on scale { NumberAnimation { duration: 320; easing.type: Easing.OutCubic } }
                            Behavior on opacity { NumberAnimation { duration: 280 } }
                        }

                        // Main capacitive sensor circle
                        Rectangle {
                            id: fpPad
                            anchors.centerIn: parent
                            width: 84; height: 84; radius: 42
                            color: SystemControl.fpState === "success" ? Theme.success
                                 : (SystemControl.fpState === "error" ? Theme.danger
                                 : (SystemControl.fpState === "scanning"
                                    ? Qt.alpha(ShellPrefs.accentColor, 0.28)
                                    : Qt.alpha(Theme.surfaceHigh, 0.84)))
                            border.width: 2
                            border.color: SystemControl.fpState === "success" ? "#86EFAC"
                                        : (SystemControl.fpState === "error" ? "#FCA5A5"
                                        : (SystemControl.fpState === "scanning"
                                           ? ShellPrefs.accentColor
                                           : Qt.alpha(ShellPrefs.accentColor, 0.55)))
                            scale: SystemControl.fpState === "success" ? 1.08
                                 : (fpPadMouse.pressed ? 0.92 : 1.0)
                            Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Easing.OutBack } }
                            Behavior on color { ColorAnimation { duration: 180 } }
                            Behavior on border.color { ColorAnimation { duration: 180 } }

                            MaterialIcon {
                                anchors.centerIn: parent
                                name: SystemControl.fpState === "success" ? "check_circle"
                                    : (SystemControl.fpState === "error" ? "error_outline" : "fingerprint")
                                size: 42
                                color: SystemControl.fpState === "success" || SystemControl.fpState === "error"
                                    ? "#FFFFFF"
                                    : (SystemControl.fpState === "scanning" ? ShellPrefs.accentColor : Theme.text)
                            }
                        }

                        MouseArea {
                            id: fpPadMouse
                            anchors.fill: parent
                            onPressed: {
                                if (SystemControl.fpState !== "success") {
                                    SystemControl.fpState = "scanning"
                                    fpScanHoldTimer.restart()
                                }
                            }
                            onReleased: {
                                fpScanHoldTimer.stop()
                                if (SystemControl.fpState === "scanning")
                                    SystemControl.triggerFingerprintUnlock(true)
                            }
                            onCanceled: {
                                fpScanHoldTimer.stop()
                                if (SystemControl.fpState === "scanning")
                                    SystemControl.fpState = "idle"
                            }
                        }
                    }

                    Text {
                        visible: ShellPrefs.fingerprintEnabled
                        Layout.alignment: Qt.AlignHCenter
                        text: SystemControl.fpState === "success" ? "Unlocked · " + SystemControl.fingerprintChip
                            : (SystemControl.fpState === "error" ? "Fingerprint not recognized"
                            : (SystemControl.fpState === "scanning" ? "Verifying " + SystemControl.fingerprintChip + "..."
                            : "Touch sensor or swipe up to unlock"))
                        color: SystemControl.fpState === "success" ? Theme.success
                             : (SystemControl.fpState === "error" ? Theme.danger : Theme.textDim)
                        font.pixelSize: Theme.fontStatus - 1
                        font.family: Theme.fontFamily
                        font.weight: Font.Medium
                    }

                    Rectangle {
                        Layout.alignment: Qt.AlignHCenter
                        implicitWidth: sleepChipRow.implicitWidth + Theme.spaceM * 1.6
                        implicitHeight: 38
                        radius: Theme.radiusPill
                        color: sleepChipTap.pressed
                            ? Qt.alpha(ShellPrefs.accentColor, 0.25)
                            : Qt.alpha(Theme.surfaceHigh, 0.75)
                        border.width: 1
                        border.color: Qt.alpha(Theme.text, 0.14)

                        Row {
                            id: sleepChipRow
                            anchors.centerIn: parent
                            spacing: 6
                            MaterialIcon {
                                anchors.verticalCenter: parent.verticalCenter
                                name: "bedtime"
                                size: 18; color: ShellPrefs.accentColor
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: "Sleep · Screen Off"
                                color: Theme.text
                                font.pixelSize: Theme.fontStatus - 1
                                font.family: Theme.fontFamily
                                font.weight: Font.Medium
                            }
                        }
                        TapHandler {
                            id: sleepChipTap
                            onTapped: SystemControl.sleep()
                        }
                    }

                    ColumnLayout {
                        Layout.alignment: Qt.AlignHCenter
                        spacing: 4

                        MaterialIcon {
                            visible: !ShellPrefs.fingerprintEnabled
                            Layout.alignment: Qt.AlignHCenter
                            name: "keyboard_arrow_up"
                            size: 28
                            color: Theme.textDim
                        }
                        Text {
                            visible: !ShellPrefs.fingerprintEnabled
                            Layout.alignment: Qt.AlignHCenter
                            text: "Swipe up or tap to unlock"
                            color: Theme.textDim
                            font.pixelSize: Theme.fontCaption
                            font.family: Theme.fontFamily
                        }
                        Rectangle {
                            Layout.alignment: Qt.AlignHCenter
                            Layout.topMargin: 2
                            width: 150; height: 6; radius: 3
                            color: Qt.alpha(Theme.text, 0.70)
                        }
                        TapHandler {
                            onTapped: SystemControl.unlockScreen()
                        }
                    }
                }

                // Bottom-Right: Power Menu Quick Corner
                Rectangle {
                    Layout.alignment: Qt.AlignBottom
                    width: 58; height: 58; radius: 29
                    color: Qt.alpha(Theme.surfaceHigh, 0.78)
                    border.width: 1
                    border.color: Qt.alpha(Theme.text, 0.16)
                    scale: pwrCornerTap.pressed ? 0.90 : 1.0
                    Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

                    MaterialIcon {
                        anchors.centerIn: parent
                        name: "power_settings_new"
                        size: 26
                        color: Theme.danger
                    }
                    TapHandler {
                        id: pwrCornerTap
                        onTapped: ShellState.powerMenuOpen = true
                    }
                }
            }
        }
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
                        { key: "steam",      label: "SteamOS Mode",       sub: "Launch Big Picture",             icon: "sports_esports",     tint: "#38BDF8" },
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
