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
    color: "#0B0C0E"

    readonly property color spotifyGreen: "#1DB954"
    readonly property color spotifyGreenDim: "#14833B"
    readonly property color cardBg: "#15171C"
    readonly property color cardElevated: "#1E2028"

    property string connectName: "Galaxy A21s"
    property bool connectRunning: false
    property string connectPid: ""
    property string playerSource: ""
    property string playerStatus: "Idle"
    property string trackTitle: ""
    property string trackArtist: ""
    property string trackAlbum: ""
    property string trackArtUrl: ""
    property int volumePct: 50
    property bool audioMuted: false
    property string audioSink: ""
    property string deviceIp: ""
    property bool authReady: false
    property string toastMessage: ""
    property bool volumeDragging: false

    readonly property bool isPlaying: Media.hasRealPlayer
        ? Media.isPlaying
        : (win.playerStatus === "Playing")

    readonly property string effectiveTitle: {
        if (Media.hasRealPlayer && Media.title.length > 0)
            return Media.title
        if (win.trackTitle.length > 0)
            return win.trackTitle
        return win.connectRunning ? "Ready for Spotify Connect" : "Spotify Standby"
    }

    readonly property string effectiveArtist: {
        if (Media.hasRealPlayer && Media.artist.length > 0)
            return Media.artist
        if (win.trackArtist.length > 0) {
            return win.trackAlbum.length > 0
                ? (win.trackArtist + " · " + win.trackAlbum)
                : win.trackArtist
        }
        return win.connectRunning
            ? ("Open Spotify app on phone/PC and cast to \"" + win.connectName + "\"")
            : "Tap Start Receiver below or open Terminal Spotify"
    }

    readonly property string effectiveArtUrl: {
        if (Media.hasRealPlayer && Media.artUrl.length > 0)
            return Media.artUrl
        return win.trackArtUrl
    }

    readonly property real effectiveProgress: {
        if (Media.hasRealPlayer)
            return Media.progress
        return win.isPlaying ? 0.42 : 0.0
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

    Component.onCompleted: {
        win.runAction(["start-connect"], "")
        win.refreshStatus()
    }

    Timer {
        id: pollTimer
        interval: 2500
        running: true
        repeat: true
        onTriggered: win.refreshStatus()
    }

    Timer {
        id: toastTimer
        interval: 3200
        onTriggered: win.toastMessage = ""
    }

    Process {
        id: statusProc
        command: ["a21s-spotify-ctl", "status"]
        stdout: StdioCollector {
            onStreamFinished: {
                var lines = (this.text || "").split("\n")
                for (var i = 0; i < lines.length; ++i) {
                    var line = lines[i].trim()
                    if (line.length === 0)
                        continue
                    var eq = line.indexOf("=")
                    if (eq <= 0)
                        continue
                    var key = line.slice(0, eq)
                    var val = line.slice(eq + 1)
                    if (key === "CONNECT_NAME") win.connectName = val || "Galaxy A21s"
                    else if (key === "CONNECT_RUNNING") win.connectRunning = (val === "1")
                    else if (key === "CONNECT_PID") win.connectPid = val
                    else if (key === "PLAYER_SOURCE") win.playerSource = val
                    else if (key === "PLAYER_STATUS") win.playerStatus = val || "Idle"
                    else if (key === "TITLE") win.trackTitle = val
                    else if (key === "ARTIST") win.trackArtist = val
                    else if (key === "ALBUM") win.trackAlbum = val
                    else if (key === "ART_URL") win.trackArtUrl = val
                    else if (key === "VOLUME" && !win.volumeDragging) {
                        var parsed = parseInt(val, 10)
                        if (!isNaN(parsed))
                            win.volumePct = Math.max(0, Math.min(100, parsed))
                    }
                    else if (key === "MUTED") win.audioMuted = (val === "1")
                    else if (key === "SINK") win.audioSink = val
                    else if (key === "IP") win.deviceIp = val
                    else if (key === "AUTH_READY") win.authReady = (val === "1")
                }
            }
        }
    }

    Process {
        id: actionProc
        onExited: (code, status) => {
            delayedRefresh.restart()
        }
    }

    Timer {
        id: delayedRefresh
        interval: 300
        onTriggered: win.refreshStatus()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Top Header Bar
        Rectangle {
            Layout.fillWidth: true
            height: 112
            color: "#111317"

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
                    color: win.cardElevated

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

                Rectangle {
                    width: 60
                    height: 60
                    radius: Theme.radiusPill
                    color: win.spotifyGreen

                    Components.MaterialIcon {
                        anchors.centerIn: parent
                        name: "graphic_eq"
                        size: Theme.iconS
                        color: "#09120B"
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2

                    Text {
                        text: "Spotify"
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontTitle
                        font.bold: true
                        color: Theme.text
                    }

                    RowLayout {
                        spacing: 8
                        Rectangle {
                            width: 12
                            height: 12
                            radius: 6
                            color: win.connectRunning ? win.spotifyGreen : Theme.warn
                        }
                        Text {
                            text: win.connectRunning
                                ? (win.connectName + " · Connect Ready")
                                : "Connect Off"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontStatus
                            color: win.connectRunning ? win.spotifyGreen : Theme.textDim
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                    }
                }

                Rectangle {
                    implicitWidth: tuiLabelRow.implicitWidth + 32
                    height: 60
                    radius: Theme.radiusPill
                    color: win.cardElevated
                    border.width: 1
                    border.color: Theme.separator

                    RowLayout {
                        id: tuiLabelRow
                        anchors.centerIn: parent
                        spacing: 8

                        Components.MaterialIcon {
                            name: "terminal"
                            size: 28
                            color: win.spotifyGreen
                        }
                        Text {
                            text: "TUI"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontCaption
                            font.bold: true
                            color: Theme.text
                        }
                    }

                    MouseArea {
                        anchors.fill: parent
                        onClicked: win.runAction(["tui"], "Opening spotify_player terminal UI...")
                    }
                }
            }

            Rectangle {
                anchors {
                    left: parent.left
                    right: parent.right
                    bottom: parent.bottom
                }
                height: 1
                color: Theme.separator
            }
        }

        // Scrollable Main Body
        Flickable {
            id: bodyFlick
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: width
            contentHeight: bodyColumn.implicitHeight + Theme.spaceXL + 48
            clip: true
            boundsBehavior: Flickable.DragOverBounds

            ColumnLayout {
                id: bodyColumn
                width: bodyFlick.width - (Theme.spaceM * 2)
                anchors {
                    top: parent.top
                    topMargin: Theme.spaceM
                    horizontalCenter: parent.horizontalCenter
                }
                spacing: Theme.spaceM

                // 1. Now Playing Card
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: nowPlayingCol.implicitHeight + (Theme.spaceL * 2)
                    radius: Theme.radiusL
                    color: win.cardBg
                    border.width: 1
                    border.color: win.isPlaying ? win.spotifyGreenDim : Theme.separator

                    ColumnLayout {
                        id: nowPlayingCol
                        anchors {
                            left: parent.left
                            right: parent.right
                            top: parent.top
                            margins: Theme.spaceL
                        }
                        spacing: Theme.spaceM

                        // Source + Live State badge
                        RowLayout {
                            Layout.fillWidth: true

                            Rectangle {
                                implicitWidth: sourceBadgeRow.implicitWidth + 24
                                height: 40
                                radius: Theme.radiusPill
                                color: win.isPlaying ? "#132E1C" : win.cardElevated

                                RowLayout {
                                    id: sourceBadgeRow
                                    anchors.centerIn: parent
                                    spacing: 8
                                    Components.MaterialIcon {
                                        name: win.isPlaying ? "music_note" : "headphones"
                                        size: 22
                                        color: win.isPlaying ? win.spotifyGreen : Theme.textDim
                                    }
                                    Text {
                                        text: Media.hasRealPlayer
                                            ? ("MPRIS · " + (win.isPlaying ? "PLAYING" : win.playerStatus.toUpperCase()))
                                            : (win.connectRunning ? "SPOTIFY CONNECT" : "STANDBY")
                                        font.family: Theme.fontFamily
                                        font.pixelSize: 18
                                        font.bold: true
                                        color: win.isPlaying ? win.spotifyGreen : Theme.textDim
                                    }
                                }
                            }

                            Item { Layout.fillWidth: true }

                            Rectangle {
                                width: 48
                                height: 48
                                radius: Theme.radiusPill
                                color: win.cardElevated

                                Components.MaterialIcon {
                                    anchors.centerIn: parent
                                    name: "refresh"
                                    size: 26
                                    color: Theme.textDim
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: win.refreshStatus()
                                }
                            }
                        }

                        // Album Art / Disc Visual
                        Item {
                            Layout.alignment: Qt.AlignHCenter
                            width: 340
                            height: 340

                            Rectangle {
                                anchors.fill: parent
                                radius: win.effectiveArtUrl.length > 0 ? Theme.radiusL : 170
                                color: "#11161C"
                                border.width: 2
                                border.color: win.isPlaying ? win.spotifyGreen : Theme.separator
                                clip: true

                                Image {
                                    anchors.fill: parent
                                    source: win.effectiveArtUrl
                                    fillMode: Image.PreserveAspectCrop
                                    asynchronous: true
                                    visible: win.effectiveArtUrl.length > 0 && status === Image.Ready
                                }

                                // Fallback vinyl graphic when waiting or no artwork URL available
                                Item {
                                    anchors.fill: parent
                                    visible: win.effectiveArtUrl.length === 0

                                    Rectangle {
                                        anchors.centerIn: parent
                                        width: 270
                                        height: 270
                                        radius: 135
                                        color: "#171A21"
                                        border.width: 2
                                        border.color: "#252934"
                                    }
                                    Rectangle {
                                        anchors.centerIn: parent
                                        width: 190
                                        height: 190
                                        radius: 95
                                        color: "transparent"
                                        border.width: 1
                                        border.color: "#2C3240"
                                    }
                                    Rectangle {
                                        anchors.centerIn: parent
                                        width: 108
                                        height: 108
                                        radius: 54
                                        color: win.spotifyGreen

                                        Components.MaterialIcon {
                                            anchors.centerIn: parent
                                            name: "headphones"
                                            size: 54
                                            color: "#08120B"
                                        }
                                    }

                                    RotationAnimator on rotation {
                                        from: 0
                                        to: 360
                                        duration: 9000
                                        loops: Animation.Infinite
                                        running: win.isPlaying && win.effectiveArtUrl.length === 0
                                    }
                                }
                            }
                        }

                        // Track Title + Artist
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 6

                            Text {
                                Layout.fillWidth: true
                                text: win.effectiveTitle
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontTitle
                                font.bold: true
                                color: Theme.text
                                horizontalAlignment: Text.AlignHCenter
                                elide: Text.ElideRight
                                maximumLineCount: 2
                                wrapMode: Text.Wrap
                            }

                            Text {
                                Layout.fillWidth: true
                                text: win.effectiveArtist
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontCaption
                                color: Theme.textDim
                                horizontalAlignment: Text.AlignHCenter
                                elide: Text.ElideRight
                                maximumLineCount: 2
                                wrapMode: Text.Wrap
                            }
                        }

                        // Progress Bar
                        Rectangle {
                            Layout.fillWidth: true
                            height: 10
                            radius: 5
                            color: "#262933"

                            Rectangle {
                                width: Math.max(0, Math.min(parent.width, parent.width * win.effectiveProgress))
                                height: parent.height
                                radius: 5
                                color: win.spotifyGreen
                            }
                        }

                        // Transport Buttons
                        RowLayout {
                            Layout.alignment: Qt.AlignHCenter
                            spacing: Theme.spaceL

                            Rectangle {
                                width: 92
                                height: 92
                                radius: 46
                                color: win.cardElevated

                                Components.MaterialIcon {
                                    anchors.centerIn: parent
                                    name: "skip_previous"
                                    size: Theme.iconM
                                    color: Theme.text
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: {
                                        if (Media.hasRealPlayer)
                                            Media.previous()
                                        else
                                            win.runAction(["prev"], "Previous track")
                                    }
                                }
                            }

                            Rectangle {
                                width: 118
                                height: 118
                                radius: 59
                                color: win.spotifyGreen

                                Components.MaterialIcon {
                                    anchors.centerIn: parent
                                    name: win.isPlaying ? "pause" : "play_arrow"
                                    size: 62
                                    color: "#09120B"
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: {
                                        if (Media.hasRealPlayer)
                                            Media.togglePlaying()
                                        else
                                            win.runAction(["play-pause"], win.isPlaying ? "Paused" : "Resumed")
                                    }
                                }
                            }

                            Rectangle {
                                width: 92
                                height: 92
                                radius: 46
                                color: win.cardElevated

                                Components.MaterialIcon {
                                    anchors.centerIn: parent
                                    name: "skip_next"
                                    size: Theme.iconM
                                    color: Theme.text
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: {
                                        if (Media.hasRealPlayer)
                                            Media.next()
                                        else
                                            win.runAction(["next"], "Next track")
                                    }
                                }
                            }
                        }
                    }
                }

                // 2. Output Volume Card
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: volCol.implicitHeight + (Theme.spaceM * 2)
                    radius: Theme.radiusL
                    color: win.cardBg
                    border.width: 1
                    border.color: Theme.separator

                    ColumnLayout {
                        id: volCol
                        anchors {
                            left: parent.left
                            right: parent.right
                            top: parent.top
                            margins: Theme.spaceM
                        }
                        spacing: Theme.spaceS

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spaceS

                            Components.MaterialIcon {
                                name: "volume_up"
                                size: Theme.iconS
                                color: win.spotifyGreen
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Text {
                                    text: "PipeWire Output Volume"
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontBody
                                    font.bold: true
                                    color: Theme.text
                                }
                                Text {
                                    text: win.audioSink.length > 0 ? win.audioSink : "Default Audio Sink"
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontStatus
                                    color: Theme.textDim
                                    elide: Text.ElideMiddle
                                    Layout.fillWidth: true
                                }
                            }

                            Text {
                                text: win.volumePct + "%"
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody
                                font.bold: true
                                color: win.spotifyGreen
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spaceS

                            Rectangle {
                                width: 64
                                height: 56
                                radius: Theme.radiusM
                                color: win.cardElevated
                                Text {
                                    anchors.centerIn: parent
                                    text: "-5"
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontCaption
                                    font.bold: true
                                    color: Theme.text
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: {
                                        win.volumePct = Math.max(0, win.volumePct - 5)
                                        win.runAction(["volume", String(win.volumePct)], "")
                                    }
                                }
                            }

                            Slider {
                                id: volumeSlider
                                Layout.fillWidth: true
                                height: 56
                                from: 0
                                to: 100
                                stepSize: 1
                                value: win.volumePct
                                onPressedChanged: {
                                    win.volumeDragging = pressed
                                    if (!pressed) {
                                        var target = Math.round(value)
                                        win.volumePct = target
                                        win.runAction(["volume", String(target)], "")
                                    }
                                }
                                onMoved: {
                                    win.volumePct = Math.round(value)
                                }

                                background: Rectangle {
                                    x: volumeSlider.leftPadding
                                    y: volumeSlider.topPadding + volumeSlider.availableHeight / 2 - height / 2
                                    implicitWidth: 200
                                    implicitHeight: 12
                                    width: volumeSlider.availableWidth
                                    height: implicitHeight
                                    radius: 6
                                    color: "#262933"

                                    Rectangle {
                                        width: volumeSlider.visualPosition * parent.width
                                        height: parent.height
                                        radius: 6
                                        color: win.spotifyGreen
                                    }
                                }

                                handle: Rectangle {
                                    x: volumeSlider.leftPadding + volumeSlider.visualPosition * (volumeSlider.availableWidth - width)
                                    y: volumeSlider.topPadding + volumeSlider.availableHeight / 2 - height / 2
                                    implicitWidth: 34
                                    implicitHeight: 34
                                    radius: 17
                                    color: "#FFFFFF"
                                    border.width: 3
                                    border.color: win.spotifyGreen
                                }
                            }

                            Rectangle {
                                width: 64
                                height: 56
                                radius: Theme.radiusM
                                color: win.cardElevated
                                Text {
                                    anchors.centerIn: parent
                                    text: "+5"
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontCaption
                                    font.bold: true
                                    color: Theme.text
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: {
                                        win.volumePct = Math.min(100, win.volumePct + 5)
                                        win.runAction(["volume", String(win.volumePct)], "")
                                    }
                                }
                            }
                        }
                    }
                }

                // 3. Spotify Connect Receiver Card (librespot)
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: connectCol.implicitHeight + (Theme.spaceM * 2)
                    radius: Theme.radiusL
                    color: win.cardBg
                    border.width: 1
                    border.color: Theme.separator

                    ColumnLayout {
                        id: connectCol
                        anchors {
                            left: parent.left
                            right: parent.right
                            top: parent.top
                            margins: Theme.spaceM
                        }
                        spacing: Theme.spaceS

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spaceS

                            Components.MaterialIcon {
                                name: "cast"
                                size: Theme.iconS
                                color: win.connectRunning ? win.spotifyGreen : Theme.textDim
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Text {
                                    text: "Spotify Connect Receiver"
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontBody
                                    font.bold: true
                                    color: Theme.text
                                }
                                Text {
                                    text: (win.connectRunning ? ("Running (PID " + win.connectPid + ")") : "Stopped")
                                        + (win.deviceIp.length > 0 ? (" · Wi-Fi " + win.deviceIp) : "")
                                        + " · 320 kbps"
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontStatus
                                    color: Theme.textDim
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                            }
                        }

                        Text {
                            Layout.fillWidth: true
                            text: "Zero-login casting: connect phone/PC and A21s to the same Wi-Fi, open Spotify app, choose Devices -> \"" + win.connectName + "\"."
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontStatus
                            color: Theme.textDim
                            wrapMode: Text.Wrap
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spaceS

                            Rectangle {
                                Layout.fillWidth: true
                                height: 74
                                radius: Theme.radiusM
                                color: win.connectRunning ? win.cardElevated : win.spotifyGreen

                                RowLayout {
                                    anchors.centerIn: parent
                                    spacing: 10
                                    Components.MaterialIcon {
                                        name: win.connectRunning ? "refresh" : "play_arrow"
                                        size: 32
                                        color: win.connectRunning ? Theme.text : "#08120B"
                                    }
                                    Text {
                                        text: win.connectRunning ? "Restart Receiver" : "Start Receiver"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontCaption
                                        font.bold: true
                                        color: win.connectRunning ? Theme.text : "#08120B"
                                    }
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    onClicked: win.runAction(
                                        [win.connectRunning ? "restart-connect" : "start-connect"],
                                        win.connectRunning ? "Restarting Spotify Connect..." : "Starting Spotify Connect..."
                                    )
                                }
                            }

                            Rectangle {
                                Layout.preferredWidth: 168
                                height: 74
                                radius: Theme.radiusM
                                color: win.cardElevated
                                opacity: win.connectRunning ? 1.0 : 0.45

                                RowLayout {
                                    anchors.centerIn: parent
                                    spacing: 8
                                    Components.MaterialIcon {
                                        name: "stop"
                                        size: 30
                                        color: Theme.danger
                                    }
                                    Text {
                                        text: "Stop"
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontCaption
                                        font.bold: true
                                        color: Theme.text
                                    }
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    enabled: win.connectRunning
                                    onClicked: win.runAction(["stop-connect"], "Stopped Spotify Connect")
                                }
                            }
                        }
                    }
                }

                // 4. Native Account Player Card (spotify_player)
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: nativeCol.implicitHeight + (Theme.spaceM * 2)
                    radius: Theme.radiusL
                    color: win.cardBg
                    border.width: 1
                    border.color: Theme.separator

                    ColumnLayout {
                        id: nativeCol
                        anchors {
                            left: parent.left
                            right: parent.right
                            top: parent.top
                            margins: Theme.spaceM
                        }
                        spacing: Theme.spaceS

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spaceS

                            Components.MaterialIcon {
                                name: win.authReady ? "check_circle" : "terminal"
                                size: Theme.iconS
                                color: win.authReady ? win.spotifyGreen : Theme.warn
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 2
                                Text {
                                    text: "Native Library & Search (spotify_player)"
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontBody
                                    font.bold: true
                                    color: Theme.text
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: win.authReady
                                        ? "Cached Spotify credentials present on device"
                                        : "Sign in once via Terminal UI to browse Playlists, Liked Songs & Search"
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
                            height: 76
                            radius: Theme.radiusM
                            color: win.spotifyGreen

                            RowLayout {
                                anchors.centerIn: parent
                                spacing: 12
                                Components.MaterialIcon {
                                    name: "terminal"
                                    size: 32
                                    color: "#08120B"
                                }
                                Text {
                                    text: "Launch Terminal Spotify (Login / Browse)"
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontCaption
                                    font.bold: true
                                    color: "#08120B"
                                }
                            }

                            MouseArea {
                                anchors.fill: parent
                                onClicked: win.runAction(["tui"], "Launching spotify_player in Terminal...")
                            }
                        }
                    }
                }
            }
        }
    }

    // Floating feedback toast
    Rectangle {
        visible: win.toastMessage.length > 0
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: Theme.navZoneHeight + Theme.spaceM
        }
        width: Math.min(parent.width - 48, toastText.implicitWidth + 40)
        height: 58
        radius: Theme.radiusPill
        color: "#1F2923"
        border.width: 1
        border.color: win.spotifyGreen
        z: 50

        Text {
            id: toastText
            anchors.centerIn: parent
            text: win.toastMessage
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontCaption
            color: Theme.text
        }
    }

    Components.AppGestureLayer {
        anchors.fill: parent
        z: 100
        onBackTriggered: Qt.quit()
    }
}
