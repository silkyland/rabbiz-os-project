import Quickshell
import Quickshell.Wayland
import Quickshell.Services.UPower
import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services
import A21s.Widgets

// Pull-down notification / quick-settings shade.
//
// Mechanic chosen: RESIZE (not mask). implicitHeight is bound to
// ShellState.shadeProgress, so the layer-shell surface only ever covers
//   * collapsed -> Theme.shadeHandleZone px invisible input strip at the
//     very top of the screen (Android-style: drag down on the status bar
//     opens the shade; a tap on the strip also opens it), and
//   * expanded  -> the full screen.
// The panel itself is bottom-anchored inside the window, so it slides
// down from above the screen like a physical sheet (bottom edge first).
// Only the bottom `tailH` slice is visible while collapsed; that slice
// is kept transparent (opacity fades in with progress) so the strip is
// invisible and cannot occlude the StatusBar window beneath it.
PanelWindow {
    id: win

    anchors { top: true; left: true; right: true }
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.namespace: "a21s-shade"
    color: "transparent"

    // Full open height = screen height. Transiently 0 before the screen
    // object binds (re-evaluates when it appears); the clamp below keeps
    // the collapsed strip intact in that window.
    readonly property real fullH: win.screen ? win.screen.height : 0
    // Bottom slice of the panel: 48px sits under the NavBar zone, the
    // grab pill floats in the 32px above it. Transparent when collapsed.
    readonly property real tailH: Theme.shadeHandleZone + Theme.navZoneHeight

    implicitHeight: Theme.shadeHandleZone
                  + ShellState.shadeProgress * Math.max(0, fullH - Theme.shadeHandleZone)

    // Release-to-snap animation on the shared progress property.
    NumberAnimation {
        id: snap
        target: ShellState
        property: "shadeProgress"
        duration: Theme.animMed
        easing.type: Theme.animEase
    }

    function snapTo(v) {
        snap.to = v
        snap.restart()
    }

    Item {
        anchors.fill: parent
        clip: true

        // Shade drag / tap handling. Lies *beneath* the panel content, so
        // tiles, sliders and the notification list get their own input
        // first; only presses on gaps/handle areas land here — which is
        // exactly the Android "drag anywhere on the shade" behavior.
        MouseArea {
            id: dragArea
            anchors.fill: parent
            preventStealing: true

            property real pressY: 0
            property real pressProgress: 0

            onPressed: function(mouse) {
                snap.stop()
                pressY = mouse.y
                pressProgress = ShellState.shadeProgress
            }
            onPositionChanged: function(mouse) {
                if (!pressed)
                    return
                const dy = mouse.y - pressY
                const range = Math.max(1, win.fullH - Theme.shadeHandleZone)
                ShellState.shadeProgress = Math.max(0, Math.min(1,
                    pressProgress + dy / range))
            }
            onReleased: function(mouse) {
                const dy = mouse.y - pressY
                const tap = Math.abs(dy) < Theme.spaceS
                if (tap) {
                    if (!ShellState.shadeOpen)
                        win.snapTo(1)   // tap on collapsed strip -> open
                    else if (mouse.y > win.height - win.tailH)
                        win.snapTo(0)   // tap on bottom grab zone -> close
                } else {
                    win.snapTo(ShellState.shadeProgress > 0.5 ? 1 : 0)
                }
            }
        }

        // The sheet. Bottom-anchored => slides down from above the screen.
        Item {
            id: panel
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            height: win.fullH

            Rectangle {
                id: bgMain
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                }
                height: Math.max(0, panel.height - win.tailH)
                color: Theme.surface
            }

            // Continuation of the background over the handle strip.
            // Fades in with progress; invisible at rest.
            Rectangle {
                anchors {
                    left: parent.left
                    right: parent.right
                    top: bgMain.bottom
                }
                height: win.tailH
                color: Theme.surface
                opacity: ShellState.shadeProgress
            }

            ColumnLayout {
                anchors {
                    left: bgMain.left
                    right: bgMain.right
                    top: bgMain.top
                    bottom: bgMain.bottom
                    leftMargin: Theme.spaceM
                    rightMargin: Theme.spaceM
                    topMargin: Theme.statusBarHeight + Theme.spaceS
                    bottomMargin: Theme.spaceS
                }
                spacing: Theme.spaceM

                // Slim header: clock/date left, battery right (guarded —
                // UPower device may not exist until drivers land).
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceS

                    Text {
                        id: clockText
                        color: Theme.textDim
                        font.pixelSize: Theme.fontCaption
                        text: Qt.formatTime(new Date(), "hh:mm")
                              + "  ·  "
                              + Qt.formatDate(new Date(), "ddd d MMM")
                        Timer {
                            interval: 1000
                            running: true
                            repeat: true
                            onTriggered: clockText.text =
                                Qt.formatTime(new Date(), "hh:mm")
                                + "  ·  "
                                + Qt.formatDate(new Date(), "ddd d MMM")
                        }
                    }

                    Item { Layout.fillWidth: true }

                    Text {
                        visible: UPower.displayDevice
                                 && UPower.displayDevice.isLaptopBattery
                        color: Theme.textDim
                        font.pixelSize: Theme.fontCaption
                        text: UPower.displayDevice
                              ? Math.round(UPower.displayDevice.percentage * 100) + "%"
                              : ""
                    }

                    // Screenshot, Sleep, Lock & Power Menu pills in shade header
                    Rectangle {
                        width: 40; height: 40; radius: 20
                        color: Theme.surfaceHigh
                        border.width: 1; border.color: Qt.alpha(Theme.text, 0.10)
                        MaterialIcon {
                            anchors.centerIn: parent
                            name: "photo_camera"
                            size: 20; color: "#A855F7"
                        }
                        TapHandler {
                            onTapped: {
                                win.snapTo(0)
                                SystemControl.takeScreenshot(350)
                            }
                        }
                    }

                    Rectangle {
                        width: 40; height: 40; radius: 20
                        color: Theme.surfaceHigh
                        border.width: 1; border.color: Qt.alpha(Theme.text, 0.10)
                        MaterialIcon {
                            anchors.centerIn: parent
                            name: "bedtime"
                            size: 20; color: ShellPrefs.accentColor
                        }
                        TapHandler {
                            onTapped: SystemControl.sleep()
                        }
                    }

                    Rectangle {
                        width: 40; height: 40; radius: 20
                        color: Theme.surfaceHigh
                        border.width: 1; border.color: Qt.alpha(Theme.text, 0.10)
                        MaterialIcon {
                            anchors.centerIn: parent
                            name: "lock"
                            size: 20; color: Theme.textDim
                        }
                        TapHandler {
                            onTapped: SystemControl.lockScreen()
                        }
                    }

                    Rectangle {
                        width: 40; height: 40; radius: 20
                        color: Theme.surfaceHigh
                        border.width: 1; border.color: Qt.alpha(Theme.text, 0.10)
                        MaterialIcon {
                            anchors.centerIn: parent
                            name: "power_settings_new"
                            size: 20; color: Theme.danger
                        }
                        TapHandler {
                            onTapped: {
                                win.snapTo(0)
                                ShellState.powerMenuOpen = true
                            }
                        }
                    }
                }

                QuickSettings {
                    Layout.fillWidth: true
                }

                // Notification list — implemented by W6 as
                // shell/components/NotificationCenter.qml (plain Item API,
                // no required props). Missing file is expected until W6 lands.
                NotificationCenter {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                }
            }

            // Bottom grab zone: transparent hit area + pill that fades in.
            Item {
                anchors {
                    left: parent.left
                    right: parent.right
                    bottom: parent.bottom
                }
                height: win.tailH

                Rectangle {
                    anchors {
                        horizontalCenter: parent.horizontalCenter
                        top: parent.top
                        topMargin: Theme.spaceS
                    }
                    width: Theme.navPillWidth
                    height: Theme.navPillHeight
                    radius: Theme.radiusPill
                    color: Theme.textFaint
                    opacity: ShellState.shadeProgress
                }
            }
        }
    }
}
