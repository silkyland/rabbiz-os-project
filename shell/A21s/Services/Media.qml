pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Mpris

// Media playback controller combining Quickshell.Services.Mpris (e.g. Elisa,
// Angelfish/Chromium, Spotify Connect) with a21s-spotify-ctl (mpv + librespot).
QtObject {
    id: root

    readonly property var playersList: Mpris.players ? Mpris.players.values : []

    readonly property var activePlayer: {
        var list = root.playersList || []
        for (var i = 0; i < list.length; ++i) {
            if (list[i] && list[i].isPlaying)
                return list[i]
        }
        return list.length > 0 ? list[0] : null
    }

    readonly property string _eventFilePath: {
        var rt = Quickshell.env("XDG_RUNTIME_DIR") || "/run/user/1001"
        return rt + "/a21s-spotify/event.env"
    }

    property string ctlStatus: "Idle"
    property string ctlTitle: ""
    property string ctlArtist: ""
    property string ctlAlbum: ""
    property string ctlArtUrl: ""
    property string ctlSource: ""
    property real ctlPosSec: 0
    property real ctlDurSec: 0

    readonly property bool ctlHasTrack: ctlTitle.length > 0
        && (ctlStatus === "Playing" || ctlStatus === "Paused")
    readonly property bool mprisIsPlaying: activePlayer !== null && activePlayer.isPlaying
    readonly property bool ctlIsPlaying: ctlStatus === "Playing"
    readonly property bool useMpris: activePlayer !== null
        && (mprisIsPlaying || (!ctlIsPlaying && !ctlHasTrack))

    readonly property bool hasRealPlayer: activePlayer !== null || ctlHasTrack

    readonly property bool isPlaying: useMpris
        ? activePlayer.isPlaying
        : ctlIsPlaying

    readonly property string title: {
        if (useMpris)
            return activePlayer.trackTitle || activePlayer.identity || "Now Playing"
        if (ctlTitle.length > 0)
            return ctlTitle
        return "Galaxy A21s Music"
    }

    readonly property string artist: {
        if (useMpris)
            return activePlayer.trackArtist || activePlayer.identity || "Media Player"
        if (ctlArtist.length > 0)
            return ctlSource.length > 0 ? (ctlArtist + " · " + ctlSource) : ctlArtist
        return "Tap Play for audio or open Spotify"
    }

    readonly property string artUrl: {
        if (useMpris && activePlayer.trackArtUrl)
            return String(activePlayer.trackArtUrl)
        return ctlArtUrl
    }

    readonly property real progress: {
        if (useMpris && activePlayer.length > 0)
            return Math.max(0, Math.min(1, activePlayer.position / activePlayer.length))
        if (ctlDurSec > 0)
            return Math.max(0, Math.min(1, ctlPosSec / ctlDurSec))
        return ctlIsPlaying ? 0.35 : 0.0
    }

    function _parseEventEnv(txt) {
        if (!txt)
            return
        var lines = String(txt).split("\n")
        var status = "Idle"
        var title = ""
        var artist = ""
        var album = ""
        var art = ""
        var source = ""
        for (var i = 0; i < lines.length; ++i) {
            var line = lines[i].trim()
            if (!line)
                continue
            var eq = line.indexOf("=")
            if (eq <= 0)
                continue
            var k = line.slice(0, eq)
            var v = line.slice(eq + 1)
            if (k === "EVT_STATUS") status = v
            else if (k === "EVT_TITLE") title = v
            else if (k === "EVT_ARTIST") artist = v
            else if (k === "EVT_ALBUM") album = v
            else if (k === "EVT_ART") art = v
            else if (k === "EVT_SOURCE") source = v
        }
        root.ctlStatus = status || "Idle"
        root.ctlTitle = title
        root.ctlArtist = artist
        root.ctlAlbum = album
        root.ctlArtUrl = art
        root.ctlSource = source
    }

    function _parseCtlStatus(txt) {
        if (!txt)
            return
        var lines = String(txt).split("\n")
        for (var i = 0; i < lines.length; ++i) {
            var line = lines[i].trim()
            if (!line)
                continue
            var eq = line.indexOf("=")
            if (eq <= 0)
                continue
            var k = line.slice(0, eq)
            var v = line.slice(eq + 1)
            if (k === "PLAYER_STATUS") root.ctlStatus = v || "Idle"
            else if (k === "TITLE") root.ctlTitle = v
            else if (k === "ARTIST") root.ctlArtist = v
            else if (k === "ALBUM") root.ctlAlbum = v
            else if (k === "ART_URL") root.ctlArtUrl = v
            else if (k === "PLAYER_SOURCE") root.ctlSource = v
            else if (k === "POS_SEC") root.ctlPosSec = parseFloat(v) || 0
            else if (k === "DUR_SEC") root.ctlDurSec = parseFloat(v) || 0
        }
    }

    function _runCtl(args) {
        if (!ctlActionProc.running) {
            ctlActionProc.command = ["a21s-spotify-ctl"].concat(args)
            ctlActionProc.running = true
        }
    }

    function togglePlaying() {
        if (useMpris && activePlayer.canTogglePlaying) {
            activePlayer.togglePlaying()
        } else if (useMpris && (activePlayer.canPlay || activePlayer.canPause)) {
            if (activePlayer.isPlaying)
                activePlayer.pause()
            else
                activePlayer.play()
        } else {
            _runCtl(["play-pause"])
        }
    }

    function next() {
        if (useMpris && activePlayer.canGoNext) {
            activePlayer.next()
        } else {
            _runCtl(["next"])
        }
    }

    function previous() {
        if (useMpris && activePlayer.canGoPrevious) {
            activePlayer.previous()
        } else {
            _runCtl(["prev"])
        }
    }

    function openMusicApp() {
        var entry = DesktopEntries.byId("a21s-spotify")
            || DesktopEntries.byId("org.kde.elisa")
            || DesktopEntries.byId("elisa")
        if (entry && typeof entry.execute === "function")
            entry.execute()
        else if (!spotifyProc.running)
            spotifyProc.running = true
    }

    Component.onCompleted: {
        eventInitProc.running = true
        ctlStatusProc.running = true
    }

    property var eventInitProc: Process {
        command: ["sh", "-c",
            "F=\"$1\"; mkdir -p \"$(dirname \"$F\")\" 2>/dev/null || true; [ -f \"$F\" ] || : > \"$F\"; echo ok",
            "_", root._eventFilePath]
        stdout: StdioCollector {
            onStreamFinished: eventWatcher.reload()
        }
    }

    property var eventWatcher: FileView {
        path: root._eventFilePath
        watchChanges: true
        onLoaded: root._parseEventEnv(text())
        onFileChanged: reload()
    }

    property var ctlActionProc: Process {
        stdout: StdioCollector {
            onStreamFinished: {
                eventWatcher.reload()
                if (!ctlStatusProc.running)
                    ctlStatusProc.running = true
            }
        }
    }

    property var ctlStatusProc: Process {
        command: ["a21s-spotify-ctl", "status"]
        stdout: StdioCollector {
            onStreamFinished: root._parseCtlStatus(text)
        }
    }

    property var _pollTimer: Timer {
        interval: 2500
        running: root.ctlStatus === "Playing"
        repeat: true
        onTriggered: {
            if (!ctlStatusProc.running)
                ctlStatusProc.running = true
        }
    }

    property var spotifyProc: Process {
        command: ["sh", "-c",
            "QT_QPA_PLATFORM=wayland QT_QUICK_BACKEND=software QT_QUICK_CONTROLS_MOBILE=1 a21s-spotify >/dev/null 2>&1 &"]
    }

    property var elisaProc: Process {
        command: ["sh", "-c",
            "QT_QPA_PLATFORM=wayland QT_QUICK_BACKEND=software QT_QUICK_CONTROLS_MOBILE=1 elisa >/dev/null 2>&1 &"]
    }
}
