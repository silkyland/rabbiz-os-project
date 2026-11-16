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
    WlrLayershell.layer: WlrLayer.Overlay
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

    Connections {
        target: ShellState
        function onShadeSnapRequested(value) { win.snapTo(value) }
    }

    // 0 = notifications, 1 = controls. Draggable (follows the finger) and animated on
    // tab taps / on release, so the page switch is not a hard snap any more.
    property real pageShift: ShellState.shadeMode === "settings" ? 1.0 : 0.0
    property bool pageDragging: false
    property real pageDragStart: 0.0
    Behavior on pageShift {
        enabled: !win.pageDragging
        NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutCubic }
    }
    function setPage(id) {
        ShellState.shadeMode = id
        win.pageShift = id === "settings" ? 1.0 : 0.0
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
            property real pressX: 0
            property real pressProgress: 0

            onPressed: function(mouse) {
                snap.stop()
                pressY = mouse.y
                pressX = mouse.x
                pressProgress = ShellState.shadeProgress
                win.pageDragging = false
                win.pageDragStart = win.pageShift
                if (ShellState.shadeProgress < 0.05) {
                    ShellState.shadeMode = mouse.x < win.width / 2 ? "notifications" : "settings"
                    win.pageShift = ShellState.shadeMode === "settings" ? 1.0 : 0.0
                }
            }
            onPositionChanged: function(mouse) {
                if (!pressed)
                    return
                const dy = mouse.y - pressY
                const dx = mouse.x - pressX
                // Decide the axis once: once the gesture is clearly sideways the page
                // follows the finger and the vertical progress is frozen.
                if (!win.pageDragging && Math.abs(dx) > 14 && Math.abs(dx) > Math.abs(dy) * 1.2)
                    win.pageDragging = true
                if (win.pageDragging) {
                    win.pageShift = Math.max(0, Math.min(1, win.pageDragStart - dx / win.width))
                    return
                }
                const range = Math.max(1, win.fullH - Theme.shadeHandleZone)
                ShellState.shadeProgress = Math.max(0, Math.min(1,
                    pressProgress + dy / range))
            }
            onReleased: function(mouse) {
                const dy = mouse.y - pressY
                const dx = mouse.x - pressX
                if (win.pageDragging) {
                    // past 35 % of the width (or a clear flick) commits the page
                    const target = (Math.abs(dx) > win.width * 0.35)
                        ? (dx < 0 ? 1.0 : 0.0)
                        : (win.pageShift > 0.5 ? 1.0 : 0.0)
                    win.pageDragging = false            // re-enable the Behavior
                    win.setPage(target >= 0.5 ? "settings" : "notifications")
                    win.snapTo(1)
                    return
                }
                const tap = Math.abs(dy) < Theme.spaceS
                if (tap) {
                    if (!ShellState.shadeOpen)
                        win.snapTo(1)   // tap on collapsed strip -> open
                    else if (mouse.y > win.height - win.tailH)
                        win.snapTo(0)   // tap on bottom grab zone -> close
                } else {
                    win.snapTo(ShellState.shadeProgress > 0.3 ? 1 : 0)
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
                color: Theme.glassHigh          // frosted sheet over the app/home
                border.width: 0
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
                color: Theme.glassHigh
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

                // Page switch. Android 12+ has no tabs, but on 720 px the two pages need
                // a visible affordance; tapping a tab and swiping sideways both work.
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceXS

                    Repeater {
                        model: [
                            { id: "notifications", label: "Notifications" },
                            { id: "settings", label: "Controls" }
                        ]
                        delegate: Rectangle {
                            required property var modelData
                            readonly property bool active: modelData.id === "settings"
                                ? win.pageShift > 0.5 : win.pageShift <= 0.5
                            Layout.fillWidth: true
                            implicitHeight: 34
                            radius: 17
                            color: active ? ShellPrefs.accentColor : Theme.surfaceHigh
                            border.width: active ? 0 : 1
                            border.color: Qt.alpha(Theme.text, 0.10)

                            Text {
                                anchors.centerIn: parent
                                text: modelData.label
                                color: parent.active ? ShellPrefs.onAccentColor : Theme.textDim
                                font.pixelSize: Theme.fontCaption
                                font.family: Theme.fontFamily
                                font.weight: Font.Medium
                            }
                            TapHandler {
                                onTapped: win.setPage(modelData.id)
                            }
                        }
                    }
                }

                // --- Pages: a real horizontal pager. Both pages sit side by side and the
                // strip follows the finger (pageShift 0 = notifications, 1 = controls); the
                // Behavior animates the settle on release and on tab taps.
                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true

                    Row {
                        id: pagesRow
                        width: parent.width * 2
                        height: parent.height
                        x: -win.pageShift * parent.width

                        ColumnLayout {
                            id: notifPage
                            width: parent.width / 2
                            height: parent.height
                            enabled: win.pageShift < 0.5
                            spacing: Theme.spaceM

                            // Compact Now Playing card (same Media service as the home deck).
                            Rectangle {
                                visible: Media.hasRealPlayer
                                Layout.fillWidth: true
                                implicitHeight: mediaRow.implicitHeight + 2 * Theme.spaceM
                                radius: Theme.radiusL
                                color: Qt.alpha(Theme.card, 0.92)
                                border.width: 1
                                border.color: Qt.alpha(Theme.text, 0.08)

                                RowLayout {
                                    id: mediaRow
                                    anchors {
                                        left: parent.left
                                        right: parent.right
                                        top: parent.top
                                        margins: Theme.spaceM
                                    }
                                    spacing: Theme.spaceM

                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 2
                                        Text {
                                            text: "NOW PLAYING"
                                            color: Theme.textDim
                                            font.pixelSize: Theme.fontCaption - 2
                                            font.family: Theme.fontFamily
                                            font.letterSpacing: 1.2
                                        }
                                        Text {
                                            Layout.fillWidth: true
                                            text: Media.title
                                            color: Theme.text
                                            font.pixelSize: Theme.fontBody
                                            font.family: Theme.fontFamily
                                            font.weight: Font.Medium
                                            elide: Text.ElideRight
                                        }
                                        Text {
                                            Layout.fillWidth: true
                                            text: Media.artist
                                            color: Theme.textDim
                                            font.pixelSize: Theme.fontCaption
                                            font.family: Theme.fontFamily
                                            elide: Text.ElideRight
                                        }
                                    }

                                    Rectangle {
                                        width: 44; height: 44; radius: Media.isPlaying ? 22 : 14
                                        color: Media.isPlaying ? ShellPrefs.accentColor : Theme.surfaceHigh
                                        border.width: Media.isPlaying ? 0 : 1
                                        border.color: Qt.alpha(Theme.text, 0.12)
                                        MaterialIcon {
                                            anchors.centerIn: parent
                                            name: Media.isPlaying ? "pause" : "play_arrow"
                                            size: 22
                                            color: Media.isPlaying ? ShellPrefs.onAccentColor : Theme.text
                                        }
                                        TapHandler { onTapped: Media.togglePlaying() }
                                    }
                                    Rectangle {
                                        width: 40; height: 40; radius: 20
                                        color: Theme.surfaceHigh
                                        MaterialIcon {
                                            anchors.centerIn: parent
                                            name: "skip_next"
                                            size: 20
                                            color: Theme.text
                                        }
                                        TapHandler { onTapped: Media.next() }
                                    }
                                }
                            }

                            NotificationCenter {
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                            }
                        }

                        Item {
                            id: ctrlPage
                            width: parent.width / 2
                            height: parent.height
                            enabled: win.pageShift >= 0.5

                            QuickSettings {
                                anchors {
                                    left: parent.left
                                    right: parent.right
                                    top: parent.top
                                }
                            }
                        }
                    }
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
