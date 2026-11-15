import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import Quickshell.Services.UPower
import Quickshell.Services.Pipewire
import A21s.Theme
import A21s.Services

// Android-style status bar (W6).
// PanelWindow at WlrLayer.Top with an exclusive zone so app surfaces start
// below it. Left: clock + carrier. Right: telephony bars, Wi-Fi, Bluetooth,
// muted-sink and battery indicators — all tiny geometric glyphs drawn from
// Theme tokens only (no emoji / no guaranteed-glyph fonts needed).
// Tapping the bar toggles the notification shade; the pull-down drag itself
// is handled by Shade.qml (W3).
PanelWindow {
    id: bar

    anchors { top: true; left: true; right: true }
    implicitHeight: Theme.statusBarHeight
    exclusionMode: ExclusionMode.Normal
    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.namespace: "a21s-statusbar"
    color: "transparent"

    // The a21s notification daemon facade lives here because the status bar is
    // instantiated unconditionally at boot (shell.qml): the shell owns the
    // org.freedesktop.Notifications name even while the shade is collapsed or
    // lazily loaded. Quickshell's underlying server is a per-process
    // singleton, so NotificationCenter's own facade sees the same tracked
    // notifications.
    NotificationServer { id: notifServer }

    SystemClock { id: clock; precision: SystemClock.Minutes }

    // --- derived metrics / state (all sizes derive from Theme tokens) ------
    readonly property int indH: Theme.statusBarHeight - Theme.spaceS * 2   // 20
    readonly property int smallFont: Math.round(Theme.fontStatus * 0.65)   // ~13

    // 0..4 filled bars, -1/unknown strength -> empty bars
    readonly property int cellLevel: Telephony.signalStrength >= 0
        ? Math.min(4, Math.ceil(Telephony.signalStrength / 25.0)) : 0

    readonly property bool muted: Pipewire.defaultAudioSink !== null
        && Pipewire.defaultAudioSink.audio !== null
        && Pipewire.defaultAudioSink.audio.muted

    // No battery driver on the A21s yet (M5): displayDevice may exist but be
    // unready — then we show the AC plug fallback instead of a bogus 0%.
    readonly property bool batteryOk: UPower.displayDevice !== null
        && UPower.displayDevice.ready
        && UPower.displayDevice.isLaptopBattery
    readonly property real batteryPct: batteryOk ? UPower.displayDevice.percentage : 0
    readonly property bool batteryCharging: batteryOk
        && (!UPower.onBattery
            || UPower.displayDevice.state === UPowerDeviceState.Charging
            || UPower.displayDevice.state === UPowerDeviceState.PendingCharge
            || UPower.displayDevice.state === UPowerDeviceState.FullyCharged)

    // Translucent bar background (~85% opacity of Theme.bg).
    Rectangle {
        anchors.fill: parent
        color: tapArea.pressed
               ? Qt.alpha(Theme.surfaceHigh, 0.9)
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

        Text {
            text: Qt.formatTime(clock.date, "HH:mm")
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontStatus
            font.weight: Font.DemiBold
        }
        Text {
            visible: text.length > 0
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

        // DEMO tag while the modem is simulated (M6).
        Rectangle {
            visible: Telephony.demoMode
            Layout.preferredWidth: demoText.implicitWidth + Theme.spaceS
            Layout.preferredHeight: demoText.implicitHeight + Theme.spaceXS
            Layout.alignment: Qt.AlignVCenter
            radius: Theme.radiusPill
            color: Theme.warn

            Text {
                id: demoText
                anchors.centerIn: parent
                text: "DEMO"
                color: Theme.bg
                font.pixelSize: Math.round(Theme.fontStatus * 0.55)
                font.weight: Font.Bold
            }
        }

        // Cellular signal
        MaterialIcon {
            Layout.alignment: Qt.AlignVCenter
            name: bar.cellLevel > 0 ? "signal_cellular_alt" : "signal_cellular_off"
            size: Theme.fontBody - 4
            color: bar.cellLevel > 0 ? Theme.text : Theme.textFaint
        }

        // Wi-Fi
        MaterialIcon {
            Layout.alignment: Qt.AlignVCenter
            visible: Network.wifiEnabled
            name: Network.wifiConnected ? "wifi" : "wifi_off"
            size: Theme.fontBody - 4
            color: Network.wifiConnected ? Theme.text : Theme.textFaint
        }

        // Bluetooth
        MaterialIcon {
            Layout.alignment: Qt.AlignVCenter
            visible: Bluetooth.powered
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

        // Battery % + icon (or AC power plug icon when battery driver is pending M5)
        Row {
            Layout.alignment: Qt.AlignVCenter
            spacing: 4

            Text {
                visible: bar.batteryOk
                anchors.verticalCenter: parent.verticalCenter
                text: Math.round(bar.batteryPct * 100) + "%"
                color: Theme.textDim
                font.family: Theme.fontFamily
                font.pixelSize: bar.smallFont + 2
                font.weight: Font.Medium
            }

            MaterialIcon {
                anchors.verticalCenter: parent.verticalCenter
                name: !bar.batteryOk ? "power"
                    : bar.batteryCharging ? "battery_charging_full"
                    : bar.batteryPct <= 0.15 ? "battery_alert"
                    : "battery_std"
                size: Theme.fontBody - 2
                color: !bar.batteryOk ? Theme.textDim
                     : bar.batteryCharging ? Theme.success
                     : bar.batteryPct <= 0.15 ? Theme.danger
                     : Theme.text
            }
        }
    }

    // Tap anywhere on the bar -> toggle the shade (convenience; the drag is
    // owned by the shade itself). Declared last so it sits on top of the
    // purely-decorative indicators.
    MouseArea {
        id: tapArea
        anchors.fill: parent
        onClicked: ShellState.shadeProgress = ShellState.shadeOpen ? 0 : 1
    }
}
