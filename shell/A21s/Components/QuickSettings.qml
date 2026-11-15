import QtQuick
import QtQuick.Layouts
import Quickshell.Io
import Quickshell.Services.Pipewire
import A21s.Theme
import A21s.Services

// Quick Settings: brightness slider, wired 2x4 Expressive pill tiles,
// compact MPRIS media player row, and volume slider.
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

        // --- 2-Column Expressive Pill Tiles -----------------------------
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
                secondary: SystemControl.airplaneMode ? "On" : "Off"
                active: SystemControl.airplaneMode
                onTapped: SystemControl.setAirplaneMode(!SystemControl.airplaneMode)
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: "screen_rotation"
                label: "Auto-rotate"
                secondary: SystemControl.autoRotate ? "Landscape" : "Portrait"
                active: SystemControl.autoRotate
                onTapped: SystemControl.toggleAutoRotate()
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: "flashlight_on"
                label: "Flashlight"
                secondary: SystemControl.flashlightOn
                    ? (SystemControl.flashlightHardware ? "LED On" : "Screen Torch")
                    : "Off"
                active: SystemControl.flashlightOn
                onTapped: SystemControl.setFlashlight(!SystemControl.flashlightOn)
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: "do_not_disturb_on"
                label: "Do Not Disturb"
                secondary: SystemControl.dndEnabled ? "Muted" : "Off"
                active: SystemControl.dndEnabled
                onTapped: SystemControl.setDndEnabled(!SystemControl.dndEnabled)
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: "signal_cellular_alt"
                label: "Mobile Data"
                secondary: SystemControl.airplaneMode ? "Airplane"
                         : (!SystemControl.mobileData ? "Off"
                            : (SystemControl.cpifLoaded || Telephony.modemAvailable
                                ? (ShellPrefs.carrierName + " · " + ShellPrefs.preferredNetworkMode)
                                : "4G Demo"))
                active: !SystemControl.airplaneMode && SystemControl.mobileData
                onTapped: SystemControl.toggleMobileData()
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: "settings"
                label: "Settings"
                secondary: SystemControl.cpuGovernor === "performance" ? "Perf Mode" : "Balanced"
                onTapped: {
                    ShellState.shadeProgress = 0
                    if (!settingsProc.running)
                        settingsProc.running = true
                }
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: "keyboard"
                label: "Keyboard"
                secondary: "Show / hide OSK"
                onTapped: SystemControl.toggleOsk()
            }

            QSTile {
                Layout.fillWidth: true
                materialIcon: "language"
                label: "Language"
                secondary: "EN ⇄ ไทย"
                onTapped: SystemControl.nextOskLanguage()
            }
        }

        // --- Compact Shade Media Player Row ----------------------------
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: 74
            radius: Theme.radiusM + 4
            color: Theme.card
            border.width: 1
            border.color: Media.isPlaying
                ? Qt.alpha(ShellPrefs.accentColor, 0.45)
                : Theme.separator
            Behavior on border.color { ColorAnimation { duration: Theme.animFast } }

            RowLayout {
                anchors {
                    fill: parent
                    leftMargin: Theme.spaceM
                    rightMargin: Theme.spaceM
                }
                spacing: Theme.spaceM

                Rectangle {
                    Layout.preferredWidth: 44
                    Layout.preferredHeight: 44
                    radius: 12
                    color: Qt.alpha(ShellPrefs.accentColor, 0.16)
                    clip: true

                    Image {
                        id: qsArt
                        anchors.fill: parent
                        source: Media.artUrl
                        fillMode: Image.PreserveAspectCrop
                        visible: status === Image.Ready
                    }
                    MaterialIcon {
                        anchors.centerIn: parent
                        visible: qsArt.status !== Image.Ready
                        name: "music_note"
                        size: 24
                        color: ShellPrefs.accentColor
                    }
                    TapHandler {
                        onTapped: {
                            ShellState.shadeProgress = 0
                            Media.openMusicApp()
                        }
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
                    TapHandler {
                        onTapped: {
                            ShellState.shadeProgress = 0
                            Media.openMusicApp()
                        }
                    }
                }

                Row {
                    spacing: 8
                    Rectangle {
                        width: 38; height: 38; radius: 19
                        color: Theme.surfaceHigh
                        MaterialIcon {
                            anchors.centerIn: parent
                            name: "skip_previous"
                            size: 20; color: Theme.text
                        }
                        TapHandler { onTapped: Media.previous() }
                    }
                    Rectangle {
                        width: 44; height: 44
                        radius: Media.isPlaying ? 12 : 22
                        color: Media.isPlaying ? ShellPrefs.accentColor : Theme.surfaceHigh
                        Behavior on radius { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }
                        Behavior on color { ColorAnimation { duration: Theme.animFast } }
                        MaterialIcon {
                            anchors.centerIn: parent
                            name: Media.isPlaying ? "pause" : "play_arrow"
                            size: 24
                            color: Media.isPlaying ? ShellPrefs.onAccentColor : Theme.text
                        }
                        TapHandler { onTapped: Media.togglePlaying() }
                    }
                    Rectangle {
                        width: 38; height: 38; radius: 19
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

        // --- Volume ----------------------------------------------------
        QSSlider {
            Layout.fillWidth: true
            materialIcon: (SystemControl.volumeMuted || SystemControl.volumeLevel === 0)
                ? "volume_off" : "volume_up"
            interactive: true
            disabledHint: "audio unavailable"
            value: SystemControl.volumeMuted ? 0 : (SystemControl.volumeLevel / 100)
            onMoved: function(v) {
                SystemControl.setVolume(Math.round(v * 100))
            }
            onIconTapped: {
                SystemControl.toggleMute()
            }
        }
    }
}
