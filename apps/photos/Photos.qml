import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets
import RabbizKit
import "PhotosLogic.js" as PhotosLogic

// Photos — in-house replacement for pix (docs/BASE-APPS.md).
// One `find` lists every image under ~/Pictures (recursive) and ~/Downloads; the grid
// renders thumbnails through Image.sourceSize, so there is no thumbnail cache to keep in
// sync with the folders. Tapping a tile opens a full-screen viewer: swipe or the arrows
// move between photos (wrapping at both ends), double-tap toggles 1x/2.5x, and dragging
// pans while zoomed.
ApplicationWindow {
    id: win
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    title: "Photos"
    color: Theme.bg
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontBody

    Binding { target: Theme; property: "light"; value: ShellPrefs.lightTheme }

    readonly property string home: Quickshell.env("HOME") || "/home/gamer"
    readonly property var photoRoots: [win.home + "/Pictures", win.home + "/Downloads"]
    // 3 columns on the 720 px handset, still readable on a tablet.
    readonly property int columns: Math.max(2, Math.min(5,
        Math.floor((win.width - Theme.spaceM * 2) / (Theme.iconL * 3))))
    readonly property real swipeThreshold: Theme.spaceXL * 1.5

    property var photos: []
    // Set by `rabbiz-photos <file>` (xdg-open / the screenshot toast): jump straight into the viewer on it.
    property string openPath: Quickshell.env("RABBIZ_PHOTOS_OPEN") || ""
    property bool scanning: false
    property string scanError: ""
    property int viewerIndex: -1
    property real zoom: PhotosLogic.ZOOM_MIN
    property real panX: 0
    property real panY: 0

    readonly property var current: (win.viewerIndex >= 0 && win.viewerIndex < win.photos.length)
        ? win.photos[win.viewerIndex] : null
    readonly property bool zoomed: PhotosLogic.isZoomed(win.zoom)
    // Pan room depends on the painted (aspect-fitted) size, which arrives with the image.
    readonly property real panLimitX: PhotosLogic.maxPan(photo.paintedWidth, stage.width, win.zoom)
    readonly property real panLimitY: PhotosLogic.maxPan(photo.paintedHeight, stage.height, win.zoom)
    // "Today, 14:41 · ~/Pictures/2024" — the date is missing only for a file with no mtime.
    readonly property string currentCaption: {
        if (!win.current) return ""
        var date = PhotosLogic.formatDate(win.current.mtime)
        var folder = PhotosLogic.displayFolder(win.current.dir, win.home)
        return date.length > 0 ? date + " · " + folder : folder
    }
    readonly property string emptyHint: win.scanError.length > 0
        ? "Could not read ~/Pictures or ~/Downloads. Tap Refresh to try again."
        : "Pictures in ~/Pictures and ~/Downloads show up here, newest first."

    // ---- file listing -------------------------------------------------
    function reload() {
        if (scanProc.running) return          // the scan in flight will land in a moment
        win.scanError = ""
        win.scanning = true
        scanProc.running = true
    }

    function applyFindOutput(text) {
        win.photos = PhotosLogic.parseFindOutput(text)
        if (win.openPath.length > 0) {
            for (var i = 0; i < win.photos.length; i++) {
                if (win.photos[i].path === win.openPath) { win.openViewer(i); break }
            }
            win.openPath = ""
        }
        if (win.viewerIndex >= win.photos.length)
            win.viewerIndex = win.photos.length - 1
        win.scanning = false
    }

    // ---- viewer -------------------------------------------------------
    function openViewer(index) {
        if (index < 0 || index >= win.photos.length) return
        win.viewerIndex = index
        win.zoom = PhotosLogic.ZOOM_MIN
        win.panX = 0
        win.panY = 0
    }

    function closeViewer() {
        win.viewerIndex = -1
        win.zoom = PhotosLogic.ZOOM_MIN
        win.panX = 0
        win.panY = 0
    }

    function step(delta) {
        if (win.photos.length === 0) return
        win.viewerIndex = delta < 0
            ? PhotosLogic.prevIndex(win.viewerIndex, win.photos.length)
            : PhotosLogic.nextIndex(win.viewerIndex, win.photos.length)
        win.zoom = PhotosLogic.ZOOM_MIN
        win.panX = 0
        win.panY = 0
    }

    function toggleZoom() {
        if (win.viewerIndex < 0) return
        var target = PhotosLogic.zoomTarget(win.zoom)
        win.zoom = target
        if (!PhotosLogic.isZoomed(target)) {
            win.panX = 0
            win.panY = 0
        }
    }

    function clampPanNow() {
        win.panX = PhotosLogic.clampPan(win.panX, win.panLimitX)
        win.panY = PhotosLogic.clampPan(win.panY, win.panLimitY)
    }

    Process {
        id: scanProc
        command: PhotosLogic.findArgs(win.photoRoots)
        stdout: StdioCollector {
            onStreamFinished: win.applyFindOutput(this.text)
        }
        stderr: StdioCollector {
            onStreamFinished: win.scanError = this.text.trim()
        }
    }

    // A find that never reports back must not leave the grid saying "Looking…" forever.
    Timer {
        id: scanGuard
        interval: 15000
        running: win.scanning
        onTriggered: win.scanning = false
    }

    Component.onCompleted: win.reload()

    // ---- grid ---------------------------------------------------------
    ColumnLayout {
        id: page
        anchors.fill: parent
        anchors.margins: Theme.spaceM
        anchors.bottomMargin: Theme.spaceM + Theme.navZoneHeight
        spacing: Theme.spaceS
        enabled: win.viewerIndex < 0

        AppHeader {
            Layout.fillWidth: true
            title: "Photos"
            subtitle: win.scanning ? "Looking for photos…"
                : win.photos.length > 0 ? PhotosLogic.formatCount(win.photos.length) + " · newest first"
                : "~/Pictures and ~/Downloads"
            IconButton {
                icon: "refresh"
                onClicked: win.reload()
            }
        }

        Item {
            id: gridArea
            Layout.fillWidth: true
            Layout.fillHeight: true

            GridView {
                id: grid
                anchors.fill: parent
                clip: true
                model: win.photos
                boundsBehavior: Flickable.StopAtBounds
                cellWidth: Math.floor(width / Math.max(1, win.columns))
                cellHeight: cellWidth
                cacheBuffer: cellHeight * 6

                delegate: Item {
                    id: tile
                    required property int index
                    required property var modelData
                    width: grid.cellWidth
                    height: grid.cellHeight

                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: Theme.spaceXS / 2
                        radius: Theme.radiusS
                        color: Theme.surfaceHigh
                        clip: true
                        scale: tileTap.pressed ? 0.96 : 1.0
                        Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                        // Placeholder under the image: a tile that is still decoding (or is
                        // an unreadable file) shows a picture glyph instead of a blank hole.
                        MaterialIcon {
                            anchors.centerIn: parent
                            name: "image"
                            size: Theme.iconM
                            color: Theme.textFaint
                        }
                        Image {
                            anchors.fill: parent
                            source: PhotosLogic.fileUrl(tile.modelData.path)
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            autoTransform: true
                            sourceSize.width: Math.max(Theme.iconM * 2, Math.round(grid.cellWidth * 1.5))
                        }
                        TapHandler { id: tileTap; onTapped: win.openViewer(tile.index) }
                    }
                }
            }

            Column {
                id: emptyBlock
                width: grid.width - Theme.spaceL * 2
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.verticalCenter: parent.verticalCenter
                spacing: Theme.spaceM
                visible: win.photos.length === 0

                EmptyState {
                    icon: "photo_library"
                    title: win.scanning ? "Looking for photos…" : "No photos yet"
                    hint: win.scanning ? "" : win.emptyHint
                }
                PrimaryButton {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: !win.scanning
                    text: "Refresh"
                    materialIcon: "refresh"
                    onClicked: win.reload()
                }
            }
        }
    }

    // ---- full-screen viewer -------------------------------------------
    Item {
        id: viewer
        anchors.fill: parent
        visible: win.viewerIndex >= 0
        focus: visible
        z: 100

        Keys.onPressed: function (event) {
            if (event.key === Qt.Key_Left) {
                win.step(-1)
                event.accepted = true
            } else if (event.key === Qt.Key_Right) {
                win.step(1)
                event.accepted = true
            } else if (event.key === Qt.Key_Escape) {
                win.closeViewer()
                event.accepted = true
            }
        }

        Rectangle {
            anchors.fill: parent
            color: Theme.bg
        }

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: Theme.spaceM
            anchors.bottomMargin: Theme.spaceM + Theme.navZoneHeight
            spacing: Theme.spaceS

            Item {
                Layout.fillWidth: true
                Layout.preferredHeight: closeButton.height

                PhotoButton {
                    id: closeButton
                    anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                    icon: "close"
                    onClicked: win.closeViewer()
                }
                Text {
                    anchors.centerIn: parent
                    text: (win.viewerIndex + 1) + " / " + win.photos.length
                    color: ShellPrefs.accentColor
                    font.pixelSize: Theme.fontCaption
                    font.family: Theme.fontFamily
                    font.weight: Font.Medium
                }
                PhotoButton {
                    anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                    icon: win.zoomed ? "zoom_out" : "zoom_in"
                    onClicked: win.toggleZoom()
                }
            }

            Item {
                id: stage
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true

                Image {
                    id: photo
                    x: win.panX
                    y: win.panY
                    width: stage.width
                    height: stage.height
                    scale: win.zoom
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    autoTransform: true
                    cache: false
                    source: win.current ? PhotosLogic.fileUrl(win.current.path) : ""
                    // Enough pixels for the 2.5x zoom without decoding a full 12 MP frame.
                    sourceSize.width: Math.max(Theme.iconApp, Math.round(stage.width * PhotosLogic.ZOOM_MAX))
                    Behavior on scale {
                        NumberAnimation { duration: Theme.animMed; easing.type: Theme.animEase }
                    }
                    onStatusChanged: if (photo.status === Image.Ready) win.clampPanNow()
                }

                Text {
                    anchors.centerIn: parent
                    visible: win.current !== null && photo.status === Image.Loading
                    text: "Loading…"
                    color: Theme.textDim
                    font.pixelSize: Theme.fontCaption
                    font.family: Theme.fontFamily
                }
                Text {
                    anchors.centerIn: parent
                    visible: photo.status === Image.Error
                    text: "This image can't be opened"
                    color: Theme.textDim
                    font.pixelSize: Theme.fontBody - 2
                    font.family: Theme.fontFamily
                    horizontalAlignment: Text.AlignHCenter
                    width: stage.width
                }

                MouseArea {
                    id: gestures
                    anchors.fill: parent
                    property real pressX: 0
                    property real pressY: 0
                    property real baseX: 0
                    property real baseY: 0
                    property real dragX: 0
                    property real dragY: 0
                    // Double-tap is detected from press timestamps instead of
                    // MouseArea.onDoubleClicked, which depends on Qt's synthesized
                    // double-click for touch input.
                    property real lastTapMs: 0
                    property real lastTapX: 0
                    property real lastTapY: 0

                    function doublePressed(mouse) {
                        var now = Date.now()
                        var same = Math.abs(mouse.x - gestures.lastTapX) < Theme.spaceXL
                            && Math.abs(mouse.y - gestures.lastTapY) < Theme.spaceXL
                        var twice = gestures.lastTapMs > 0 && (now - gestures.lastTapMs) < 350 && same
                        gestures.lastTapMs = twice ? 0 : now
                        gestures.lastTapX = mouse.x
                        gestures.lastTapY = mouse.y
                        return twice
                    }

                    onPressed: function (mouse) {
                        gestures.pressX = mouse.x
                        gestures.pressY = mouse.y
                        gestures.baseX = win.panX
                        gestures.baseY = win.panY
                        gestures.dragX = 0
                        gestures.dragY = 0
                        if (gestures.doublePressed(mouse)) win.toggleZoom()
                    }
                    onPositionChanged: function (mouse) {
                        if (!gestures.pressed) return
                        gestures.dragX = mouse.x - gestures.pressX
                        gestures.dragY = mouse.y - gestures.pressY
                        if (!win.zoomed) return
                        win.panX = PhotosLogic.clampPan(gestures.baseX + gestures.dragX, win.panLimitX)
                        win.panY = PhotosLogic.clampPan(gestures.baseY + gestures.dragY, win.panLimitY)
                    }
                    onReleased: function (mouse) {
                        if (win.zoomed) return
                        if (!PhotosLogic.isSwipe(gestures.dragX, gestures.dragY, win.swipeThreshold))
                            return
                        if (gestures.dragX < 0) win.step(1)
                        else win.step(-1)
                    }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                implicitHeight: info.implicitHeight + Theme.spaceM * 2
                radius: Theme.radiusL
                color: Theme.card
                border.width: 1
                border.color: Qt.alpha(Theme.text, 0.08)

                ColumnLayout {
                    id: info
                    anchors {
                        left: parent.left
                        right: parent.right
                        verticalCenter: parent.verticalCenter
                        leftMargin: Theme.spaceM
                        rightMargin: Theme.spaceM
                    }
                    spacing: Theme.spaceXS / 2

                    Text {
                        Layout.fillWidth: true
                        text: win.current ? win.current.name : ""
                        color: Theme.text
                        font.pixelSize: Theme.fontBody - 2
                        font.family: Theme.fontFamily
                        font.weight: Font.DemiBold
                        elide: Text.ElideMiddle
                    }
                    Text {
                        Layout.fillWidth: true
                        text: win.currentCaption
                        color: Theme.textDim
                        font.pixelSize: Theme.fontCaption
                        font.family: Theme.fontFamily
                        elide: Text.ElideMiddle
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.topMargin: Theme.spaceS
                        spacing: Theme.spaceS

                        PhotoButton {
                            icon: "chevron_left"
                            onClicked: win.step(-1)
                        }
                        Text {
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                            text: win.zoomed ? "2.5× · drag to pan" : "Swipe · double-tap to zoom"
                            color: Theme.textFaint
                            font.pixelSize: Theme.fontCaption - 2
                            font.family: Theme.fontFamily
                        }
                        PhotoButton {
                            icon: "chevron_right"
                            onClicked: win.step(1)
                        }
                    }
                }
            }
        }
    }
}
