pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import Rabbiz.Theme

// Persistent launcher & theme preferences shared between the shell and
// Settings.qml via ~/.config/rabbiz-shell/prefs.json.
QtObject {
    id: prefs

    property bool loaded: false
    property int brightnessLevel: -1          // -1 = migrate current backlight on first load

    property string wallpaperId: "aurora"     // aurora | nebula | sunset | emerald | oled
    property string accentId: "blue"          // mono | blue | emerald | amber | crimson
    property bool lightTheme: false           // false = dark (default)
    property string clockStyle: "editorial"   // editorial | classic | compact
    property bool use24h: true
    property bool dndEnabled: false
    property string iconStyle: "squircle"     // squircle | circle | glass
    property int sleepTimeoutSec: 120         // 30 | 60 | 120 | 300 | 0 (Never)
    property bool fingerprintEnabled: true
    property bool fingerprintWakeUnlock: true
    property int enrolledFingerCount: 1
    property bool fingerprintHaptic: true
    property string carrierName: "AIS 4G+"
    property string preferredNetworkMode: "4G LTE"
    property string apnName: "AIS Internet (internet)"
    property bool volteEnabled: true
    property bool dataRoaming: false

    readonly property var wallpapers: [
        {
            id: "aurora",
            name: "Aurora",
            image: Qt.resolvedUrl("../../assets/wallpapers/aurora.png"),
            top: "#060B18",
            mid: "#0C1B2E",
            bottom: "#050C16",
            orb1: "#38BDF8",
            orb2: "#34D399",
            orb3: "#818CF8"
        },
        {
            id: "nebula",
            name: "Nebula",
            image: Qt.resolvedUrl("../../assets/wallpapers/nebula.png"),
            top: "#0B0616",
            mid: "#170D2B",
            bottom: "#07040E",
            orb1: "#C084FC",
            orb2: "#F472B6",
            orb3: "#38BDF8"
        },
        {
            id: "sunset",
            name: "Dune",
            image: Qt.resolvedUrl("../../assets/wallpapers/sunset.png"),
            top: "#180B16",
            mid: "#2B1420",
            bottom: "#0B070B",
            orb1: "#F59E0B",
            orb2: "#FB7185",
            orb3: "#FBBF24"
        },
        {
            id: "emerald",
            name: "Alpine",
            image: Qt.resolvedUrl("../../assets/wallpapers/emerald.png"),
            top: "#061312",
            mid: "#0C2421",
            bottom: "#040B0A",
            orb1: "#10B981",
            orb2: "#2DD4BF",
            orb3: "#6EE7B7"
        },
        {
            id: "oled",
            name: "OLED",
            image: Qt.resolvedUrl("../../assets/wallpapers/oled.png"),
            top: "#080A10",
            mid: "#030407",
            bottom: "#000000",
            orb1: "#60A5FA",
            orb2: "#94A3B8",
            orb3: "#334155"
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

    // Glyph/text colour for content drawn ON the accent fill. Named
    // "accentContent", never "on<Capitalized>": QML parses that shape as a signal
    // handler for the sibling `accentColor` property, the declaration never gets
    // its initialiser, and the value silently stays black — which is why
    // `onAccentColor` below renders black to this day (Theme.qml documents the
    // same trap for `primaryContent`). Fixing the remaining callers is a separate
    // sweep across the legacy apps.
    readonly property color accentContent: accentId === "mono" ? "#09090B" : "#FFFFFF"
    // Legacy name kept for the apps that still bind it; do not use in new code.
    readonly property color onAccentColor: accentId === "mono" ? "#09090B" : "#FFFFFF"

    // Accent colour for glyphs drawn on an inverted "on" well (Theme.text fill)
    // in the shade's tiles and vertical sliders. The mono accent is nearly white,
    // so on the dark theme it would vanish on a light well — fall back to the
    // shell background there. The light theme inverts the well (dark fill), so
    // the accent itself is already legible.
    readonly property color accentInk: {
        var a = accentColor
        var luma = 0.2126 * a.r + 0.7152 * a.g + 0.0722 * a.b
        return (luma > 0.8 && !lightTheme) ? Theme.bg : a
    }

    readonly property string prefsPath: {
        var home = Quickshell.env("HOME") || "/home/rabbiz"
        return home + "/.config/rabbiz-shell/prefs.json"
    }

    function applyJson(raw) {
        if (!raw || raw.trim().length === 0)
            return
        try {
            var d = JSON.parse(raw)
            if (d.brightnessLevel !== undefined) {
                var savedLevel = Number(d.brightnessLevel)
                if (isFinite(savedLevel) && savedLevel >= 0)
                    brightnessLevel = Math.max(0, Math.min(100, Math.round(savedLevel)))
            }
            if (d.wallpaperId !== undefined) wallpaperId = String(d.wallpaperId)
            if (d.accentId !== undefined) accentId = String(d.accentId)
            if (d.lightTheme !== undefined) lightTheme = Boolean(d.lightTheme)
            if (d.clockStyle !== undefined) clockStyle = String(d.clockStyle)
            if (d.use24h !== undefined) use24h = Boolean(d.use24h)
            if (d.dndEnabled !== undefined) dndEnabled = Boolean(d.dndEnabled)
            if (d.iconStyle !== undefined) iconStyle = String(d.iconStyle)
            if (d.sleepTimeoutSec !== undefined) sleepTimeoutSec = Math.max(0, parseInt(d.sleepTimeoutSec) || 0)
            if (d.fingerprintEnabled !== undefined) fingerprintEnabled = Boolean(d.fingerprintEnabled)
            if (d.fingerprintWakeUnlock !== undefined) fingerprintWakeUnlock = Boolean(d.fingerprintWakeUnlock)
            if (d.enrolledFingerCount !== undefined) enrolledFingerCount = Math.max(0, parseInt(d.enrolledFingerCount) || 1)
            if (d.fingerprintHaptic !== undefined) fingerprintHaptic = Boolean(d.fingerprintHaptic)
            if (d.carrierName !== undefined) carrierName = String(d.carrierName)
            if (d.preferredNetworkMode !== undefined) preferredNetworkMode = String(d.preferredNetworkMode)
            if (d.apnName !== undefined) apnName = String(d.apnName)
            if (d.volteEnabled !== undefined) volteEnabled = Boolean(d.volteEnabled)
            if (d.dataRoaming !== undefined) dataRoaming = Boolean(d.dataRoaming)
        } catch (e) {}
    }

    function save() {
        var payload = JSON.stringify({
            brightnessLevel: brightnessLevel,
            wallpaperId: wallpaperId,
            accentId: accentId,
            lightTheme: lightTheme,
            clockStyle: clockStyle,
            use24h: use24h,
            dndEnabled: dndEnabled,
            iconStyle: iconStyle,
            sleepTimeoutSec: sleepTimeoutSec,
            fingerprintEnabled: fingerprintEnabled,
            fingerprintWakeUnlock: fingerprintWakeUnlock,
            enrolledFingerCount: enrolledFingerCount,
            fingerprintHaptic: fingerprintHaptic,
            carrierName: carrierName,
            preferredNetworkMode: preferredNetworkMode,
            apnName: apnName,
            volteEnabled: volteEnabled,
            dataRoaming: dataRoaming
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
        onLoaded: {
            prefs.applyJson(text())
            prefs.loaded = true
        }
        onFileChanged: reload()
    }
}
