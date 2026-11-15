import QtQuick
import QtQuick.Layouts
import Quickshell.Io
import Quickshell.Services.Pipewire
import A21s.Theme
import A21s.Services

// Stitch-designed Quick Settings: brightness slider, 2x4 Pixel-style pill
// tiles with MaterialIcons, and volume slider.
Item {
    id: root

    implicitWidth: col.implicitWidth
    implicitHeight: col.implicitHeight

    readonly property var sink: Pipewire.defaultAudioSink
    readonly property var sinkAudio: root.sink ? root.sink.audio : null

    PwObjectTracker {
        objects: root.sink ? [root.sink] : []
    }

    Process {
        id: settingsProc
        command: ["a21s-settings"]
    }

    ColumnLayout {
        id: col
        anchors.fill: parent
        spacing: Theme.spaceM

        // --- Brightness ------------------------------------------------
        QSSlider {
            Layout.fillWidth: true
            materialIcon: "brightness_medium"
            interactive: Brightness.available
            disabledHint: "brightness unavailable"
            value: Brightness.level / 100
            onMoved: function(v) { Brightness.setLevel(Math.round(v * 100)) }
        }

        // --- 2-Column Pill Tiles --------------------------------------
        GridLayout {
            Layout.fillWidth: true
            columns: 2
            columnSpacing: Theme.spaceS
            rowSpacing: Theme.spaceS

            QSTile {
                Layout.fillWidth: true
                materialIcon: Network.wifiEnabled ? "wifi" : "wifi_off"
                label: "Wi-Fi"
                secondary: Network.wifiConnected ? Network.activeSsid
                         : Network.wifiEnabled ? "On" : "Off"
                active: Network.wifiEnabled
                onTapped: Network.setWifiEnabled(!Network.wifiEnabled)
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: Bluetooth.powered ? "bluetooth" : "bluetooth_disabled"
                label: "Bluetooth"
                secondary: Bluetooth.connected ? Bluetooth.connectedName
                         : Bluetooth.powered ? "On" : "Off"
                active: Bluetooth.powered
                onTapped: Bluetooth.setPowered(!Bluetooth.powered)
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: "flight"
                label: "Airplane"
                stub: true
                onTapped: {}
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: "screen_rotation"
                label: "Auto-rotate"
                secondary: "Portrait"
                stub: true
                onTapped: {}
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: "flashlight_on"
                label: "Flashlight"
                stub: true
                onTapped: {}
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: "do_not_disturb_on"
                label: "Do Not Disturb"
                stub: true
                onTapped: {}
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: "signal_cellular_alt"
                label: "Mobile Data"
                stub: true
                onTapped: {}
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: "settings"
                label: "Settings"
                secondary: "System"
                onTapped: {
                    if (!settingsProc.running)
                        settingsProc.running = true
                }
            }
        }

        // --- Volume ----------------------------------------------------
        QSSlider {
            Layout.fillWidth: true
            materialIcon: root.sinkAudio && root.sinkAudio.muted ? "volume_off" : "volume_up"
            interactive: root.sinkAudio !== null
            disabledHint: "audio unavailable"
            value: root.sinkAudio ? Math.min(1, root.sinkAudio.volume) : 0
            onMoved: function(v) {
                if (root.sinkAudio)
                    root.sinkAudio.volume = v
            }
            onIconTapped: {
                if (root.sinkAudio)
                    root.sinkAudio.muted = !root.sinkAudio.muted
            }
        }
    }
}
