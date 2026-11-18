import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services
import A21s.Widgets

// Pull-down notification / quick-settings shade.
//
// Two pages share one pager: Notifications (clock + category-grouped cards) and
// Controls (grouped quick settings). Dragging sideways anywhere on the sheet or
// tapping a page dot switches them; pulling down from the left/right half of the
// status bar opens on that page directly.
//
// Keep the GPU swapchain at a fixed size. Only the clipped content and input
// region follow the drag; resizing a KMS-backed buffer every frame exhausts CMA.
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

    implicitHeight: Math.max(Theme.shadeHandleZone, fullH)
    readonly property real visibleHeight: Theme.shadeHandleZone
                  + ShellState.shadeProgress * Math.max(0, fullH - Theme.shadeHandleZone)
    mask: Region { item: content }

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
        // Pulling down from the left/right half of the status bar (or `ipc shade
        // open <page>`) chooses the page while the sheet is still closed; do it
        // here too, otherwise pageShift keeps the last page after a manual switch.
        function onShadeModeChanged() {
            if (ShellState.shadeProgress < 0.05)
                win.setPage(ShellState.shadeMode)
        }
    }

    // 0 = notifications, 1 = controls. Draggable (follows the finger) and animated on
    // tab taps / on release, so the page switch is not a hard snap any more.
    property real pageShift: ShellState.shadeMode === "settings" ? 1.0 : 0.0
    property bool pageDragging: false

    IpcHandler {
        target: "shade"
        function open(page: string): void {
            win.setPage(page === "settings" ? "settings" : "notifications")
            win.snapTo(1)
        }
        function close(): void { win.snapTo(0) }
        function state(): string {
            return JSON.stringify({ progress: ShellState.shadeProgress, gpu: win.gpu,
                captured: backdrop.hasContent, sourceWidth: backdrop.sourceSize.width,
                sourceHeight: backdrop.sourceSize.height, windowHeight: win.height,
                visibleHeight: win.visibleHeight, page: win.pageShift,
                mode: ShellState.shadeMode, pageHeight: ctrlPage.height,
                controlsContent: quickControls.contentHeight,
                controlsScrolling: quickControls.scrolling })
        }
    }
    readonly property bool gpu: content.GraphicsInfo.api === GraphicsInfo.OpenGL

    // One fresh capture per opening, while the sheet is still invisible. A live
    // output capture here would include the shade and recursively blur itself.
    ScreencopyView {
        id: backdrop
        width: win.width
        height: win.fullH
        visible: false
        paintCursor: false
        live: false
        captureSource: win.gpu && ShellState.shadeProgress > 0 ? win.screen : null
    }
    Timer {
        id: captureDeadline
        interval: 250
        running: win.gpu && ShellState.shadeProgress > 0 && !backdrop.hasContent
    }
    Behavior on pageShift {
        enabled: !win.pageDragging
        NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutCubic }
    }
    function setPage(id) {
        ShellState.shadeMode = id
        win.pageShift = id === "settings" ? 1.0 : 0.0
    }

    Item {
        id: content
        width: win.width
        height: win.visibleHeight
        clip: true

        // Shade drag handling. Lies *beneath* the panel content, so tiles,
        // sliders and the notification list get their own input first; only
        // presses on gaps/handle areas land here — which is exactly the
        // Android "drag anywhere on the shade" behavior. Taps are inert: the
        // collapsed top strip is a pull handle only (owner request 2026-11-17),
        // so a stray tap at the top of the screen cannot pop the shade open.
        MouseArea {
            id: dragArea
            anchors.fill: parent
            preventStealing: true

            property real pressX: 0
            property real pressY: 0
            property real pressProgress: 0
            property bool dragging: false

            onPressed: function(mouse) {
                snap.stop()
                pressX = mouse.x
                pressY = mouse.y
                pressProgress = ShellState.shadeProgress
                dragging = false
            }
            onPositionChanged: function(mouse) {
                if (!pressed || win.pageDragging)
                    return
                // Vertical only: sideways page drags are owned by the DragHandler
                // below, which can steal the grab from tiles and sliders.
                if (!dragging) {
                    if (Math.abs(mouse.y - pressY) < Theme.spaceS)
                        return
                    dragging = true
                    if (ShellState.shadeProgress < 0.05) {
                        ShellState.shadeMode = pressX < win.width / 2 ? "notifications" : "settings"
                        win.pageShift = ShellState.shadeMode === "settings" ? 1.0 : 0.0
                    }
                }
                const range = Math.max(1, win.fullH - Theme.shadeHandleZone)
                ShellState.shadeProgress = Math.max(0, Math.min(1,
                    pressProgress + (mouse.y - pressY) / range))
            }
            onReleased: function(mouse) {
                if (win.pageDragging)
                    return
                if (dragging)
                    win.snapTo(ShellState.shadeProgress > 0.3 ? 1 : 0)
                else if (ShellState.shadeOpen && mouse.y > content.height - win.tailH)
                    win.snapTo(0)   // tap on bottom grab zone -> close
            }
        }

        // Horizontal pager. A DragHandler (not a MouseArea) so it can take the
        // grab from the tiles, sliders and cards underneath: only x-axis motion
        // activates it, so vertical drags still scroll the list, move the
        // sliders and pull the shade closed.
        DragHandler {
            id: pageDrag
            target: null
            dragThreshold: 12
            xAxis.enabled: true
            yAxis.enabled: false
            grabPermissions: PointerHandler.CanTakeOverFromAnything

            property real startShift: 0
            property real lastDx: 0

            onActiveChanged: {
                if (active) {
                    startShift = win.pageShift
                    lastDx = 0
                    win.pageDragging = true
                } else if (win.pageDragging) {
                    win.pageDragging = false            // re-enable the Behavior
                    // past 30 % of the width (or a clear flick) commits the page
                    const target = Math.abs(lastDx) > win.width * 0.3
                        ? (lastDx < 0 ? 1.0 : 0.0)
                        : (win.pageShift > 0.5 ? 1.0 : 0.0)
                    win.setPage(target >= 0.5 ? "settings" : "notifications")
                }
            }
            onTranslationChanged: {
                if (!active)
                    return
                lastDx = activeTranslation.x
                win.pageShift = Math.max(0, Math.min(1, startShift - lastDx / win.width))
            }
        }

        // The sheet. Bottom-anchored => slides down from above the screen.
        Item {
            id: panel
            opacity: !win.gpu || backdrop.hasContent || (!captureDeadline.running && ShellState.shadeProgress > 0) ? 1 : 0
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            height: win.fullH

            BackdropBlur {
                id: bgMain
                sourceItem: backdrop.hasContent ? backdrop : null
                // The captured screen stays fixed as the sheet slides down.
                sourceRect: Qt.rect(0, content.height - panel.height, width, height)
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                }
                height: Math.max(0, panel.height - win.tailH)
                color: gpu && backdrop.hasContent ? Theme.glassBlurHigh : Theme.glassHigh
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
                spacing: Theme.spaceS

                // Page dots. One UI has no tabs; two dots keep the second page
                // discoverable, and the whole 36x28 cell is the tap target
                // (swiping sideways anywhere on the sheet also switches pages).
                Row {
                    Layout.alignment: Qt.AlignHCenter
                    spacing: 0

                    Repeater {
                        model: ["notifications", "settings"]

                        delegate: Item {
                            id: dotCell
                            required property var modelData
                            width: 36
                            height: 28
                            readonly property bool active: (dotCell.modelData === "settings")
                                                            === (win.pageShift > 0.5)

                            Rectangle {
                                anchors.centerIn: parent
                                width: dotCell.active ? 26 : 10
                                height: 10
                                radius: height / 2
                                color: dotCell.active ? Theme.text : Qt.alpha(Theme.text, 0.26)

                                Behavior on width {
                                    NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutCubic }
                                }
                            }

                            TapHandler { onTapped: win.setPage(dotCell.modelData) }
                        }
                    }
                }

                // --- Pages: a real horizontal pager. Both pages sit side by side and the
                // strip follows the finger (pageShift 0 = notifications, 1 = controls); the
                // Behavior animates the settle on release and on dot taps.
                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true

                    Row {
                        id: pagesRow
                        width: parent.width * 2
                        height: parent.height
                        x: -win.pageShift * parent.width

                        Item {
                            id: notifPage
                            width: parent.width / 2
                            height: parent.height
                            enabled: win.pageShift < 0.5

                            NotificationCenter {
                                anchors.fill: parent
                            }
                        }

                        Item {
                            id: ctrlPage
                            width: parent.width / 2
                            height: parent.height
                            enabled: win.pageShift >= 0.5

                            QuickSettings {
                                id: quickControls
                                anchors.fill: parent
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
