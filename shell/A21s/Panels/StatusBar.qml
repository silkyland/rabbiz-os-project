import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import Quickshell.Services.UPower
import Quickshell.Services.Pipewire
import A21s.Theme
import A21s.Services
import A21s.Widgets

// Android-style status bar with a morphing Dynamic Island / Heads-Up
// Notification capsule, airplane/DND indicators, and shade tap toggle.
PanelWindow {
    id: bar

    anchors { top: true; left: true; right: true }
    implicitHeight: Theme.statusBarHeight
    // Immersive: hide while an app fills the screen; the surface unmaps so
    // labwc gives the whole top strip back. The shade handle / power menu
    // / lock screen bring it back temporarily.
    visible: !ShellState.appOpen
        || ShellState.shadeProgress > 0.02
        || ShellState.powerMenuOpen
        || ShellState.lockScreenOpen
        || ShellState.sleeping
    // Pure overlay: never reserves space (labwc margin top=0), so apps
    // stay edge-to-edge and don't reflow when the bar appears/disappears.
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "a21s-statusbar"
    color: "transparent"

    NotificationServer {
        id: notifServer
        onNewNotification: function(notification) {
            if (SystemControl.dndEnabled || !notification)
                return
            bar.toastApp = notification.appName || "Notification"
            bar.toastSummary = notification.summary || bar.toastApp
            bar.toastActive = true
            toastTimer.restart()
        }
    }

    SystemClock { id: clock; precision: SystemClock.Minutes }

    property bool toastActive: false
    property string toastApp: ""
    property string toastSummary: ""

    Timer {
        id: toastTimer
        interval: 4200
        onTriggered: bar.toastActive = false
    }

    readonly property int smallFont: Math.round(Theme.fontStatus * 0.65)
    readonly property int cellLevel: Telephony.signalStrength >= 0
        ? Math.min(4, Math.ceil(Telephony.signalStrength / 25.0)) : 0
    readonly property bool muted: Pipewire.defaultAudioSink !== null
        && Pipewire.defaultAudioSink.audio !== null
        && Pipewire.defaultAudioSink.audio.muted
    // A phone battery also counts: UPower's DisplayDevice is a battery when the kernel
    // exposes one, and `isPresent` is false while nothing is wired (which is the current
    // A21s state: no fuel-gauge driver yet, /sys/class/power_supply is empty).
    readonly property bool batteryOk: UPower.displayDevice !== null
        && UPower.displayDevice.ready
        && UPower.displayDevice.isPresent
    readonly property real batteryPct: batteryOk ? UPower.displayDevice.percentage : 0
    readonly property bool batteryCharging: batteryOk
        && (!UPower.onBattery
            || UPower.displayDevice.state === UPowerDeviceState.Charging
            || UPower.displayDevice.state === UPowerDeviceState.PendingCharge
            || UPower.displayDevice.state === UPowerDeviceState.FullyCharged)

    readonly property bool islandExpanded: bar.toastActive || Media.isPlaying || SystemControl.flashlightOn

    // Translucent bar background
    Rectangle {
        anchors.fill: parent
        color: tapArea.pressed
               ? Qt.alpha(Theme.surfaceHigh, 0.92)
               : Qt.alpha(Theme.bg, 0.85)
    }

    // ---- LEFT: clock + carrier -------------------------------------------
    RowLayout {
        anchors {
            left: parent.left
            leftMargin: Theme.spaceS
            verticalCenter: parent.verticalCenter
        }
        spacing: Theme.spaceS
        opacity: bar.toastActive ? 0.25 : 1.0
        Behavior on opacity { NumberAnimation { duration: Theme.animFast } }

        Text {
            text: Qt.formatTime(clock.date, "HH:mm")
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontStatus
            font.weight: Font.DemiBold
        }
        Text {
            visible: text.length > 0 && !bar.toastActive
            text: Telephony.operatorName
            color: Theme.textDim
            font.pixelSize: bar.smallFont
        }
    }

    // ---- RIGHT: indicator cluster ----------------------------------------
    RowLayout {
        anchors {
            right: parent.right
            rightMargin: Theme.spaceS
            verticalCenter: parent.verticalCenter
        }
        spacing: Theme.spaceS
        opacity: bar.toastActive ? 0.25 : 1.0
        Behavior on opacity { NumberAnimation { duration: Theme.animFast } }

        // Airplane indicator
        MaterialIcon {
            Layout.alignment: Qt.AlignVCenter
            visible: SystemControl.airplaneMode
            name: "flight"
            size: Theme.fontBody - 4
            color: Theme.warn
        }

        // Do Not Disturb indicator
        MaterialIcon {
            Layout.alignment: Qt.AlignVCenter
            visible: SystemControl.dndEnabled
            name: "do_not_disturb_on"
            size: Theme.fontBody - 4
            color: Theme.warn
        }

        // Cellular signal
        MaterialIcon {
            Layout.alignment: Qt.AlignVCenter
            visible: !SystemControl.airplaneMode
            name: bar.cellLevel > 0 ? "signal_cellular_alt" : "signal_cellular_off"
            size: Theme.fontBody - 4
            color: bar.cellLevel > 0 ? Theme.text : Theme.textFaint
        }

        // Wi-Fi
        MaterialIcon {
            Layout.alignment: Qt.AlignVCenter
            visible: Network.wifiEnabled && !SystemControl.airplaneMode
            name: Network.wifiConnected ? "wifi" : "wifi_off"
            size: Theme.fontBody - 4
            color: Network.wifiConnected ? Theme.text : Theme.textFaint
        }

        // Bluetooth
        MaterialIcon {
            Layout.alignment: Qt.AlignVCenter
            visible: Bluetooth.powered && !SystemControl.airplaneMode
            name: Bluetooth.connected ? "bluetooth_connected" : "bluetooth"
            size: Theme.fontBody - 4
            color: Bluetooth.connected ? Theme.text : Theme.textDim
        }

        // Muted default sink
        MaterialIcon {
            Layout.alignment: Qt.AlignVCenter
            visible: bar.muted
            name: "volume_off"
            size: Theme.fontBody - 4
            color: Theme.text
        }

        // Battery % + icon
        Row {
            Layout.alignment: Qt.AlignVCenter
            spacing: 4

            Text {
                visible: !bar.batteryOk
                anchors.verticalCenter: parent.verticalCenter
                text: "AC"
                color: Theme.textFaint
                font.family: Theme.fontFamily
                font.pixelSize: bar.smallFont
                font.weight: Font.Medium
            }

            Text {
                visible: bar.batteryOk
                anchors.verticalCenter: parent.verticalCenter
                text: Math.round(bar.batteryPct * 100) + "%"
                color: Theme.textDim
                font.family: Theme.fontFamily
                font.pixelSize: bar.smallFont + 2
                font.weight: Font.Medium
            }

            BatteryMeter {
                anchors.verticalCenter: parent.verticalCenter
                present: bar.batteryOk
                level: bar.batteryPct
                charging: bar.batteryCharging
            }
        }
    }

    // Pull down on the status bar to open the shade (Android 12+ split): the half you start
    // on picks the page — left = notifications + media, right = quick settings. A plain tap
    // still toggles the shade. Before this, the bar only had onClicked, so dragging from the
    // top bar did nothing at all.
    MouseArea {
        id: tapArea
        anchors.fill: parent
        property real pressY: 0
        property real pressProgress: 0
        property bool dragging: false

        onPressed: (m) => {
            pressY = m.y
            pressProgress = ShellState.shadeProgress
            dragging = false
            if (ShellState.shadeProgress < 0.05)
                ShellState.shadeMode = m.x < width / 2 ? "notifications" : "settings"
        }
        onPositionChanged: (m) => {
            if (!pressed)
                return
            var dy = m.y - pressY
            if (!dragging && Math.abs(dy) > 8)
                dragging = true
            if (dragging) {
                var range = Math.max(1, (bar.screen ? bar.screen.height : 1600)
                                        - Theme.shadeHandleZone)
                ShellState.shadeProgress = Math.max(0, Math.min(1, pressProgress + dy / range))
            }
        }
        onReleased: (m) => {
            if (!dragging) {
                // Plain tap: same instant toggle the bar always had.
                ShellState.shadeProgress = ShellState.shadeOpen ? 0 : 1
            } else {
                ShellState.shadeSnapRequested(ShellState.shadeProgress > 0.3 ? 1 : 0)
            }
        }
        onCanceled: {
            if (dragging)
                ShellState.shadeSnapRequested(ShellState.shadeProgress > 0.3 ? 1 : 0)
        }
    }

    // ---- CENTER: Dynamic Island / Heads-Up Notification Capsule ----------
    Rectangle {
        id: islandCapsule
        z: 5
        anchors.centerIn: parent
        visible: bar.islandExpanded
        opacity: bar.islandExpanded ? 1.0 : 0.0
        width: bar.toastActive
            ? Math.min(340, Math.max(140, islandRow.implicitWidth + 24))
            : Math.min(200, Math.max(106, islandRow.implicitWidth + 20))
        height: bar.toastActive ? 34 : 28
        radius: height / 2
        color: Qt.alpha(Theme.surfaceHigh, 0.96)
        border.width: 1
        border.color: bar.toastActive
            ? ShellPrefs.accentColor
            : (SystemControl.flashlightOn ? Theme.warn : Qt.alpha(ShellPrefs.accentColor, 0.45))

        Behavior on width { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }
        Behavior on height { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }
        Behavior on opacity { NumberAnimation { duration: Theme.animFast } }
        Behavior on border.color { ColorAnimation { duration: Theme.animFast } }

        RowLayout {
            id: islandRow
            anchors {
                centerIn: parent
            }
            width: Math.min(parent.width - 18, implicitWidth)
            spacing: 6

            MaterialIcon {
                Layout.alignment: Qt.AlignVCenter
                name: bar.toastActive ? "notifications_active"
                    : (SystemControl.flashlightOn ? "flashlight_on" : "music_note")
                size: 16
                color: SystemControl.flashlightOn && !bar.toastActive ? Theme.warn : ShellPrefs.accentColor
            }

            Text {
                Layout.alignment: Qt.AlignVCenter
                Layout.fillWidth: true
                text: bar.toastActive ? bar.toastSummary
                    : (SystemControl.flashlightOn ? "Torch On" : Media.title)
                color: Theme.text
                font.pixelSize: bar.smallFont + 1
                font.family: Theme.fontFamily
                font.weight: Font.Medium
                elide: Text.ElideRight
            }
        }

        TapHandler {
            onTapped: {
                if (bar.toastActive) {
                    bar.toastActive = false
                    toastTimer.stop()
                    ShellState.shadeProgress = 1
                } else if (SystemControl.flashlightOn) {
                    SystemControl.setFlashlight(false)
                } else if (Media.isPlaying) {
                    Media.togglePlaying()
                }
            }
        }
    }
}
