import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets
import RabbizKit
import "MusicLogic.js" as Music

// Music — in-house replacement for elisa (docs/BASE-APPS.md).
//
// Library: one `find` over ~/Music and ~/Downloads (mp3/flac/ogg/m4a/wav/opus),
// newest first. Playback: one `mpv --no-video` process per track; next/previous play
// another track and pause/stop terminate mpv. Resume restarts mpv at the offset the
// QML timer reached (`--start=`), because Quickshell 0.3 only exposes a Unix socket
// *server* (Quickshell.Io.Socket), so nothing here can talk to mpv's --input-ipc-server.
// Timing honesty: the length comes from ffprobe, the elapsed clock is this app's own
// timer; mpv's playback position is never queried or guessed. No shader effects.
ApplicationWindow {
    id: win
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    title: "Music"
    color: Theme.bg
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontBody

    Binding { target: Theme; property: "light"; value: ShellPrefs.lightTheme }

    readonly property string home: Quickshell.env("HOME") || "/home/rabbiz"

    // ---- library ----------------------------------------------------------
    property var tracks: []
    property string scope: "all"          // all | music | downloads
    property bool scanning: false
    property string scanError: ""
    readonly property var visibleTracks: Music.filterTracks(win.tracks, win.scope)

    // ---- now playing ------------------------------------------------------
    property string currentPath: ""
    property string currentTitle: ""
    property string currentCaption: ""
    property string currentFormat: ""
    property real baseSec: 0              // offset the running mpv was started at
    property real elapsedSec: 0
    property real durationSec: 0          // 0 = unknown (ffprobe missing or failed)
    property bool playing: false
    property bool paused: false
    property real segmentStartMs: 0
    property int expectedExits: 0         // deliberate SIGTERMs still awaiting onExited
    readonly property bool hasTrack: win.currentPath.length > 0
    readonly property real progress: Music.progressFraction(win.elapsedSec, win.durationSec)

    // ---- backend availability / errors ------------------------------------
    property bool hasMpv: true
    property bool hasFfprobe: true
    property string playerError: ""

    readonly property string subtitle: {
        if (win.scanning)
            return "Scanning ~/Music and ~/Downloads…"
        if (win.tracks.length === 0)
            return win.scanError.length > 0 ? "Could not read your music folders"
                                            : "No audio found in ~/Music or ~/Downloads"
        var n = win.tracks.length
        return n + (n === 1 ? " track" : " tracks") + " · tap one to play"
    }

    readonly property string emptyIcon: win.scanning ? "search"
        : (win.scanError.length > 0 && win.tracks.length === 0) ? "error_outline"
        : "music_off"

    readonly property string emptyTitle: {
        if (win.scanning) return "Looking for audio…"
        if (win.tracks.length === 0)
            return win.scanError.length > 0 ? "Could not read your music folders" : "No audio files yet"
        return win.scope === "downloads" ? "Nothing in Downloads" : "Nothing in Music"
    }

    readonly property string emptyHint: {
        if (win.scanning)
            return "Checking ~/Music and ~/Downloads for mp3, flac, ogg, m4a, wav and opus files."
        if (win.tracks.length === 0) {
            if (win.scanError.length > 0)
                return "Check that both folders exist and are readable, then tap Scan again."
            return "Put audio in ~/Music or ~/Downloads, then tap Scan again."
        }
        return win.scope === "downloads"
            ? "Audio you download lands here. Tap All to see every track."
            : "Tap All to see every track, or add audio to ~/Music."
    }

    readonly property string scanScript:
        'for d in "$1" "$2"; do ' +
        '[ -d "$d" ] && find "$d" -maxdepth 4 -type f ' +
        '\\( -iname "*.mp3" -o -iname "*.flac" -o -iname "*.ogg" ' +
        '-o -iname "*.m4a" -o -iname "*.wav" -o -iname "*.opus" \\) ' +
        '-printf "%T@\\t%p\\n" 2>/dev/null; ' +
        'done | sort -rn'

    readonly property string depsScript:
        "command -v mpv >/dev/null 2>&1 && echo mpv=yes || echo mpv=no; " +
        "command -v ffprobe >/dev/null 2>&1 && echo ffprobe=yes || echo ffprobe=no"

    // Honesty about the clock: mpv's position is never read, so say where it comes from.
    readonly property string timingNote: win.durationSec > 0
        ? "Length from ffprobe · elapsed counted by this app, not read from mpv"
        : "Length unknown · elapsed counted by this app, not read from mpv"

    // ---- library ----------------------------------------------------------
    function scan() {
        if (scanProc.running) return
        win.scanning = true
        win.scanError = ""
        scanProc.command = ["sh", "-c", win.scanScript, "_",
                            win.home + "/Music", win.home + "/Downloads"]
        scanProc.running = true
    }

    function applyScan(raw) {
        win.scanning = false
        win.tracks = Music.parseFind(raw, win.home)
    }

    function applyDeps(raw) {
        var text = String(raw || "")
        win.hasMpv = text.indexOf("mpv=yes") >= 0
        win.hasFfprobe = text.indexOf("ffprobe=yes") >= 0
    }

    // ---- transport --------------------------------------------------------
    function playTrack(track, startSec) {
        if (!track || !track.path) return
        win.currentPath = track.path
        win.currentTitle = track.title
        win.currentCaption = track.caption
        win.currentFormat = String(track.ext || "").toUpperCase()
        win.durationSec = 0
        win.probeDuration(track.path)
        win.startPlayer(track.path, Math.max(0, startSec || 0))
    }

    function onTrackClicked(track) {
        if (!track) return
        if (track.path === win.currentPath) win.togglePlay()
        else win.playTrack(track, 0)
    }

    function startPlayer(path, startSec) {
        if (path.length === 0) return
        if (player.running) win.expectedExits += 1
        win.baseSec = Math.max(0, startSec || 0)
        win.segmentStartMs = Date.now()
        win.elapsedSec = win.baseSec
        win.playing = true
        win.paused = false
        win.playerError = ""
        tickTimer.running = true
        stallTimer.restart()
        player.exec(["mpv", "--no-video", "--audio-display=no",
                     "--no-terminal", "--force-window=no",
                     "--audio-client-name=rabbiz-music",
                     "--start=" + win.baseSec.toFixed(2), path])
    }

    function killPlayer() {
        if (player.running) {
            win.expectedExits += 1
            player.running = false
        }
    }

    function pause() {
        if (!win.playing) return
        win.baseSec = win.elapsedSec
        win.playing = false
        win.paused = true
        tickTimer.running = false
        stallTimer.stop()
        win.killPlayer()
    }

    function resume() {
        if (win.playing || !win.hasTrack) return
        var start = win.baseSec
        if (win.durationSec > 0 && start >= win.durationSec - 0.5) start = 0
        win.startPlayer(win.currentPath, start)
    }

    function togglePlay() {
        if (win.playing) {
            win.pause()
        } else if (win.hasTrack) {
            win.resume()
        } else {
            let first = win.visibleTracks.length > 0 ? win.visibleTracks[0]
                      : win.tracks.length > 0 ? win.tracks[0] : null
            if (first) win.playTrack(first, 0)
        }
    }

    function stop() {
        win.playing = false
        win.paused = false
        tickTimer.running = false
        stallTimer.stop()
        win.killPlayer()
        win.currentPath = ""
        win.currentTitle = ""
        win.currentCaption = ""
        win.currentFormat = ""
        win.elapsedSec = 0
        win.baseSec = 0
        win.durationSec = 0
        win.playerError = ""
    }

    function next() {
        var index = Music.indexOfPath(win.tracks, win.currentPath)
        var target = Music.nextIndex(index, win.tracks.length)
        if (target >= 0) win.playTrack(win.tracks[target], 0)
    }

    function previous() {
        var index = Music.indexOfPath(win.tracks, win.currentPath)
        var target = Music.prevIndex(index, win.tracks.length)
        if (target >= 0) win.playTrack(win.tracks[target], 0)
    }

    function transport(action) {
        if (action === "next") win.next()
        else if (action === "prev") win.previous()
        else win.togglePlay()
    }

    function probeDuration(path) {
        if (!win.hasFfprobe || path.length === 0) return
        durationProc.exec(["ffprobe", "-v", "error",
                           "-show_entries", "format=filename,duration",
                           "-of", "default=nw=1", path])
    }

    function applyProbe(raw) {
        var info = Music.parseProbe(raw)
        if (info.path.length > 0 && info.path !== win.currentPath) return
        if (info.duration > 0) win.durationSec = info.duration
    }

    // Tapping the progress bar restarts mpv at the tapped offset (no IPC needed).
    function seekTo(fraction) {
        if (!win.hasTrack || win.durationSec <= 0) return
        win.startPlayer(win.currentPath, Music.seekSeconds(fraction, win.durationSec))
    }

    // ---- elapsed clock / end of track -------------------------------------
    function onTick() {
        if (!win.playing) return
        win.elapsedSec = win.baseSec + (Date.now() - win.segmentStartMs) / 1000
        if (win.durationSec > 0 && win.elapsedSec >= win.durationSec) {
            win.elapsedSec = win.durationSec
            win.finishTrack()
        }
    }

    function finishTrack() {
        if (!win.playing) return
        win.playing = false
        win.paused = true
        tickTimer.running = false
        var index = Music.indexOfPath(win.tracks, win.currentPath)
        var target = Music.autoNextIndex(index, win.tracks.length)
        if (target >= 0) {
            win.playTrack(win.tracks[target], 0)
        } else {
            win.baseSec = win.elapsedSec
            win.killPlayer()
        }
    }

    function handlePlayerExit(code, status) {
        if (win.expectedExits > 0) {
            win.expectedExits -= 1          // we asked for this exit (pause/stop/switch)
            return
        }
        if (!win.playing) return
        // Refresh the clock at the moment of exit: the 500 ms tick may be a bit behind,
        // and mpv exits on its own when the file ends.
        win.elapsedSec = win.baseSec + (Date.now() - win.segmentStartMs) / 1000
        // mpv logs are silenced with --no-terminal, so the exit itself is the signal:
        // reaching the known end (or a clean exit after real playback) advances the list.
        var reachedEnd = win.durationSec > 0 && win.elapsedSec >= win.durationSec - 1.5
        var playedEnough = win.elapsedSec >= 1.0
        if (reachedEnd || (code === 0 && playedEnough)) {
            win.finishTrack()
            return
        }
        win.playing = false
        win.paused = true
        tickTimer.running = false
        win.playerError = "Playback stopped before the track ended (mpv exit code " + code
            + "). The file may be unsupported, or the audio output is busy."
    }

    function checkStall() {
        if (!win.playing || player.running || win.expectedExits > 0) return
        win.playing = false
        win.paused = true
        tickTimer.running = false
        win.playerError = win.hasMpv
            ? "Playback did not start. The file may be unsupported, or the audio output is busy."
            : "mpv is not installed, so audio cannot play."
    }

    // ---- processes --------------------------------------------------------
    Process {
        id: depsProc
        command: ["sh", "-c", win.depsScript]
        stdout: StdioCollector { onStreamFinished: win.applyDeps(this.text) }
        Component.onCompleted: running = true
    }

    Process {
        id: scanProc
        command: ["sh", "-c", win.scanScript, "_",
                  win.home + "/Music", win.home + "/Downloads"]
        stdout: StdioCollector { onStreamFinished: win.applyScan(this.text) }
        stderr: StdioCollector {
            onStreamFinished: {
                var detail = String(this.text || "").trim()
                if (detail.length > 0) win.scanError = detail.split("\n")[0]
            }
        }
        onExited: win.scanning = false
    }

    Process {
        id: durationProc
        stdout: StdioCollector { onStreamFinished: win.applyProbe(this.text) }
    }

    Process {
        id: player
        onStarted: {
            win.expectedExits = 0
            win.segmentStartMs = Date.now()
            win.elapsedSec = win.baseSec
            stallTimer.stop()
        }
        onExited: function (code, status) { win.handlePlayerExit(code, status) }
    }

    Timer {
        id: tickTimer
        interval: 500
        repeat: true
        running: false
        onTriggered: win.onTick()
    }

    Timer {
        id: stallTimer
        interval: 1500
        repeat: false
        onTriggered: win.checkStall()
    }

    Component.onCompleted: win.scan()

    // ---- UI ---------------------------------------------------------------
    ColumnLayout {
        anchors {
            fill: parent
            leftMargin: Theme.spaceM
            rightMargin: Theme.spaceM
            topMargin: Theme.spaceM
            bottomMargin: Theme.spaceL
        }
        spacing: Theme.spaceM

        AppHeader {
            Layout.fillWidth: true
            title: "Music"
            subtitle: win.subtitle
            IconButton {
                icon: "refresh"
                filled: win.scanning
                // The kit button is 44 px; the base-app rule is a 48 px touch target.
                width: 48
                height: 48
                radius: 24
                onClicked: win.scan()
            }
        }

        // Missing backends: say exactly what is broken instead of doing nothing.
        Card {
            Layout.fillWidth: true
            visible: !win.hasMpv || !win.hasFfprobe
            padding: Theme.spaceM

            Text {
                width: parent.width
                wrapMode: Text.Wrap
                color: !win.hasMpv ? Theme.danger : Theme.textDim
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontCaption - 4
                text: !win.hasMpv
                    ? "mpv is not installed, so nothing can play. Install it with pacman -S mpv, then tap Refresh."
                    : "ffprobe is missing (package ffmpeg), so track length and seeking are unavailable. Playback still works."
            }
        }

        Card {
            Layout.fillWidth: true
            visible: win.playerError.length > 0
            padding: Theme.spaceM

            Text {
                width: parent.width
                wrapMode: Text.Wrap
                color: Theme.danger
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontCaption - 4
                text: win.playerError
            }
        }

        // Scope chips: All / Music / Downloads, all at least 48 px tall.
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceS

            Repeater {
                model: [
                    { scope: "all", label: "All" },
                    { scope: "music", label: "Music" },
                    { scope: "downloads", label: "Downloads" }
                ]

                delegate: Rectangle {
                    id: chip
                    required property var modelData
                    readonly property int scopeCount: modelData.scope === "all"
                        ? win.tracks.length
                        : Music.countFolder(win.tracks, modelData.scope === "music" ? "Music" : "Downloads")
                    readonly property bool active: win.scope === modelData.scope

                    Layout.fillWidth: true
                    Layout.preferredHeight: 48
                    radius: Theme.radiusPill
                    color: chip.active ? Qt.alpha(ShellPrefs.accentColor, 0.20) : Theme.surfaceHigh
                    border.width: 1
                    border.color: chip.active ? ShellPrefs.accentColor : Qt.alpha(Theme.text, 0.08)

                    Row {
                        anchors.centerIn: parent
                        spacing: Theme.spaceXS

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: chip.modelData.label
                            color: chip.active ? ShellPrefs.accentColor : Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontCaption - 4
                            font.weight: Font.Medium
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: chip.scopeCount
                            color: chip.active ? ShellPrefs.accentColor : Theme.textDim
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontCaption - 4
                        }
                    }

                    TapHandler { onTapped: win.scope = chip.modelData.scope }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true

            SectionLabel {
                Layout.fillWidth: true
                text: "LIBRARY"
            }
            Text {
                text: "Newest first"
                color: Theme.textFaint
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontCaption - 6
            }
        }

        // List surface. A plain Rectangle (not Card) because it must fill the page.
        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            radius: Theme.radiusL
            color: Theme.card
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.08)

            ListView {
                id: list
                anchors.fill: parent
                anchors.margins: Theme.spaceXS
                clip: true
                spacing: 2
                cacheBuffer: 1200
                boundsBehavior: Flickable.StopAtBounds
                model: win.visibleTracks

                delegate: Item {
                    id: row
                    required property var modelData
                    required property int index

                    width: ListView.view.width
                    height: 76

                    Rectangle {
                        anchors.fill: parent
                        radius: Theme.radiusM
                        color: row.modelData.path === win.currentPath
                            ? Qt.alpha(ShellPrefs.accentColor, 0.14) : "transparent"
                    }

                    RowItem {
                        anchors.fill: parent
                        icon: row.modelData.path === win.currentPath
                            ? (win.playing ? "graphic_eq" : "pause_circle") : "music_note"
                        title: row.modelData.title
                        caption: row.modelData.caption
                        value: String(row.modelData.ext || "").toUpperCase()
                        showChevron: false
                        onClicked: win.onTrackClicked(row.modelData)
                    }
                }
            }

            Column {
                visible: list.count === 0
                anchors.centerIn: parent
                width: parent.width - Theme.spaceXL
                spacing: Theme.spaceL

                EmptyState {
                    width: parent.width
                    icon: win.emptyIcon
                    title: win.emptyTitle
                    hint: win.emptyHint
                }

                // Accent pill. Same size/shape as RabbizKit.PrimaryButton, inlined because
                // PrimaryButton.qml currently references MaterialIcon without importing
                // Rabbiz.Widgets, so it fails to load (kit is lead-owned — reported, not edited).
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: !win.scanning && (win.tracks.length === 0 || win.scope !== "all")
                    width: emptyActionRow.implicitWidth + Theme.spaceXL
                    height: 48
                    radius: height / 2
                    color: ShellPrefs.accentColor
                    scale: emptyActionTap.pressed ? 0.96 : 1.0
                    Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                    RowLayout {
                        id: emptyActionRow
                        anchors.centerIn: parent
                        spacing: Theme.spaceXS

                        MaterialIcon {
                            Layout.alignment: Qt.AlignVCenter
                            name: win.tracks.length === 0 ? "refresh" : "library_music"
                            size: 20
                            color: ShellPrefs.onAccentColor
                        }
                        Text {
                            Layout.alignment: Qt.AlignVCenter
                            text: win.tracks.length === 0 ? "Scan again" : "Show all tracks"
                            color: ShellPrefs.onAccentColor
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontBody - 4
                            font.weight: Font.Medium
                        }
                    }

                    TapHandler {
                        id: emptyActionTap
                        onTapped: {
                            if (win.tracks.length === 0) win.scan()
                            else win.scope = "all"
                        }
                    }
                }
            }
        }

        // ---- now-playing bar ----------------------------------------------
        Card {
            Layout.fillWidth: true
            visible: win.hasTrack
            padding: Theme.spaceM

            Column {
                width: parent.width
                spacing: Theme.spaceS

                Row {
                    width: parent.width
                    spacing: Theme.spaceM

                    Rectangle {
                        width: 64
                        height: 64
                        radius: Theme.radiusM
                        color: Qt.alpha(ShellPrefs.accentColor, 0.16)

                        MaterialIcon {
                            anchors.centerIn: parent
                            name: win.playing ? "graphic_eq" : "music_note"
                            size: 32
                            color: ShellPrefs.accentColor
                        }
                    }

                    Item {
                        width: parent.width - 64 - 48 - Theme.spaceM * 2
                        height: 64

                        Column {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width
                            spacing: 2

                            Text {
                                width: parent.width
                                text: win.currentTitle
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody - 2
                                font.weight: Font.DemiBold
                                elide: Text.ElideRight
                            }
                            Text {
                                width: parent.width
                                text: win.currentCaption
                                    + (win.currentFormat.length > 0 ? " · " + win.currentFormat : "")
                                color: Theme.textDim
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontCaption - 6
                                elide: Text.ElideRight
                            }
                        }
                    }

                    Item {
                        width: 48
                        height: 64

                        Rectangle {
                            anchors.centerIn: parent
                            width: 48
                            height: 48
                            radius: 24
                            color: Theme.surfaceHigh
                            border.width: 1
                            border.color: Qt.alpha(Theme.text, 0.10)

                            MaterialIcon {
                                anchors.centerIn: parent
                                name: "stop"
                                size: 22
                                color: Theme.text
                            }
                            TapHandler { onTapped: win.stop() }
                        }
                    }
                }

                // Seek bar: 48 px tall touch area, 8 px track.
                Item {
                    id: seekArea
                    width: parent.width
                    height: 48

                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width
                        height: 8
                        radius: 4
                        color: Theme.surfaceHigh

                        Rectangle {
                            width: parent.width * win.progress
                            height: parent.height
                            radius: parent.radius
                            color: ShellPrefs.accentColor
                        }
                    }

                    TapHandler {
                        onTapped: function (point) {
                            if (win.durationSec > 0)
                                win.seekTo(point.position.x / seekArea.width)
                        }
                    }
                }

                Item {
                    width: parent.width
                    height: 20

                    Text {
                        anchors.left: parent.left
                        text: Music.formatTime(win.elapsedSec)
                        color: Theme.textDim
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontCaption - 6
                    }
                    Text {
                        anchors.right: parent.right
                        text: win.durationSec > 0 ? Music.formatTime(win.durationSec) : "--:--"
                        color: Theme.textDim
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontCaption - 6
                    }
                }

                Item {
                    width: parent.width
                    height: 84

                    Row {
                        anchors.centerIn: parent
                        spacing: Theme.spaceXL

                        Repeater {
                            model: [
                                { act: "prev", icon: "skip_previous", size: 52, primary: false },
                                { act: "toggle", icon: "play_arrow", size: 72, primary: true },
                                { act: "next", icon: "skip_next", size: 52, primary: false }
                            ]

                            delegate: Rectangle {
                                id: transportButton
                                required property var modelData
                                readonly property bool primary: modelData.primary
                                readonly property string glyph: modelData.act === "toggle"
                                    ? (win.playing ? "pause" : "play_arrow") : modelData.icon

                                width: modelData.size
                                height: modelData.size
                                radius: width / 2
                                color: transportButton.primary ? ShellPrefs.accentColor : Theme.surfaceHigh
                                border.width: transportButton.primary ? 0 : 1
                                border.color: Qt.alpha(Theme.text, 0.10)
                                scale: transportTap.pressed ? 0.94 : 1.0
                                Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                                MaterialIcon {
                                    anchors.centerIn: parent
                                    name: transportButton.glyph
                                    size: transportButton.primary ? 38 : 26
                                    color: transportButton.primary ? ShellPrefs.onAccentColor : Theme.text
                                }
                                TapHandler {
                                    id: transportTap
                                    onTapped: win.transport(transportButton.modelData.act)
                                }
                            }
                        }
                    }
                }

                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    text: "Volume follows the system speaker — use the side keys."
                    color: Theme.textDim
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontCaption - 6
                }

                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    text: win.timingNote
                    color: Theme.textFaint
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontCaption - 6
                }
            }
        }
    }
}
