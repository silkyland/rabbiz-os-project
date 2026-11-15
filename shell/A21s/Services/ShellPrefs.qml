pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Persistent launcher & theme preferences shared between the shell and
// Settings.qml via ~/.config/a21s-shell/prefs.json.
QtObject {
    id: prefs

    property string wallpaperId: "obsidian"   // obsidian | graphite | slate | oled | warm
    property string accentId: "blue"          // mono | blue | emerald | amber | crimson
    property string clockStyle: "editorial"   // editorial | classic | compact
    property bool use24h: true
    property bool showSteamCard: true
    property string iconStyle: "squircle"     // squircle | circle | glass

    readonly property var wallpapers: [
        {
            id: "obsidian",
            name: "Obsidian",
            top: "#141417",
            mid: "#0E0E10",
            bottom: "#08080A",
            orb1: "#FFFFFF",
            orb2: "#FFFFFF",
            orb3: "#FFFFFF"
        },
        {
            id: "graphite",
            name: "Graphite",
            top: "#1A1A1E",
            mid: "#131316",
            bottom: "#0B0B0D",
            orb1: "#FFFFFF",
            orb2: "#FFFFFF",
            orb3: "#FFFFFF"
        },
        {
            id: "slate",
            name: "Slate",
            top: "#14181F",
            mid: "#0E1116",
            bottom: "#090B0E",
            orb1: "#94A3B8",
            orb2: "#94A3B8",
            orb3: "#94A3B8"
        },
        {
            id: "oled",
            name: "Pure Black",
            top: "#000000",
            mid: "#000000",
            bottom: "#000000",
            orb1: "#27272A",
            orb2: "#27272A",
            orb3: "#27272A"
        },
        {
            id: "warm",
            name: "Espresso",
            top: "#171412",
            mid: "#110F0D",
            bottom: "#0A0908",
            orb1: "#D6D3D1",
            orb2: "#D6D3D1",
            orb3: "#D6D3D1"
        }
    ]

    readonly property var accents: [
        { id: "mono",    name: "Monochrome", color: "#E4E4E7" },
        { id: "blue",    name: "Blue",       color: "#3B82F6" },
        { id: "emerald", name: "Green",      color: "#22C55E" },
        { id: "amber",   name: "Amber",      color: "#F59E0B" },
        { id: "crimson", name: "Red",        color: "#EF4444" }
    ]

    readonly property var activeWallpaper: {
        for (var i = 0; i < wallpapers.length; ++i) {
            if (wallpapers[i].id === wallpaperId)
                return wallpapers[i]
        }
        return wallpapers[0]
    }

    readonly property color accentColor: {
        for (var i = 0; i < accents.length; ++i) {
            if (accents[i].id === accentId)
                return accents[i].color
        }
        return accents[0].color
    }

    readonly property color onAccentColor: accentId === "mono" ? "#09090B" : "#FFFFFF"

    readonly property string prefsPath: {
        var home = Quickshell.env("HOME") || "/home/gamer"
        return home + "/.config/a21s-shell/prefs.json"
    }

    function applyJson(raw) {
        if (!raw || raw.trim().length === 0)
            return
        try {
            var d = JSON.parse(raw)
            if (d.wallpaperId !== undefined) wallpaperId = String(d.wallpaperId)
            if (d.accentId !== undefined) accentId = String(d.accentId)
            if (d.clockStyle !== undefined) clockStyle = String(d.clockStyle)
            if (d.use24h !== undefined) use24h = Boolean(d.use24h)
            if (d.showSteamCard !== undefined) showSteamCard = Boolean(d.showSteamCard)
            if (d.iconStyle !== undefined) iconStyle = String(d.iconStyle)
        } catch (e) {}
    }

    function save() {
        var payload = JSON.stringify({
            wallpaperId: wallpaperId,
            accentId: accentId,
            clockStyle: clockStyle,
            use24h: use24h,
            showSteamCard: showSteamCard,
            iconStyle: iconStyle
        }, null, 2)
        saveProc.command = [
            "sh", "-c",
            "mkdir -p \"$(dirname \"$1\")\" && printf '%s\n' \"$2\" > \"$1.tmp\" && mv \"$1.tmp\" \"$1\"",
            "_", prefs.prefsPath, payload
        ]
        saveProc.running = true
    }

    property var saveProc: Process {}

    property var fileWatcher: FileView {
        path: prefs.prefsPath
        watchChanges: true
        onLoaded: prefs.applyJson(text())
        onFileChanged: reload()
    }
}
