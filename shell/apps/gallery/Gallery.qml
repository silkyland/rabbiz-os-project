import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import Quickshell
import Quickshell.Io
import A21s.Theme
import A21s.Components as Components

ApplicationWindow {
    id: win

    title: "Gallery"
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    color: Theme.bg

    property string activeFilter: "all" // all | screenshots | wallpapers
    property var allImages: []
    property var visibleImages: []
    property int lightboxIndex: -1
    property bool confirmDelete: false
    property real zoomScale: 1.0
    property string initialOpenPath: Quickshell.env("A21S_GALLERY_OPEN") || ""
    property string toastText: ""

    readonly property string screenshotStatePath: {
        var rt = Quickshell.env("XDG_RUNTIME_DIR") || "/run/user/1001"
        return rt + "/a21s-screenshot.state"
    }

    function applyFilter() {
        var out = []
        for (var i = 0; i < allImages.length; ++i) {
            var item = allImages[i]
            if (activeFilter === "screenshots" && !item.isScreenshot)
                continue
            if (activeFilter === "wallpapers" && !item.isWallpaper)
                continue
            out.push(item)
        }
        visibleImages = out
    }

    function openLightboxAt(idx) {
        if (idx >= 0 && idx < visibleImages.length) {
            zoomScale = 1.0
            confirmDelete = false
            lightboxIndex = idx
        }
    }

    function closeLightbox() {
        confirmDelete = false
        zoomScale = 1.0
        lightboxIndex = -1
    }

    function reloadImages() {
        if (!scanProc.running)
            scanProc.running = true
    }

    function showToast(msg) {
        toastText = msg
        toastTimer.restart()
    }

    function captureNow() {
        if (!actionProc.running) {
            actionProc.command = ["a21s-screenshot", "--delay", "250"]
            actionProc.running = true
        }
    }

    function openKoko() {
        if (!actionProc.running) {
            actionProc.command = ["sh", "-c",
                "QT_QPA_PLATFORM=wayland QT_QUICK_BACKEND=software QT_QUICK_CONTROLS_MOBILE=1 koko /home/gamer/Pictures/Screenshots >/dev/null 2>&1 &"]
            actionProc.running = true
        }
    }

    function deleteCurrentImage() {
        if (lightboxIndex < 0 || lightboxIndex >= visibleImages.length)
            return
        var target = visibleImages[lightboxIndex]
        if (!target || target.isWallpaper) {
            showToast("Built-in wallpaper cannot be deleted")
            confirmDelete = false
            return
        }
        if (!deleteProc.running) {
            deleteProc.command = ["rm", "-f", target.path]
            deleteProc.running = true
        }
    }

    Component.onCompleted: reloadImages()

    Timer {
        id: toastTimer
        interval: 2600
        onTriggered: win.toastText = ""
    }

    FileView {
        id: shotStateWatcher
        path: win.screenshotStatePath
        watchChanges: true
        onFileChanged: reloadTimer.restart()
    }

    Timer {
        id: reloadTimer
        interval: 250
        onTriggered: win.reloadImages()
    }

    Process {
        id: scanProc
        command: ["sh", "-c",
            "mkdir -p /home/gamer/Pictures/Screenshots; "
          + "find /home/gamer/Pictures/Screenshots /home/gamer/Pictures /usr/share/a21s-shell/assets/wallpapers "
          + "-maxdepth 3 -type f \\( -iname '*.png' -o -iname '*.jpg' -o -iname '*.jpeg' -o -iname '*.webp' \\) "
          + "-printf '%T@|%p|%f|%TY-%Tm-%Td %TH:%TM\\n' 2>/dev/null | sort -rn"]
        stdout: StdioCollector {
            onStreamFinished: {
                var lines = (this.text || "").split("\n")
                var seen = {}
                var list = []
                for (var i = 0; i < lines.length; ++i) {
                    var line = lines[i].trim()
                    if (line.length === 0)
                        continue
                    var parts = line.split("|")
                    if (parts.length < 4)
                        continue
                    var path = parts[1]
                    if (seen[path])
                        continue
                    seen[path] = true
                    list.push({
                        path: path,
                        name: parts[2],
                        mtime: parts[3],
                        isScreenshot: path.indexOf("/Screenshots/") >= 0,
                        isWallpaper: path.indexOf("/wallpapers/") >= 0
                    })
                }
                win.allImages = list
                win.applyFilter()

                if (win.initialOpenPath.length > 0) {
                    var target = win.initialOpenPath
                    win.initialOpenPath = ""
                    for (var j = 0; j < win.visibleImages.length; ++j) {
                        if (win.visibleImages[j].path === target) {
                            win.openLightboxAt(j)
                            break
                        }
                    }
                }
            }
        }
    }

    Process {
        id: actionProc
        onExited: reloadTimer.restart()
    }

    Process {
        id: deleteProc
        onExited: {
            win.closeLightbox()
            win.showToast("Deleted image")
            win.reloadImages()
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Top Header
        Rectangle {
            Layout.fillWidth: true
            height: 112
            color: Theme.surface

            RowLayout {
                anchors {
                    fill: parent
                    leftMargin: Theme.spaceM
                    rightMargin: Theme.spaceM
                }
                spacing: Theme.spaceS

                Rectangle {
                    width: 68
                    height: 68
                    radius: Theme.radiusPill
                    color: Theme.surfaceHigh

                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: "arrow_back"
                        size: Theme.iconS
                        color: Theme.text
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: Qt.quit()
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2

                    Text {
                        text: "คลังภาพ · Gallery"
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontTitle
                        font.bold: true
                        color: Theme.text
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }

                    Text {
                        text: win.visibleImages.length + " รูปภาพ"
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontStatus
                        color: Theme.textDim
                    }
                }

                Rectangle {
                    implicitWidth: capRow.implicitWidth + 28
                    height: 60
                    radius: Theme.radiusPill
                    color: Theme.primary

                    RowLayout {
                        id: capRow
                        anchors.centerIn: parent
                        spacing: 8
                        Components.MaterialIcon {
                            name: "photo_camera"
                            size: 26
                            color: Theme.primaryContent
                        }
                        Text {
                            text: "แคปหน้าจอ"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontStatus
                            font.bold: true
                            color: Theme.primaryContent
                        }
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: win.captureNow()
                    }
                }

                Rectangle {
                    width: 60
                    height: 60
                    radius: Theme.radiusPill
                    color: Theme.surfaceHigh
                    border.width: 1
                    border.color: Theme.separator

                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: "open_in_new"
                        size: 26
                        color: Theme.textDim
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: win.openKoko()
                    }
                }
            }

            Rectangle {
                anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                height: 1
                color: Theme.separator
            }
        }

        // Filter Chips
        Row {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spaceM
            Layout.rightMargin: Theme.spaceM
            Layout.topMargin: Theme.spaceS
            Layout.bottomMargin: Theme.spaceS
            spacing: Theme.spaceS

            Repeater {
                model: [
                    { key: "all", label: "ทั้งหมด" },
                    { key: "screenshots", label: "ภาพหน้าจอ" },
                    { key: "wallpapers", label: "พื้นหลัง" }
                ]

                delegate: Rectangle {
                    required property var modelData
                    readonly property bool selected: win.activeFilter === modelData.key
                    implicitWidth: chipTxt.implicitWidth + 36
                    height: 54
                    radius: Theme.radiusPill
                    color: selected ? Theme.primary : Theme.surfaceHigh
                    border.width: selected ? 0 : 1
                    border.color: Theme.separator

                    Text {
                        id: chipTxt
                        anchors.centerIn: parent
                        text: modelData.label
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontCaption
                        font.bold: selected
                        color: selected ? Theme.primaryContent : Theme.textDim
                    }

                    MouseArea {
                        anchors.fill: parent
                        onClicked: {
                            win.activeFilter = modelData.key
                            win.applyFilter()
                        }
                    }
                }
            }
        }

        // 2-Column Image Grid
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            GridView {
                id: grid
                anchors {
                    fill: parent
                    leftMargin: Theme.spaceM
                    rightMargin: Theme.spaceM
                    bottomMargin: Theme.navZoneHeight
                }
                cellWidth: Math.floor(width / 2)
                cellHeight: Math.round(cellWidth * 1.48)
                clip: true
                model: win.visibleImages

                delegate: Item {
                    required property var modelData
                    required property int index
                    width: grid.cellWidth
                    height: grid.cellHeight

                    Rectangle {
                        anchors {
                            fill: parent
                            margins: 8
                        }
                        radius: Theme.radiusM
                        color: Theme.card
                        border.width: 1
                        border.color: Theme.separator
                        clip: true

                        Image {
                            anchors {
                                left: parent.left
                                right: parent.right
                                top: parent.top
                                bottom: footerMeta.top
                            }
                            source: "file://" + modelData.path
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            sourceSize.width: 360
                        }

                        Rectangle {
                            id: footerMeta
                            anchors {
                                left: parent.left
                                right: parent.right
                                bottom: parent.bottom
                            }
                            height: 76
                            color: "#E615171C"

                            ColumnLayout {
                                anchors {
                                    fill: parent
                                    leftMargin: 14
                                    rightMargin: 14
                                    topMargin: 8
                                    bottomMargin: 8
                                }
                                spacing: 2

                                Text {
                                    Layout.fillWidth: true
                                    text: modelData.name
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 19
                                    font.bold: true
                                    color: Theme.text
                                    elide: Text.ElideMiddle
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: modelData.mtime
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 17
                                    color: Theme.textDim
                                    elide: Text.ElideRight
                                }
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            onClicked: win.openLightboxAt(index)
                        }
                    }
                }
            }

            // Empty State
            Column {
                anchors.centerIn: parent
                spacing: Theme.spaceM
                visible: win.visibleImages.length === 0

                Components.MaterialIcon {
                    anchors.horizontalCenter: parent.horizontalCenter
                    name: "photo_library"
                    size: 72
                    color: Theme.textFaint
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "ยังไม่มีภาพในหมวดนี้"
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                    color: Theme.textDim
                }
            }
        }
    }

    // Lightbox Fullscreen Overlay
    Rectangle {
        id: lightbox
        anchors.fill: parent
        color: "#F5000000"
        visible: win.lightboxIndex >= 0 && win.lightboxIndex < win.visibleImages.length
        z: 100

        readonly property var currentItem: visible ? win.visibleImages[win.lightboxIndex] : null

        Flickable {
            id: zoomFlick
            anchors {
                left: parent.left
                right: parent.right
                top: lightboxTopBar.bottom
                bottom: lightboxBottomBar.top
            }
            contentWidth: Math.max(width, width * win.zoomScale)
            contentHeight: Math.max(height, height * win.zoomScale)
            clip: true
            interactive: win.zoomScale > 1.01

            Item {
                width: zoomFlick.contentWidth
                height: zoomFlick.contentHeight

                Image {
                    anchors.centerIn: parent
                    width: zoomFlick.width * win.zoomScale
                    height: zoomFlick.height * win.zoomScale
                    source: lightbox.currentItem ? ("file://" + lightbox.currentItem.path) : ""
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                }

                MouseArea {
                    anchors.fill: parent
                    onDoubleClicked: {
                        if (win.zoomScale > 1.1) {
                            win.zoomScale = 1.0
                            zoomFlick.contentX = 0
                            zoomFlick.contentY = 0
                        } else {
                            win.zoomScale = 2.2
                            zoomFlick.contentX = zoomFlick.width * 0.6
                            zoomFlick.contentY = zoomFlick.height * 0.6
                        }
                    }
                }
            }
        }

        // Top Bar
        Rectangle {
            id: lightboxTopBar
            anchors { left: parent.left; right: parent.right; top: parent.top }
            height: 104
            color: "#CC0B0C0E"

            RowLayout {
                anchors {
                    fill: parent
                    leftMargin: Theme.spaceM
                    rightMargin: Theme.spaceM
                }
                spacing: Theme.spaceS

                Rectangle {
                    width: 64
                    height: 64
                    radius: 32
                    color: Theme.surfaceHigh
                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: "close"
                        size: 32
                        color: Theme.text
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: win.closeLightbox()
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    Text {
                        Layout.fillWidth: true
                        text: lightbox.currentItem ? lightbox.currentItem.name : ""
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontCaption
                        font.bold: true
                        color: Theme.text
                        elide: Text.ElideMiddle
                    }
                    Text {
                        text: (win.lightboxIndex + 1) + " / " + win.visibleImages.length
                            + (lightbox.currentItem ? (" · " + lightbox.currentItem.mtime) : "")
                        font.family: Theme.fontFamily
                        font.pixelSize: 18
                        color: Theme.textDim
                    }
                }

                Rectangle {
                    visible: lightbox.currentItem !== null && !lightbox.currentItem.isWallpaper
                    implicitWidth: delRow.implicitWidth + 24
                    height: 58
                    radius: Theme.radiusPill
                    color: win.confirmDelete ? Theme.danger : Theme.surfaceHigh

                    RowLayout {
                        id: delRow
                        anchors.centerIn: parent
                        spacing: 6
                        Components.MaterialIcon {
                            name: "delete"
                            size: 26
                            color: Theme.text
                        }
                        Text {
                            visible: win.confirmDelete
                            text: "ยืนยันลบ?"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontStatus
                            font.bold: true
                            color: Theme.text
                        }
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: {
                            if (win.confirmDelete)
                                win.deleteCurrentImage()
                            else
                                win.confirmDelete = true
                        }
                    }
                }
            }
        }

        // Bottom Prev / Next Bar
        Rectangle {
            id: lightboxBottomBar
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
                bottomMargin: Theme.navZoneHeight
            }
            height: 92
            color: "#CC0B0C0E"

            RowLayout {
                anchors.centerIn: parent
                spacing: Theme.spaceL

                Rectangle {
                    width: 140
                    height: 60
                    radius: Theme.radiusPill
                    color: Theme.surfaceHigh
                    opacity: win.lightboxIndex > 0 ? 1.0 : 0.35
                    RowLayout {
                        anchors.centerIn: parent
                        spacing: 6
                        Components.MaterialIcon { name: "chevron_left"; size: 30; color: Theme.text }
                        Text { text: "ก่อนหน้า"; font.family: Theme.fontFamily; font.pixelSize: 20; color: Theme.text }
                    }
                    MouseArea {
                        anchors.fill: parent
                        enabled: win.lightboxIndex > 0
                        onClicked: win.openLightboxAt(win.lightboxIndex - 1)
                    }
                }

                Rectangle {
                    width: 140
                    height: 60
                    radius: Theme.radiusPill
                    color: Theme.surfaceHigh
                    opacity: win.lightboxIndex < win.visibleImages.length - 1 ? 1.0 : 0.35
                    RowLayout {
                        anchors.centerIn: parent
                        spacing: 6
                        Text { text: "ถัดไป"; font.family: Theme.fontFamily; font.pixelSize: 20; color: Theme.text }
                        Components.MaterialIcon { name: "chevron_right"; size: 30; color: Theme.text }
                    }
                    MouseArea {
                        anchors.fill: parent
                        enabled: win.lightboxIndex < win.visibleImages.length - 1
                        onClicked: win.openLightboxAt(win.lightboxIndex + 1)
                    }
                }
            }
        }
    }

    // Toast
    Rectangle {
        visible: win.toastText.length > 0
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: Theme.navZoneHeight + Theme.spaceM
        }
        width: toastLbl.implicitWidth + 40
        height: 54
        radius: Theme.radiusPill
        color: Theme.surfaceHigh
        border.width: 1
        border.color: Theme.primary
        z: 150
        Text {
            id: toastLbl
            anchors.centerIn: parent
            text: win.toastText
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontCaption
            color: Theme.text
        }
    }

    Components.AppGestureLayer {
        anchors.fill: parent
        z: 200
        onBackTriggered: {
            if (win.lightboxIndex >= 0)
                win.closeLightbox()
            else
                Qt.quit()
        }
    }
}
