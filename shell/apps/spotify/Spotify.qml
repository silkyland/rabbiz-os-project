import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import Quickshell
import Quickshell.Io
import A21s.Theme
import A21s.Services
import A21s.Components as Components

ApplicationWindow {
    id: win

    title: "Spotify"
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    color: "#121212"

    readonly property color spotifyGreen: "#1DB954"
    readonly property color spotifyGreenBright: "#1ED760"
    readonly property color cardBg: "#1E1E1E"
    readonly property color cardElevated: "#282828"
    readonly property color surfaceDark: "#181818"
    readonly property color chipBg: "#2A2A2A"

    property string activeTab: "home" // home | search | library
    property string homeFilter: "all" // all | radio | local
    property bool nowPlayingOpen: false

    property string connectName: "Galaxy A21s"
    property bool connectRunning: false
    property string connectPid: ""
    property string playerSource: ""
    property string playerStatus: "Idle"
    property string trackTitle: ""
    property string trackArtist: ""
    property string trackAlbum: ""
    property string trackArtUrl: ""
    property int currentIndex: -1
    property string audioSink: ""
    property string deviceIp: ""
    property bool authReady: false
    property real posSec: 0
    property real durSec: 0

    property var presetsList: []
    property var searchResults: []
    property bool searchLoading: false
    property string searchStatusText: ""
    property string toastMessage: ""

    readonly property var quickGenres: [
        { label: "Thai Hits",      query: "Thai pop hits",  accent: "#E13300" },
        { label: "Bodyslam",       query: "Bodyslam",       accent: "#8D67AB" },
        { label: "Three Man Down", query: "Three Man Down", accent: "#1E3264" },
        { label: "Nont Tanont",    query: "Nont Tanont",    accent: "#148A08" },
        { label: "Lo-Fi Beats",    query: "Lofi chill",     accent: "#0D73EC" },
        { label: "The Weeknd",     query: "The Weeknd",     accent: "#B02897" },
        { label: "Taylor Swift",   query: "Taylor Swift",   accent: "#BA5D07" },
        { label: "K-Pop",          query: "K-Pop hits",     accent: "#E8115B" }
    ]

    readonly property var visiblePresets: {
        var out = []
        for (var i = 0; i < presetsList.length; ++i) {
            var item = presetsList[i]
            if (homeFilter === "radio" && item.source !== "Live Radio")
                continue
            if (homeFilter === "local" && item.source !== "Local Audio")
                continue
            out.push(item)
        }
        return out
    }

    readonly property bool isPlaying: Media.isPlaying || win.playerStatus === "Playing"

    readonly property string effectiveTitle: {
        if (Media.hasRealPlayer && Media.title.length > 0 && Media.title !== "Galaxy A21s Music")
            return Media.title
        if (win.trackTitle.length > 0)
            return win.trackTitle
        return "Galaxy A21s Music"
    }

    readonly property string effectiveArtist: {
        if (Media.hasRealPlayer && Media.artist.length > 0 && Media.artist !== "Tap Play for audio or open Spotify")
            return Media.artist
        if (win.trackArtist.length > 0) {
            return win.trackAlbum.length > 0
                ? (win.trackArtist + " · " + win.trackAlbum)
                : win.trackArtist
        }
        return "Tap any track below or open Spotify Web"
    }

    readonly property string effectiveSource: {
        if (Media.ctlSource.length > 0)
            return Media.ctlSource
        if (win.playerSource.length > 0)
            return win.playerSource
        return win.connectRunning ? "Spotify Connect" : "A21s Speaker"
    }

    readonly property string effectiveArtUrl: {
        if (Media.artUrl.length > 0)
            return Media.artUrl
        return win.trackArtUrl
    }

    readonly property real effectiveProgress: {
        if (durSec > 0)
            return Math.max(0, Math.min(1, posSec / durSec))
        return Media.progress
    }

    function formatTime(secVal) {
        var s = Math.max(0, Math.floor(secVal || 0))
        var m = Math.floor(s / 60)
        var r = s % 60
        return m + ":" + (r < 10 ? "0" + r : String(r))
    }

    function presetAccent(idx) {
        var colors = ["#1DB954", "#0D73EC", "#8D67AB", "#E13300", "#148A08", "#B02897", "#BA5D07", "#1E3264"]
        return colors[((idx % colors.length) + colors.length) % colors.length]
    }

    function presetIcon(source) {
        return source === "Live Radio" ? "radio" : "graphic_eq"
    }

    function showToast(msg) {
        win.toastMessage = msg
        toastTimer.restart()
    }

    function refreshStatus() {
        if (!statusProc.running)
            statusProc.running = true
    }

    function runAction(args, toast) {
        if (toast && toast.length > 0)
            win.showToast(toast)
        if (!actionProc.running) {
            actionProc.command = ["a21s-spotify-ctl"].concat(args)
            actionProc.running = true
        }
    }

    function playPreset(idx, title) {
        win.runAction(["play-index", String(idx)], "กำลังเล่น " + title)
    }

    function playPreviewItem(item) {
        win.runAction(
            ["play-url", item.previewUrl, item.title, item.artist, item.album, item.artUrl],
            "กำลังเล่น " + item.title
        )
    }

    function togglePlayPause() {
        if (Media.useMpris)
            Media.togglePlaying()
        else
            win.runAction(["play-pause"], "")
    }

    function nextTrack() {
        if (Media.useMpris)
            Media.next()
        else
            win.runAction(["next"], "เพลงถัดไป")
    }

    function prevTrack() {
        if (Media.useMpris)
            Media.previous()
        else
            win.runAction(["prev"], "เพลงก่อนหน้า")
    }

    function searchMusic(q) {
        var trimmed = String(q || "").trim()
        if (!trimmed)
            return
        win.activeTab = "search"
        win.searchLoading = true
        win.searchStatusText = "กำลังค้นหา \"" + trimmed + "\" ..."
        searchProc.command = ["a21s-spotify-ctl", "search-json", trimmed]
        if (searchProc.running)
            searchProc.running = false
        searchProc.running = true
    }

    Component.onCompleted: {
        presetsProc.running = true
        win.runAction(["start-connect"], "")
        win.refreshStatus()
    }

    Timer {
        id: toastTimer
        interval: 2600
        onTriggered: win.toastMessage = ""
    }

    Timer {
        id: pollTimer
        interval: win.isPlaying ? 1500 : 4000
        running: true
        repeat: true
        onTriggered: win.refreshStatus()
    }

    Process {
        id: presetsProc
        command: ["a21s-spotify-ctl", "list-presets"]
        stdout: StdioCollector {
            onStreamFinished: {
                var lines = String(text || "").split("\n")
                var parsed = []
                for (var i = 0; i < lines.length; ++i) {
                    var ln = lines[i].trim()
                    if (!ln)
                        continue
                    var parts = ln.split("|")
                    if (parts.length < 7)
                        continue
                    parsed.push({
                        index: parseInt(parts[0], 10) || 0,
                        title: parts[1],
                        artist: parts[2],
                        album: parts[3],
                        artUrl: parts[4],
                        url: parts[5],
                        source: parts[6]
                    })
                }
                win.presetsList = parsed
            }
        }
    }

    Process {
        id: searchProc
        stdout: StdioCollector {
            onStreamFinished: {
                win.searchLoading = false
                var lines = String(text || "").split("\n")
                var out = []
                for (var i = 0; i < lines.length; ++i) {
                    var ln = lines[i].trim()
                    if (!ln)
                        continue
                    var parts = ln.split("|")
                    if (parts.length < 6)
                        continue
                    out.push({
                        index: parseInt(parts[0], 10) || 0,
                        title: parts[1],
                        artist: parts[2],
                        album: parts[3],
                        artUrl: parts[4],
                        previewUrl: parts[5]
                    })
                }
                win.searchResults = out
                win.searchStatusText = out.length > 0
                    ? ("พบ " + out.length + " เพลง (แตะเพื่อฟังตัวอย่างเสียงจริง)")
                    : "ไม่พบผลลัพธ์ ลองคำอื่นหรือตรวจสอบการเชื่อมต่อเน็ต"
            }
        }
        onExited: (exitCode) => {
            if (exitCode !== 0 && win.searchLoading) {
                win.searchLoading = false
                win.searchResults = []
                win.searchStatusText = "ค้นหาไม่สำเร็จ (ตรวจสอบ Wi-Fi/อินเทอร์เน็ต)"
            }
        }
    }

    Process {
        id: statusProc
        command: ["a21s-spotify-ctl", "status"]
        stdout: StdioCollector {
            onStreamFinished: {
                var lines = String(text || "").split("\n")
                for (var i = 0; i < lines.length; ++i) {
                    var line = lines[i].trim()
                    if (!line)
                        continue
                    var eq = line.indexOf("=")
                    if (eq <= 0)
                        continue
                    var k = line.slice(0, eq)
                    var v = line.slice(eq + 1)
                    if (k === "CONNECT_NAME") win.connectName = v
                    else if (k === "CONNECT_RUNNING") win.connectRunning = (v === "1")
                    else if (k === "CONNECT_PID") win.connectPid = v
                    else if (k === "PLAYER_SOURCE") win.playerSource = v
                    else if (k === "PLAYER_STATUS") win.playerStatus = v
                    else if (k === "TITLE") win.trackTitle = v
                    else if (k === "ARTIST") win.trackArtist = v
                    else if (k === "ALBUM") win.trackAlbum = v
                    else if (k === "ART_URL") win.trackArtUrl = v
                    else if (k === "SINK") win.audioSink = v
                    else if (k === "IP") win.deviceIp = v
                    else if (k === "AUTH_READY") win.authReady = (v === "1")
                    else if (k === "POS_SEC") win.posSec = parseFloat(v) || 0
                    else if (k === "DUR_SEC") win.durSec = parseFloat(v) || 0
                    else if (k === "INDEX") {
                        var parsedIdx = parseInt(v, 10)
                        win.currentIndex = isNaN(parsedIdx) ? -1 : parsedIdx
                    }
                }
            }
        }
    }

    Process {
        id: actionProc
        stdout: StdioCollector {
            onStreamFinished: {
                win.refreshStatus()
            }
        }
        onExited: win.refreshStatus()
    }

    // Subtle Spotify top ambiance gradient
    Rectangle {
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
        }
        height: 460
        gradient: Gradient {
            GradientStop { position: 0.0; color: win.activeTab === "home" ? "#1E3A2A" : (win.activeTab === "search" ? "#1F263B" : "#1D2C28") }
            GradientStop { position: 1.0; color: "#121212" }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Top App Header
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: 112

            RowLayout {
                anchors {
                    fill: parent
                    leftMargin: Theme.spaceM
                    rightMargin: Theme.spaceM
                    topMargin: Theme.spaceS
                }
                spacing: Theme.spaceS

                Rectangle {
                    width: 68
                    height: 68
                    radius: 34
                    color: "#22FFFFFF"
                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: "arrow_back"
                        size: 32
                        color: Theme.text
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: Qt.quit()
                    }
                }

                Rectangle {
                    width: 56
                    height: 56
                    radius: 28
                    color: win.spotifyGreen
                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: "graphic_eq"
                        size: 32
                        color: "#08120B"
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    Text {
                        text: win.activeTab === "home"
                            ? "Spotify"
                            : (win.activeTab === "search" ? "ค้นหาเพลง" : "Connect & Web")
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontTitle
                        font.bold: true
                        color: Theme.text
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    Text {
                        text: win.connectRunning
                            ? ("Connect พร้อมใช้ (" + win.connectName + ") · ลำโพง " + SystemControl.volumeLevel + "%")
                            : ("ลำโพงพร้อมใช้ " + SystemControl.volumeLevel + "%")
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontStatus
                        color: win.spotifyGreenBright
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }

                Rectangle {
                    height: 54
                    width: volPillRow.implicitWidth + 24
                    radius: Theme.radiusPill
                    color: win.chipBg

                    RowLayout {
                        id: volPillRow
                        anchors.centerIn: parent
                        spacing: 6
                        Components.MaterialIcon {
                            name: SystemControl.volumeMuted || SystemControl.volumeLevel === 0
                                ? "volume_off" : "volume_up"
                            size: 24
                            color: win.spotifyGreenBright
                        }
                        Text {
                            text: SystemControl.volumeMuted ? "Mute" : (SystemControl.volumeLevel + "%")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontStatus
                            font.bold: true
                            color: Theme.text
                        }
                    }

                    MouseArea {
                        anchors.fill: parent
                        onClicked: win.nowPlayingOpen = true
                    }
                }
            }
        }

        // Main Content Stack
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            // ================= HOME TAB =================
            Flickable {
                anchors.fill: parent
                visible: win.activeTab === "home"
                clip: true
                contentWidth: width
                contentHeight: homeCol.implicitHeight + 36
                boundsBehavior: Flickable.StopAtBounds

                ColumnLayout {
                    id: homeCol
                    width: parent.width - Theme.spaceM * 2
                    x: Theme.spaceM
                    spacing: Theme.spaceM

                    // Spotify header filter chips
                    Flickable {
                        Layout.fillWidth: true
                        Layout.preferredHeight: 62
                        contentWidth: chipRow.implicitWidth
                        contentHeight: 62
                        clip: true

                        Row {
                            id: chipRow
                            spacing: 12

                            Repeater {
                                model: [
                                    { id: "all",   label: "ทั้งหมด" },
                                    { id: "radio", label: "วิทยุสด" },
                                    { id: "local", label: "เสียงทดสอบเครื่อง" }
                                ]
                                delegate: Rectangle {
                                    height: 54
                                    width: fLbl.implicitWidth + 34
                                    radius: 27
                                    color: win.homeFilter === modelData.id ? win.spotifyGreen : win.chipBg
                                    Text {
                                        id: fLbl
                                        anchors.centerIn: parent
                                        text: modelData.label
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontStatus
                                        font.bold: true
                                        color: win.homeFilter === modelData.id ? "#08120B" : Theme.text
                                    }
                                    MouseArea {
                                        anchors.fill: parent
                                        onClicked: win.homeFilter = modelData.id
                                    }
                                }
                            }

                            Rectangle {
                                height: 54
                                width: webChipRow.implicitWidth + 30
                                radius: 27
                                color: "#203428"
                                border.width: 1
                                border.color: win.spotifyGreen

                                RowLayout {
                                    id: webChipRow
                                    anchors.centerIn: parent
                                    spacing: 8
                                    Components.MaterialIcon {
                                        name: "open_in_new"
                                        size: 22
                                        color: win.spotifyGreenBright
                                    }
                                    Text {
                                        text: "Spotify Web"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontStatus
                                        font.bold: true
                                        color: win.spotifyGreenBright
                                    }
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: win.runAction(["web-app"], "กำลังเปิด Spotify Web (Angelfish)...")
                                }
                            }
                        }
                    }

                    // Official Spotify Web Hero Card
                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: webHeroCol.implicitHeight + 36
                        radius: Theme.radiusM
                        color: win.cardBg
                        border.width: 1
                        border.color: "#2E4A36"

                        ColumnLayout {
                            id: webHeroCol
                            anchors {
                                fill: parent
                                margins: 18
                            }
                            spacing: 14

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 14

                                Rectangle {
                                    width: 72
                                    height: 72
                                    radius: 16
                                    color: win.spotifyGreen

                                    Components.MaterialIcon {
                                        anchors.centerIn: parent
                                        name: "language"
                                        size: 38
                                        color: "#08120B"
                                    }
                                }

                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 4
                                    Text {
                                        text: "Official Spotify App (Web Player)"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontBody
                                        font.bold: true
                                        color: Theme.text
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                    Text {
                                        text: "Spotify ไม่มีแพ็กเกจ .deb สำหรับ Linux ARM64 — เปิดหน้าเว็บจริงของ Spotify ผ่าน Angelfish เพื่อล็อกอินบัญชีและฟังเพลงเต็มทุกอัลบั้มได้ทันที"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontStatus
                                        color: Theme.textDim
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                }
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 12

                                Rectangle {
                                    Layout.fillWidth: true
                                    height: 68
                                    radius: 34
                                    color: win.spotifyGreen

                                    RowLayout {
                                        anchors.centerIn: parent
                                        spacing: 10
                                        Components.MaterialIcon {
                                            name: "play_circle"
                                            size: 28
                                            color: "#08120B"
                                        }
                                        Text {
                                            text: "เปิด open.spotify.com"
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontCaption
                                            font.bold: true
                                            color: "#08120B"
                                        }
                                    }

                                    MouseArea {
                                        anchors.fill: parent
                                        onClicked: win.runAction(["web-app"], "กำลังเปิด Official Spotify Web...")
                                    }
                                }

                                Rectangle {
                                    Layout.preferredWidth: 168
                                    height: 68
                                    radius: 34
                                    color: win.cardElevated

                                    RowLayout {
                                        anchors.centerIn: parent
                                        spacing: 8
                                        Components.MaterialIcon {
                                            name: "search"
                                            size: 24
                                            color: Theme.text
                                        }
                                        Text {
                                            text: "ค้นหาเพลง"
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontStatus
                                            font.bold: true
                                            color: Theme.text
                                        }
                                    }

                                    MouseArea {
                                        anchors.fill: parent
                                        onClicked: win.activeTab = "search"
                                    }
                                }
                            }
                        }
                    }

                    // 2-Column Quick Picks Grid (Play immediately out of speaker)
                    Text {
                        text: "กดเล่นออกลำโพงทันที"
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontBody
                        font.bold: true
                        color: Theme.text
                    }

                    GridLayout {
                        Layout.fillWidth: true
                        columns: 2
                        columnSpacing: 12
                        rowSpacing: 12

                        Repeater {
                            model: win.presetsList.slice(0, 4)
                            delegate: Rectangle {
                                readonly property bool activeItem: win.currentIndex === modelData.index && win.isPlaying
                                Layout.fillWidth: true
                                height: 104
                                radius: 14
                                color: activeItem ? "#213628" : win.cardBg
                                border.width: activeItem ? 2 : 0
                                border.color: win.spotifyGreen

                                RowLayout {
                                    anchors.fill: parent
                                    spacing: 12

                                    Rectangle {
                                        width: 104
                                        height: 104
                                        radius: 14
                                        color: win.presetAccent(modelData.index)

                                        Components.MaterialIcon {
                                            anchors.centerIn: parent
                                            name: activeItem ? "graphic_eq" : win.presetIcon(modelData.source)
                                            size: 38
                                            color: "#FFFFFF"
                                        }
                                    }

                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        Layout.rightMargin: 10
                                        spacing: 4
                                        Text {
                                            text: modelData.title
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontStatus
                                            font.bold: true
                                            color: activeItem ? win.spotifyGreenBright : Theme.text
                                            elide: Text.ElideRight
                                            maximumLineCount: 2
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                        Text {
                                            text: modelData.source
                                            font.family: Theme.fontFamily
                                            font.pixelSize: 17
                                            color: Theme.textDim
                                            elide: Text.ElideRight
                                            Layout.fillWidth: true
                                        }
                                    }
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: win.playPreset(modelData.index, modelData.title)
                                }
                            }
                        }
                    }

                    // Speaker Volume Quick Strip on Home
                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: homeVolCol.implicitHeight + 28
                        radius: Theme.radiusM
                        color: win.cardBg

                        ColumnLayout {
                            id: homeVolCol
                            anchors {
                                fill: parent
                                margins: 16
                            }
                            spacing: 10

                            RowLayout {
                                Layout.fillWidth: true
                                Text {
                                    text: "ระดับเสียงลำโพง (Speaker)"
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontCaption
                                    font.bold: true
                                    color: Theme.text
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: SystemControl.volumeMuted
                                        ? "ปิดเสียงอยู่"
                                        : (SystemControl.volumeLevel + "%")
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontCaption
                                    font.bold: true
                                    color: win.spotifyGreenBright
                                }
                            }

                            Components.QSSlider {
                                Layout.fillWidth: true
                                height: 68
                                interactive: true
                                materialIcon: SystemControl.volumeMuted || SystemControl.volumeLevel === 0
                                    ? "volume_off" : "volume_up"
                                value: SystemControl.volumeLevel / 100.0
                                onMoved: (v) => SystemControl.setVolume(Math.round(v * 100))
                                onIconTapped: SystemControl.toggleMute()
                            }
                        }
                    }

                    // Curated Stations & Sound Presets List
                    Text {
                        text: "สถานีเพลงสด & เสียงทดสอบ"
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontBody
                        font.bold: true
                        color: Theme.text
                    }

                    Repeater {
                        model: win.visiblePresets
                        delegate: Rectangle {
                            readonly property bool activeItem: win.currentIndex === modelData.index && win.isPlaying
                            Layout.fillWidth: true
                            height: 112
                            radius: Theme.radiusM
                            color: activeItem ? "#1F3025" : win.cardBg
                            border.width: activeItem ? 1 : 0
                            border.color: win.spotifyGreen

                            RowLayout {
                                anchors {
                                    fill: parent
                                    margins: 14
                                }
                                spacing: 14

                                Rectangle {
                                    width: 80
                                    height: 80
                                    radius: 14
                                    color: win.presetAccent(modelData.index)

                                    Components.MaterialIcon {
                                        anchors.centerIn: parent
                                        name: activeItem ? "graphic_eq" : win.presetIcon(modelData.source)
                                        size: 36
                                        color: "#FFFFFF"
                                    }
                                }

                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 4
                                    Text {
                                        text: modelData.title
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontCaption
                                        font.bold: true
                                        color: activeItem ? win.spotifyGreenBright : Theme.text
                                        elide: Text.ElideRight
                                        Layout.fillWidth: true
                                    }
                                    Text {
                                        text: modelData.artist + " · " + modelData.album
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontStatus
                                        color: Theme.textDim
                                        elide: Text.ElideRight
                                        Layout.fillWidth: true
                                    }
                                }

                                Rectangle {
                                    width: 60
                                    height: 60
                                    radius: 30
                                    color: activeItem ? win.spotifyGreen : win.cardElevated
                                    Components.MaterialIcon {
                                        anchors.centerIn: parent
                                        name: activeItem ? "pause" : "play_arrow"
                                        size: 32
                                        color: activeItem ? "#08120B" : Theme.text
                                    }
                                }
                            }

                            MouseArea {
                                anchors.fill: parent
                                onClicked: {
                                    if (activeItem)
                                        win.togglePlayPause()
                                    else
                                        win.playPreset(modelData.index, modelData.title)
                                }
                            }
                        }
                    }
                }
            }

            // ================= SEARCH TAB =================
            ColumnLayout {
                anchors {
                    fill: parent
                    leftMargin: Theme.spaceM
                    rightMargin: Theme.spaceM
                }
                visible: win.activeTab === "search"
                spacing: Theme.spaceS

                // Search Bar Row
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10

                    Rectangle {
                        Layout.fillWidth: true
                        height: 76
                        radius: 16
                        color: win.cardElevated

                        RowLayout {
                            anchors {
                                fill: parent
                                leftMargin: 16
                                rightMargin: 12
                            }
                            spacing: 10

                            Components.MaterialIcon {
                                name: "search"
                                size: 30
                                color: Theme.textDim
                            }

                            TextField {
                                id: searchInput
                                Layout.fillWidth: true
                                placeholderText: "พิมพ์ชื่อเพลง ศิลปิน หรือแนวเพลง..."
                                placeholderTextColor: Theme.textDim
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontCaption
                                background: Item {}
                                onAccepted: win.searchMusic(text)
                            }

                            Rectangle {
                                visible: searchInput.text.length > 0
                                width: 44
                                height: 44
                                radius: 22
                                color: win.chipBg
                                Components.MaterialIcon {
                                    anchors.centerIn: parent
                                    name: "close"
                                    size: 24
                                    color: Theme.text
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: {
                                        searchInput.text = ""
                                        win.searchResults = []
                                        win.searchStatusText = ""
                                    }
                                }
                            }
                        }
                    }

                    Rectangle {
                        width: 120
                        height: 76
                        radius: 16
                        color: win.spotifyGreen
                        Text {
                            anchors.centerIn: parent
                            text: "ค้นหา"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontCaption
                            font.bold: true
                            color: "#08120B"
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: win.searchMusic(searchInput.text || "Thai pop hits")
                        }
                    }
                }

                // Quick Genre / Artist Chips
                Flickable {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 62
                    contentWidth: genreChipRow.implicitWidth
                    contentHeight: 62
                    clip: true

                    Row {
                        id: genreChipRow
                        spacing: 10

                        Repeater {
                            model: win.quickGenres
                            delegate: Rectangle {
                                height: 52
                                width: gTxt.implicitWidth + 32
                                radius: 26
                                color: win.chipBg
                                border.width: 1
                                border.color: "#3A3A3F"

                                Text {
                                    id: gTxt
                                    anchors.centerIn: parent
                                    text: modelData.label
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontStatus
                                    color: Theme.text
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: {
                                        searchInput.text = modelData.query
                                        win.searchMusic(modelData.query)
                                    }
                                }
                            }
                        }
                    }
                }

                Text {
                    visible: win.searchStatusText.length > 0
                    text: win.searchStatusText
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontStatus
                    color: win.searchLoading ? win.spotifyGreenBright : Theme.textDim
                    Layout.fillWidth: true
                }

                // Results or Browse Categories
                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: 10
                    visible: win.searchResults.length > 0
                    model: win.searchResults

                    delegate: Rectangle {
                        readonly property bool isCurrent: win.effectiveTitle === modelData.title && win.isPlaying
                        width: ListView.view.width
                        height: 112
                        radius: Theme.radiusM
                        color: isCurrent ? "#1F3025" : win.cardBg
                        border.width: isCurrent ? 1 : 0
                        border.color: win.spotifyGreen

                        RowLayout {
                            anchors {
                                fill: parent
                                margins: 12
                            }
                            spacing: 14

                            Rectangle {
                                width: 84
                                height: 84
                                radius: 12
                                color: win.cardElevated
                                clip: true

                                Image {
                                    anchors.fill: parent
                                    source: modelData.artUrl
                                    fillMode: Image.PreserveAspectCrop
                                    asynchronous: true
                                    visible: status === Image.Ready
                                }

                                Components.MaterialIcon {
                                    anchors.centerIn: parent
                                    name: "music_note"
                                    size: 36
                                    color: win.spotifyGreenBright
                                    visible: !parent.children[0].visible
                                }
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 4
                                Text {
                                    text: modelData.title
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontCaption
                                    font.bold: true
                                    color: isCurrent ? win.spotifyGreenBright : Theme.text
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: modelData.artist + (modelData.album ? (" · " + modelData.album) : "")
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontStatus
                                    color: Theme.textDim
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                            }

                            Rectangle {
                                width: 62
                                height: 62
                                radius: 31
                                color: isCurrent ? win.spotifyGreen : win.cardElevated
                                Components.MaterialIcon {
                                    anchors.centerIn: parent
                                    name: isCurrent ? "graphic_eq" : "play_arrow"
                                    size: 32
                                    color: isCurrent ? "#08120B" : Theme.text
                                }
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            onClicked: win.playPreviewItem(modelData)
                        }
                    }
                }

                // Browse Category Tiles when no search has run yet
                Flickable {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: win.searchResults.length === 0 && !win.searchLoading
                    clip: true
                    contentWidth: width
                    contentHeight: browseGrid.implicitHeight + 24

                    GridLayout {
                        id: browseGrid
                        width: parent.width
                        columns: 2
                        columnSpacing: 12
                        rowSpacing: 12

                        Repeater {
                            model: win.quickGenres
                            delegate: Rectangle {
                                Layout.fillWidth: true
                                height: 128
                                radius: 16
                                color: modelData.accent

                                Text {
                                    anchors {
                                        left: parent.left
                                        top: parent.top
                                        right: parent.right
                                        margins: 18
                                    }
                                    text: modelData.label
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontBody
                                    font.bold: true
                                    color: "#FFFFFF"
                                    wrapMode: Text.Wrap
                                }

                                Components.MaterialIcon {
                                    anchors {
                                        right: parent.right
                                        bottom: parent.bottom
                                        margins: 14
                                    }
                                    name: "music_note"
                                    size: 40
                                    color: "#55FFFFFF"
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: {
                                        searchInput.text = modelData.query
                                        win.searchMusic(modelData.query)
                                    }
                                }
                            }
                        }
                    }
                }

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: win.searchLoading

                    ColumnLayout {
                        anchors.centerIn: parent
                        spacing: 12
                        BusyIndicator {
                            Layout.alignment: Qt.AlignHCenter
                            running: win.searchLoading
                        }
                        Text {
                            text: "กำลังโหลดรายชื่อเพลง..."
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontCaption
                            color: Theme.textDim
                        }
                    }
                }
            }

            // ================= LIBRARY / CONNECT & WEB TAB =================
            Flickable {
                anchors.fill: parent
                visible: win.activeTab === "library"
                clip: true
                contentWidth: width
                contentHeight: libCol.implicitHeight + 36
                boundsBehavior: Flickable.StopAtBounds

                ColumnLayout {
                    id: libCol
                    width: parent.width - Theme.spaceM * 2
                    x: Theme.spaceM
                    spacing: Theme.spaceM

                    // Official Spotify Web Card
                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: libWebCol.implicitHeight + 36
                        radius: Theme.radiusM
                        color: win.cardBg
                        border.width: 1
                        border.color: "#2E4A36"

                        ColumnLayout {
                            id: libWebCol
                            anchors {
                                fill: parent
                                margins: 18
                            }
                            spacing: 14

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 14
                                Rectangle {
                                    width: 68
                                    height: 68
                                    radius: 34
                                    color: win.spotifyGreen
                                    Components.MaterialIcon {
                                        anchors.centerIn: parent
                                        name: "open_in_browser"
                                        size: 34
                                        color: "#08120B"
                                    }
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 4
                                    Text {
                                        text: "Spotify Web Player (Angelfish)"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontBody
                                        font.bold: true
                                        color: Theme.text
                                    }
                                    Text {
                                        text: "ใช้งานบัญชี Spotify จริง ค้นหา Playlist / Liked Songs และสตรีมเต็มเพลงบนเบราว์เซอร์ QtWebEngine"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontStatus
                                        color: Theme.textDim
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                }
                            }

                            Rectangle {
                                Layout.fillWidth: true
                                height: 72
                                radius: 36
                                color: win.spotifyGreen
                                RowLayout {
                                    anchors.centerIn: parent
                                    spacing: 10
                                    Components.MaterialIcon {
                                        name: "launch"
                                        size: 28
                                        color: "#08120B"
                                    }
                                    Text {
                                        text: "เปิด Spotify Web (open.spotify.com)"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontCaption
                                        font.bold: true
                                        color: "#08120B"
                                    }
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: win.runAction(["web-app"], "กำลังเปิด Spotify Web...")
                                }
                            }
                        }
                    }

                    // Spotify Connect Card
                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: connectCardCol.implicitHeight + 36
                        radius: Theme.radiusM
                        color: win.cardBg

                        ColumnLayout {
                            id: connectCardCol
                            anchors {
                                fill: parent
                                margins: 18
                            }
                            spacing: 14

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 12

                                Components.MaterialIcon {
                                    name: "cast_connected"
                                    size: 36
                                    color: win.connectRunning ? win.spotifyGreen : Theme.textDim
                                }

                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 2
                                    Text {
                                        text: "Spotify Connect (librespot)"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontBody
                                        font.bold: true
                                        color: Theme.text
                                    }
                                    Text {
                                        text: win.connectRunning
                                            ? ("Active \"" + win.connectName + "\" · PID " + win.connectPid + (win.deviceIp ? (" · " + win.deviceIp) : ""))
                                            : "กด Start เพื่อส่งเสียงจากแอป Spotify ในมือถืออีกเครื่อง/PC มายัง A21s"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontStatus
                                        color: Theme.textDim
                                        wrapMode: Text.Wrap
                                        Layout.fillWidth: true
                                    }
                                }

                                Rectangle {
                                    height: 44
                                    width: stLbl.implicitWidth + 24
                                    radius: 22
                                    color: win.connectRunning ? "#1B3B28" : win.chipBg
                                    Text {
                                        id: stLbl
                                        anchors.centerIn: parent
                                        text: win.connectRunning ? "Active" : "Stopped"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: 18
                                        font.bold: true
                                        color: win.connectRunning ? win.spotifyGreenBright : Theme.textDim
                                    }
                                }
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 12

                                Rectangle {
                                    Layout.fillWidth: true
                                    height: 68
                                    radius: Theme.radiusM
                                    color: win.connectRunning ? win.cardElevated : win.spotifyGreen
                                    Text {
                                        anchors.centerIn: parent
                                        text: win.connectRunning ? "Restart Connect" : "Start Connect"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontCaption
                                        font.bold: true
                                        color: win.connectRunning ? Theme.text : "#08120B"
                                    }
                                    MouseArea {
                                        anchors.fill: parent
                                        onClicked: win.runAction(
                                            [win.connectRunning ? "restart-connect" : "start-connect"],
                                            win.connectRunning ? "กำลังรีสตาร์ท Spotify Connect..." : "กำลังเปิด Spotify Connect..."
                                        )
                                    }
                                }

                                Rectangle {
                                    Layout.fillWidth: true
                                    height: 68
                                    radius: Theme.radiusM
                                    color: win.cardElevated
                                    Text {
                                        anchors.centerIn: parent
                                        text: "Stop Connect"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontCaption
                                        color: Theme.textDim
                                    }
                                    MouseArea {
                                        anchors.fill: parent
                                        onClicked: win.runAction(["stop-connect"], "ปิด Spotify Connect แล้ว")
                                    }
                                }
                            }
                        }
                    }

                    // Terminal spotify_player + Speaker Check Card
                    Rectangle {
                        Layout.fillWidth: true
                        implicitHeight: tuiCardCol.implicitHeight + 36
                        radius: Theme.radiusM
                        color: win.cardBg

                        ColumnLayout {
                            id: tuiCardCol
                            anchors {
                                fill: parent
                                margins: 18
                            }
                            spacing: 14

                            Text {
                                text: "เครื่องมือเพิ่มเติม (Terminal Spotify & ทดสอบเสียง)"
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody
                                font.bold: true
                                color: Theme.text
                                wrapMode: Text.Wrap
                                Layout.fillWidth: true
                            }

                            Text {
                                text: win.audioSink
                                    ? ("Audio Output: " + win.audioSink)
                                    : "Audio Output: PipeWire / Galaxy A21s Speaker"
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontStatus
                                color: Theme.textDim
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }

                            Rectangle {
                                Layout.fillWidth: true
                                height: 70
                                radius: Theme.radiusM
                                color: win.cardElevated

                                RowLayout {
                                    anchors.centerIn: parent
                                    spacing: 12
                                    Components.MaterialIcon {
                                        name: "terminal"
                                        size: 30
                                        color: win.spotifyGreenBright
                                    }
                                    Text {
                                        text: win.authReady
                                            ? "เปิด Terminal Spotify (spotify_player · ล็อกอินแล้ว)"
                                            : "เปิด Terminal Spotify (spotify_player · สำหรับล็อกอิน)"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontStatus
                                        font.bold: true
                                        color: Theme.text
                                    }
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: win.runAction(["tui"], "กำลังเปิด spotify_player ใน Terminal...")
                                }
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 12

                                Rectangle {
                                    Layout.fillWidth: true
                                    height: 66
                                    radius: Theme.radiusM
                                    color: "#1B3B28"
                                    RowLayout {
                                        anchors.centerIn: parent
                                        spacing: 8
                                        Components.MaterialIcon {
                                            name: "volume_up"
                                            size: 26
                                            color: win.spotifyGreenBright
                                        }
                                        Text {
                                            text: "ทดสอบเสียงลำโพง"
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontStatus
                                            font.bold: true
                                            color: win.spotifyGreenBright
                                        }
                                    }
                                    MouseArea {
                                        anchors.fill: parent
                                        onClicked: win.playPreset(0, "เสียงทดสอบลำโพง 48kHz")
                                    }
                                }

                                Rectangle {
                                    Layout.fillWidth: true
                                    height: 66
                                    radius: Theme.radiusM
                                    color: "#331D20"
                                    RowLayout {
                                        anchors.centerIn: parent
                                        spacing: 8
                                        Components.MaterialIcon {
                                            name: "stop"
                                            size: 26
                                            color: Theme.danger
                                        }
                                        Text {
                                            text: "หยุดเล่นเสียง"
                                            font.family: Theme.fontFamily
                                            font.pixelSize: Theme.fontStatus
                                            font.bold: true
                                            color: Theme.danger
                                        }
                                    }
                                    MouseArea {
                                        anchors.fill: parent
                                        onClicked: win.runAction(["stop-player"], "หยุดการเล่นแล้ว")
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Persistent Mini-Player Bar
        Rectangle {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spaceS
            Layout.rightMargin: Theme.spaceS
            Layout.bottomMargin: 8
            Layout.preferredHeight: 96
            radius: 16
            color: win.isPlaying ? "#1D2C22" : "#242424"
            border.width: 1
            border.color: win.isPlaying ? "#2D523A" : "#323236"
            clip: true

            MouseArea {
                anchors.fill: parent
                onClicked: win.nowPlayingOpen = true
            }

            RowLayout {
                anchors {
                    fill: parent
                    leftMargin: 12
                    rightMargin: 12
                }
                spacing: 12

                Rectangle {
                    width: 68
                    height: 68
                    radius: 10
                    color: win.spotifyGreen
                    clip: true

                    Image {
                        anchors.fill: parent
                        source: win.effectiveArtUrl
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                        visible: win.effectiveArtUrl.length > 0 && status === Image.Ready
                    }

                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: win.isPlaying ? "graphic_eq" : "music_note"
                        size: 34
                        color: "#08120B"
                        visible: !parent.children[0].visible
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    Text {
                        text: win.effectiveTitle
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontCaption
                        font.bold: true
                        color: Theme.text
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    Text {
                        text: win.effectiveArtist
                        font.family: Theme.fontFamily
                        font.pixelSize: 18
                        color: Theme.textDim
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }

                Rectangle {
                    width: 60
                    height: 60
                    radius: 30
                    color: "transparent"
                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: SystemControl.volumeMuted ? "volume_off" : "volume_up"
                        size: 30
                        color: SystemControl.volumeMuted ? Theme.danger : Theme.text
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: SystemControl.toggleMute()
                    }
                }

                Rectangle {
                    width: 68
                    height: 68
                    radius: 34
                    color: win.spotifyGreen
                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: win.isPlaying ? "pause" : "play_arrow"
                        size: 36
                        color: "#08120B"
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: win.togglePlayPause()
                    }
                }
            }

            // Progress hairline
            Rectangle {
                anchors {
                    left: parent.left
                    right: parent.right
                    bottom: parent.bottom
                }
                height: 4
                color: "#33FFFFFF"

                Rectangle {
                    width: parent.width * win.effectiveProgress
                    height: parent.height
                    color: win.spotifyGreenBright
                }
            }
        }

        // Bottom 3-Tab Navigation Bar
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 92 + Theme.navZoneHeight
            color: "#141416"

            Rectangle {
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                }
                height: 1
                color: "#26262A"
            }

            RowLayout {
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                }
                height: 92
                spacing: 0

                Repeater {
                    model: [
                        { id: "home",    icon: "home",      label: "หน้าหลัก" },
                        { id: "search",  icon: "search",    label: "ค้นหา" },
                        { id: "library", icon: "podcasts",  label: "Web & Connect" }
                    ]
                    delegate: Item {
                        readonly property bool selected: win.activeTab === modelData.id
                        Layout.fillWidth: true
                        Layout.fillHeight: true

                        ColumnLayout {
                            anchors.centerIn: parent
                            spacing: 4
                            Components.MaterialIcon {
                                Layout.alignment: Qt.AlignHCenter
                                name: modelData.icon
                                size: 34
                                color: selected ? win.spotifyGreenBright : Theme.textDim
                            }
                            Text {
                                Layout.alignment: Qt.AlignHCenter
                                text: modelData.label
                                font.family: Theme.fontFamily
                                font.pixelSize: 18
                                font.bold: selected
                                color: selected ? win.spotifyGreenBright : Theme.textDim
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            onClicked: win.activeTab = modelData.id
                        }
                    }
                }
            }
        }
    }

    // ================= FULL-SCREEN NOW PLAYING SHEET =================
    Rectangle {
        id: nowPlayingSheet
        anchors.fill: parent
        z: 40
        visible: win.nowPlayingOpen
        color: "#101A14"

        gradient: Gradient {
            GradientStop { position: 0.0; color: "#1C3B28" }
            GradientStop { position: 0.5; color: "#121714" }
            GradientStop { position: 1.0; color: "#0B0C0E" }
        }

        ColumnLayout {
            anchors {
                fill: parent
                leftMargin: Theme.spaceL
                rightMargin: Theme.spaceL
                topMargin: Theme.spaceL
                bottomMargin: Theme.navZoneHeight + Theme.spaceL
            }
            spacing: Theme.spaceM

            // Sheet Top Bar
            RowLayout {
                Layout.fillWidth: true

                Rectangle {
                    width: 68
                    height: 68
                    radius: 34
                    color: "#22FFFFFF"
                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: "keyboard_arrow_down"
                        size: 38
                        color: Theme.text
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: win.nowPlayingOpen = false
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    Text {
                        Layout.alignment: Qt.AlignHCenter
                        text: "กำลังเล่นจาก"
                        font.family: Theme.fontFamily
                        font.pixelSize: 18
                        color: Theme.textDim
                    }
                    Text {
                        Layout.alignment: Qt.AlignHCenter
                        text: win.effectiveSource
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontStatus
                        font.bold: true
                        color: Theme.text
                    }
                }

                Rectangle {
                    width: 68
                    height: 68
                    radius: 34
                    color: "#22FFFFFF"
                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: "stop"
                        size: 30
                        color: Theme.text
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: win.runAction(["stop-player"], "หยุดเล่นเสียงแล้ว")
                    }
                }
            }

            Item { Layout.fillHeight: true }

            // Large Centered Album Art
            Rectangle {
                Layout.alignment: Qt.AlignHCenter
                width: 460
                height: 460
                radius: 28
                color: win.cardElevated
                border.width: 2
                border.color: win.isPlaying ? win.spotifyGreen : "#33FFFFFF"
                clip: true

                Image {
                    anchors.fill: parent
                    source: win.effectiveArtUrl
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    visible: win.effectiveArtUrl.length > 0 && status === Image.Ready
                }

                ColumnLayout {
                    anchors.centerIn: parent
                    spacing: 16
                    visible: !parent.children[0].visible

                    Rectangle {
                        Layout.alignment: Qt.AlignHCenter
                        width: 140
                        height: 140
                        radius: 70
                        color: win.spotifyGreen
                        Components.MaterialIcon {
                            anchors.centerIn: parent
                            name: win.isPlaying ? "graphic_eq" : "headphones"
                            size: 76
                            color: "#08120B"
                        }
                    }

                    Text {
                        Layout.alignment: Qt.AlignHCenter
                        text: win.isPlaying ? "SPEAKER ACTIVE" : "READY TO PLAY"
                        font.family: Theme.fontFamily
                        font.pixelSize: 20
                        font.bold: true
                        color: win.spotifyGreenBright
                    }
                }
            }

            // Title & Artist
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 6

                Text {
                    text: win.effectiveTitle
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontTitle
                    font.bold: true
                    color: Theme.text
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }

                Text {
                    text: win.effectiveArtist
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                    color: Theme.textDim
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
            }

            // Progress Bar
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 8

                Rectangle {
                    Layout.fillWidth: true
                    height: 8
                    radius: 4
                    color: "#33FFFFFF"

                    Rectangle {
                        width: parent.width * win.effectiveProgress
                        height: parent.height
                        radius: 4
                        color: win.spotifyGreenBright
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Text {
                        text: win.durSec > 0 ? win.formatTime(win.posSec) : (win.isPlaying ? "Live" : "0:00")
                        font.family: Theme.fontFamily
                        font.pixelSize: 18
                        color: Theme.textDim
                        Layout.fillWidth: true
                    }
                    Text {
                        text: win.durSec > 0 ? win.formatTime(win.durSec) : (win.isPlaying ? "Playing" : "--:--")
                        font.family: Theme.fontFamily
                        font.pixelSize: 18
                        color: Theme.textDim
                    }
                }
            }

            // Transport Controls
            RowLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignHCenter
                spacing: 36

                Item { Layout.fillWidth: true }

                Rectangle {
                    width: 84
                    height: 84
                    radius: 42
                    color: "#18FFFFFF"
                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: "skip_previous"
                        size: 46
                        color: Theme.text
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: win.prevTrack()
                    }
                }

                Rectangle {
                    width: 118
                    height: 118
                    radius: 59
                    color: "#FFFFFF"
                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: win.isPlaying ? "pause" : "play_arrow"
                        size: 62
                        color: "#08120B"
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: win.togglePlayPause()
                    }
                }

                Rectangle {
                    width: 84
                    height: 84
                    radius: 42
                    color: "#18FFFFFF"
                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: "skip_next"
                        size: 46
                        color: Theme.text
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: win.nextTrack()
                    }
                }

                Item { Layout.fillWidth: true }
            }

            // Speaker Volume Control (QSSlider + fine -/+ buttons)
            Rectangle {
                Layout.fillWidth: true
                implicitHeight: sheetVolCol.implicitHeight + 28
                radius: Theme.radiusM
                color: "#1F2622"

                ColumnLayout {
                    id: sheetVolCol
                    anchors {
                        fill: parent
                        margins: 16
                    }
                    spacing: 10

                    RowLayout {
                        Layout.fillWidth: true
                        Text {
                            text: "เสียงลำโพงเครื่อง"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontStatus
                            font.bold: true
                            color: Theme.text
                            Layout.fillWidth: true
                        }
                        Text {
                            text: SystemControl.volumeMuted ? "MUTED" : (SystemControl.volumeLevel + "%")
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontCaption
                            font.bold: true
                            color: SystemControl.volumeMuted ? Theme.danger : win.spotifyGreenBright
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 12

                        Rectangle {
                            width: 64
                            height: 64
                            radius: 32
                            color: win.cardElevated
                            Text {
                                anchors.centerIn: parent
                                text: "−"
                                font.family: Theme.fontFamily
                                font.pixelSize: 32
                                font.bold: true
                                color: Theme.text
                            }
                            MouseArea {
                                anchors.fill: parent
                                onClicked: SystemControl.adjustVolume(-5)
                            }
                        }

                        Components.QSSlider {
                            Layout.fillWidth: true
                            height: 68
                            interactive: true
                            materialIcon: SystemControl.volumeMuted || SystemControl.volumeLevel === 0
                                ? "volume_off" : "volume_up"
                            value: SystemControl.volumeLevel / 100.0
                            onMoved: (v) => SystemControl.setVolume(Math.round(v * 100))
                            onIconTapped: SystemControl.toggleMute()
                        }

                        Rectangle {
                            width: 64
                            height: 64
                            radius: 32
                            color: win.cardElevated
                            Text {
                                anchors.centerIn: parent
                                text: "+"
                                font.family: Theme.fontFamily
                                font.pixelSize: 32
                                font.bold: true
                                color: Theme.text
                            }
                            MouseArea {
                                anchors.fill: parent
                                onClicked: SystemControl.adjustVolume(5)
                            }
                        }
                    }
                }
            }

            Item { Layout.fillHeight: true }
        }
    }

    // Floating Feedback Toast
    Rectangle {
        visible: win.toastMessage.length > 0
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: Theme.navZoneHeight + 206
        }
        width: Math.min(parent.width - 48, toastText.implicitWidth + 44)
        height: 58
        radius: Theme.radiusPill
        color: "#1E2B22"
        border.width: 1
        border.color: win.spotifyGreen
        z: 60

        Text {
            id: toastText
            anchors.centerIn: parent
            text: win.toastMessage
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontStatus
            color: Theme.text
        }
    }

    Components.AppGestureLayer {
        anchors.fill: parent
        z: 100
        onBackTriggered: {
            if (win.nowPlayingOpen)
                win.nowPlayingOpen = false
            else
                Qt.quit()
        }
    }
}
