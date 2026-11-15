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

        // Cellular signal: 4 ascending bars.
        Item {
            Layout.preferredWidth: cellRow.implicitWidth
            Layout.preferredHeight: bar.indH
            Layout.alignment: Qt.AlignVCenter

            Row {
                id: cellRow
                anchors.bottom: parent.bottom
                spacing: Math.round(Theme.spaceXS / 2)

                Repeater {
                    model: 4
                    Rectangle {
                        required property int index
                        width: Math.round(Theme.spaceXS / 2) + 1
                        height: Math.round(bar.indH * 0.65 * (0.3 + 0.7 * index / 3))
                        radius: Math.round(Theme.spaceXS / 6)
                        color: index < bar.cellLevel ? Theme.text : Theme.textFaint
                    }
                }
            }
        }

        // Wi-Fi: 3 concentric arcs + dot. Hidden when the radio is off;
        // all-dim when enabled but not associated.
        Canvas {
            id: wifiCanvas
            visible: Network.wifiEnabled
            Layout.preferredWidth: Math.round(Theme.iconS * 0.6)
            Layout.preferredHeight: bar.indH
            Layout.alignment: Qt.AlignVCenter

            property int arcs: !Network.wifiConnected ? 0
                : Network.signalStrength < 0 ? 3
                : Math.min(3, 1 + Math.floor(Network.signalStrength / 34))
            onArcsChanged: requestPaint()

            onPaint: {
                var ctx = getContext("2d");
                ctx.reset();
                var cx = width / 2;
                var cy = height - Theme.spaceXS / 2;
                ctx.lineWidth = Math.max(1.5, Theme.spaceXS / 4);
                ctx.lineCap = "round";
                for (var i = 0; i < 3; ++i) {
                    ctx.beginPath();
                    ctx.strokeStyle = i < arcs ? Theme.text : Theme.textFaint;
                    ctx.arc(cx, cy, (i + 1) * width / 6,
                            -Math.PI * 0.75, -Math.PI * 0.25);
                    ctx.stroke();
                }
                ctx.beginPath();
                ctx.fillStyle = arcs > 0 ? Theme.text : Theme.textFaint;
                ctx.arc(cx, cy, ctx.lineWidth, 0, Math.PI * 2);
                ctx.fill();
            }
        }

        // Bluetooth rune — only shown while the adapter is powered; brighter
        // when a device is connected.
        Canvas {
            id: btCanvas
            visible: Bluetooth.powered
            Layout.preferredWidth: Math.round(bar.indH * 0.55)
            Layout.preferredHeight: bar.indH
            Layout.alignment: Qt.AlignVCenter

            property bool conn: Bluetooth.connected
            onConnChanged: requestPaint()

            onPaint: {
                var ctx = getContext("2d");
                ctx.reset();
                var x = width / 2;
                var t = Theme.spaceXS / 4;
                var b = height - Theme.spaceXS / 4;
                var m = height / 2;
                var r = x + width * 0.4;
                var l = x - width * 0.4;
                ctx.strokeStyle = conn ? Theme.text : Theme.textDim;
                ctx.lineWidth = Math.max(1.4, Theme.spaceXS / 4);
                ctx.lineCap = "round";
                ctx.lineJoin = "round";
                ctx.beginPath();
                ctx.moveTo(x, t);
                ctx.lineTo(x, b);                       // spine
                ctx.moveTo(x, t);
                ctx.lineTo(r, m - Theme.spaceXS / 2);   // upper right vertex
                ctx.lineTo(l, b - Theme.spaceXS / 2);   // ...down-left leg
                ctx.moveTo(x, b);
                ctx.lineTo(r, m + Theme.spaceXS / 2);   // lower right vertex
                ctx.lineTo(l, t + Theme.spaceXS / 2);   // ...up-left leg
                ctx.stroke();
            }
        }

        // Muted default sink: speaker body + cone + slash. Hidden otherwise.
        Canvas {
            id: muteCanvas
            visible: bar.muted
            Layout.preferredWidth: Math.round(bar.indH * 0.9)
            Layout.preferredHeight: bar.indH
            Layout.alignment: Qt.AlignVCenter

            onPaint: {
                var ctx = getContext("2d");
                ctx.reset();
                var w = width;
                var h = height;
                var s = Math.min(w, h);
                var x0 = 1;
                var cy = h / 2;
                ctx.fillStyle = Theme.text;
                ctx.beginPath();
                ctx.rect(x0, cy - s * 0.16, s * 0.22, s * 0.32);
                ctx.moveTo(x0 + s * 0.22, cy - s * 0.16);
                ctx.lineTo(x0 + s * 0.62, cy - s * 0.42);
                ctx.lineTo(x0 + s * 0.62, cy + s * 0.42);
                ctx.lineTo(x0 + s * 0.22, cy + s * 0.16);
                ctx.closePath();
                ctx.fill();
                ctx.strokeStyle = Theme.text;
                ctx.lineWidth = Math.max(1.5, s * 0.09);
                ctx.lineCap = "round";
                ctx.beginPath();
                ctx.moveTo(x0, h - Theme.spaceXS / 2);
                ctx.lineTo(w - x0, Theme.spaceXS / 2);
                ctx.stroke();
            }
        }

        // Battery: percentage text + outlined cell with fill. When no usable
        // battery device exists (driver not landed yet) an AC plug glyph is
        // drawn instead.
        Item {
            Layout.preferredWidth: batRow.implicitWidth
            Layout.preferredHeight: bar.indH
            Layout.alignment: Qt.AlignVCenter

            Row {
                id: batRow
                anchors.centerIn: parent
                spacing: Theme.spaceXS

                Text {
                    visible: bar.batteryOk
                    text: Math.round(bar.batteryPct * 100)
                    color: Theme.textDim
                    font.pixelSize: bar.smallFont
                    anchors.verticalCenter: parent.verticalCenter
                }

                // battery cell
                Item {
                    visible: bar.batteryOk
                    width: visible ? Math.round(Theme.iconS * 0.55) : 0
                    height: Math.round(bar.indH * 0.55)
                    anchors.verticalCenter: parent.verticalCenter

                    Rectangle {                       // outline
                        anchors.fill: parent
                        color: "transparent"
                        border.width: 1
                        border.color: bar.batteryCharging ? Theme.accent : Theme.text
                        radius: Math.round(Theme.spaceXS / 2)
                    }
                    Rectangle {                       // charge fill
                        x: Theme.spaceXS / 3
                        y: x
                        height: parent.height - x * 2
                        width: Math.max(0, (parent.width - x * 2) * bar.batteryPct)
                        radius: Math.round(Theme.spaceXS / 6)
                        color: bar.batteryCharging ? Theme.accent
                             : bar.batteryPct <= 0.15 ? Theme.danger
                             : Theme.text
                    }
                    Rectangle {                       // positive nub
                        anchors {
                            left: parent.right
                            verticalCenter: parent.verticalCenter
                        }
                        width: Theme.spaceXS / 3
                        height: Math.round(parent.height * 0.4)
                        color: bar.batteryCharging ? Theme.accent : Theme.text
                    }
                }

                // AC plug fallback: body + two prongs + cord stub.
                Item {
                    visible: !bar.batteryOk
                    width: visible ? Math.round(bar.indH * 0.55) : 0
                    height: bar.indH
                    anchors.verticalCenter: parent.verticalCenter

                    Rectangle {                       // prong L
                        x: Math.round(parent.width / 2) - Theme.spaceXS / 2 - 1
                        y: Math.round(parent.height * 0.12)
                        width: Math.round(Theme.spaceXS / 3)
                        height: Math.round(parent.height * 0.32)
                        color: Theme.textDim
                    }
                    Rectangle {                       // prong R
                        x: Math.round(parent.width / 2) + Theme.spaceXS / 2
                        y: Math.round(parent.height * 0.12)
                        width: Math.round(Theme.spaceXS / 3)
                        height: Math.round(parent.height * 0.32)
                        color: Theme.textDim
                    }
                    Rectangle {                       // plug body
                        anchors.horizontalCenter: parent.horizontalCenter
                        y: Math.round(parent.height * 0.44)
                        width: Math.round(parent.width * 0.85)
                        height: Math.round(parent.height * 0.3)
                        radius: Math.round(Theme.spaceXS / 2)
                        color: Theme.textDim
                    }
                    Rectangle {                       // cord stub
                        anchors.horizontalCenter: parent.horizontalCenter
                        y: Math.round(parent.height * 0.74)
                        width: Math.round(Theme.spaceXS / 3)
                        height: Math.round(parent.height * 0.18)
                        color: Theme.textDim
                    }
                }
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
