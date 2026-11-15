import QtQuick
import QtQuick.Layouts
import QtQuick.Shapes
import Quickshell.Io
import Quickshell.Services.Pipewire
import A21s.Theme
import A21s.Services

// Quick-settings content for the shade: brightness slider, 4x2 tile
// grid, volume slider. Tiles without a backend yet (airplane, rotation
// lock, flashlight, do-not-disturb, mobile data) are marked `stub` —
// they render dimmed with "soon" and only pulse on tap.
Item {
    id: root

    implicitWidth: col.implicitWidth
    implicitHeight: col.implicitHeight

    readonly property var sink: Pipewire.defaultAudioSink
    readonly property var sinkAudio: root.sink ? root.sink.audio : null

    // Keeps Pipewire.defaultAudioSink bindings live.
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
            glyph: "🔆"
            interactive: Brightness.available
            disabledHint: "brightness unavailable"
            value: Brightness.level / 100
            onMoved: function(v) { Brightness.setLevel(Math.round(v * 100)) }
        }

        // --- Tiles -----------------------------------------------------
        GridLayout {
            Layout.fillWidth: true
            columns: 4
            columnSpacing: Theme.spaceS
            rowSpacing: Theme.spaceS

            QSTile {
                Layout.fillWidth: true
                glyph: "📶"
                label: "Wi-Fi"
                secondary: Network.wifiConnected ? Network.activeSsid
                         : Network.wifiEnabled ? "on" : "off"
                active: Network.wifiEnabled
                onTapped: Network.setWifiEnabled(!Network.wifiEnabled)
            }

            QSTile {
                id: btTile
                Layout.fillWidth: true
                label: "Bluetooth"
                secondary: Bluetooth.connected ? Bluetooth.connectedName
                         : Bluetooth.powered ? "on" : "off"
                active: Bluetooth.powered
                onTapped: Bluetooth.setPowered(!Bluetooth.powered)

                // Bluetooth rune drawn as a Shape (no emoji glyph exists).
                glyphDelegate: Shape {
                    width: Theme.iconS
                    height: Theme.iconS
                    ShapePath {
                        strokeWidth: 2.5
                        strokeColor: btTile.active ? Theme.tileOnText
                                                   : Theme.tileOffText
                        fillColor: "transparent"
                        capStyle: ShapePath.RoundCap
                        joinStyle: ShapePath.RoundJoin
                        PathSvg {
                            path: "M 18 4 L 27 11 L 18 18 L 27 25 L 18 32 "
                                + "M 18 4 L 18 32 "
                                + "M 18 18 L 9 11 "
                                + "M 18 18 L 9 25"
                        }
                    }
                }
            }

            QSTile {
                Layout.fillWidth: true
                glyph: "✈️"
                label: "Airplane"
                stub: true
                onTapped: {}
            }

            QSTile {
                Layout.fillWidth: true
                glyph: "🔃"
                label: "Rotation"
                stub: true
                onTapped: {}
            }

            QSTile {
                Layout.fillWidth: true
                glyph: "🔦"
                label: "Torch"
                stub: true
                onTapped: {}
            }

            QSTile {
                Layout.fillWidth: true
                glyph: "🌙"
                label: "DND"
                stub: true
                onTapped: {}
            }

            QSTile {
                Layout.fillWidth: true
                glyph: "🌐"
                label: "Data"
                stub: true
                onTapped: {}
            }

            QSTile {
                Layout.fillWidth: true
                glyph: "⚙️"
                label: "Settings"
                onTapped: {
                    if (!settingsProc.running)
                        settingsProc.running = true
                }
            }
        }

        // --- Volume ----------------------------------------------------
        QSSlider {
            Layout.fillWidth: true
            glyph: root.sinkAudio && root.sinkAudio.muted ? "🔇" : "🔊"
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
