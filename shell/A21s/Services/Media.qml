pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Mpris

// Media playback controller backed by Quickshell.Services.Mpris (e.g. Elisa,
// Chromium/Spotify/Steam), with an interactive placeholder deck when no
// MPRIS player is active on the session bus.
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

    readonly property bool hasRealPlayer: activePlayer !== null

    // Interactive placeholder state when no MPRIS player is active
    property bool demoPlaying: false
    property real demoProgress: 0.34
    property int demoTrackIndex: 0

    readonly property var _demoTracks: [
        { title: "Midnight Aurora",  artist: "Synthwave · Ready" },
        { title: "Exynos Pulse",     artist: "Lo-Fi Beats · Ready" },
        { title: "Nordic Horizon",   artist: "Ambient · Ready" }
    ]

    readonly property bool isPlaying: hasRealPlayer ? activePlayer.isPlaying : demoPlaying
    readonly property string title: hasRealPlayer
        ? (activePlayer.trackTitle || activePlayer.identity || "Now Playing")
        : _demoTracks[demoTrackIndex].title
    readonly property string artist: hasRealPlayer
        ? (activePlayer.trackArtist || activePlayer.identity || "Media Player")
        : _demoTracks[demoTrackIndex].artist
    readonly property string artUrl: hasRealPlayer && activePlayer.trackArtUrl
        ? String(activePlayer.trackArtUrl)
        : ""
    readonly property real progress: {
        if (hasRealPlayer && activePlayer.length > 0)
            return Math.max(0, Math.min(1, activePlayer.position / activePlayer.length))
        return demoProgress
    }

    function togglePlaying() {
        if (hasRealPlayer && activePlayer.canTogglePlaying) {
            activePlayer.togglePlaying()
        } else if (hasRealPlayer && (activePlayer.canPlay || activePlayer.canPause)) {
            if (activePlayer.isPlaying)
                activePlayer.pause()
            else
                activePlayer.play()
        } else {
            demoPlaying = !demoPlaying
        }
    }

    function next() {
        if (hasRealPlayer && activePlayer.canGoNext) {
            activePlayer.next()
        } else {
            demoTrackIndex = (demoTrackIndex + 1) % _demoTracks.length
            demoProgress = 0.08
        }
    }

    function previous() {
        if (hasRealPlayer && activePlayer.canGoPrevious) {
            activePlayer.previous()
        } else {
            demoTrackIndex = (demoTrackIndex - 1 + _demoTracks.length) % _demoTracks.length
            demoProgress = 0.08
        }
    }

    function openMusicApp() {
        var entry = DesktopEntries.byId("org.kde.elisa") || DesktopEntries.byId("elisa")
        if (entry && typeof entry.execute === "function")
            entry.execute()
        else if (!elisaProc.running)
            elisaProc.running = true
    }

    property var elisaProc: Process {
        command: ["sh", "-c",
            "QT_QPA_PLATFORM=wayland QT_QUICK_BACKEND=software QT_QUICK_CONTROLS_MOBILE=1 elisa >/dev/null 2>&1 &"]
    }

    property var _demoTimer: Timer {
        interval: 250
        running: !root.hasRealPlayer && root.demoPlaying
        repeat: true
        onTriggered: {
            root.demoProgress += 0.008
            if (root.demoProgress >= 1.0)
                root.next()
        }
    }
}
