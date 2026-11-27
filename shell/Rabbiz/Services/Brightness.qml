pragma Singleton
import QtQuick
import Quickshell.Io

// Screen brightness via systemd-logind D-Bus (busctl), falling back to
// brightnessctl / a direct sysfs write. Restore the user-selected level after
// both preferences and the backlight are ready. Contract: 0-100 integer level.
QtObject {
    id: root

    property int level: 60
    property bool available: false   // false until a backlight device exists

    // internals: _backend is "sysfs" | "brightnessctl" | ""
    property string _backend: ""
    property string _dev: ""         // /sys/class/backlight/<_dev>
    property bool _restored: false
    property int _max: 0             // max_brightness (sysfs backend)

    function refresh() {
        if (root._backend === "sysfs") {
            if (!readProc.running)
                readProc.running = true
        } else if (root._backend === "brightnessctl") {
            if (!bcRead.running)
                bcRead.running = true
        } else if (!probeProc.running) {
            probeProc.running = true
        }
    }

    function restoreSavedLevel() {
        if (root._restored || !root.available || !ShellPrefs.loaded)
            return
        root._restored = true
        if (ShellPrefs.brightnessLevel < 0) {
            // Migrate a readable current level; never learn the sleep/off value.
            ShellPrefs.brightnessLevel = root.level > 5 ? root.level : 60
            ShellPrefs.save()
        }
        root.setTransientLevel(ShellPrefs.brightnessLevel)
    }

    onAvailableChanged: root.restoreSavedLevel()

    property var prefsReady: Connections {
        target: ShellPrefs
        function onLoadedChanged() { root.restoreSavedLevel() }
    }

    function setLevel(v) {
        if (!isFinite(v)) return
        v = Math.max(0, Math.min(100, Math.round(v)))
        if (ShellPrefs.brightnessLevel !== v) {
            ShellPrefs.brightnessLevel = v
            saveLevel.restart()
        }
        root.setTransientLevel(v)
    }

    property var saveLevel: Timer {
        interval: 250
        onTriggered: ShellPrefs.save()
    }

    // Screen torch/restore must not replace the user's normal preference.
    function setTransientLevel(v) {
        v = Math.max(0, Math.min(100, Math.round(v)))
        root.level = v                // optimistic; refresh() reconciles
        if (!root.available)
            return
        if (root._backend === "sysfs" && root._dev.length > 0 && root._max > 0) {
            var raw = Math.round(v / 100 * root._max)
            setProc.command = ["sh", "-c",
                "busctl call org.freedesktop.login1 "
              + "/org/freedesktop/login1/session/auto "
              + "org.freedesktop.login1.Session SetBrightness ssu "
              + "backlight \"$1\" \"$2\" 2>/dev/null "
              + "|| brightnessctl -d \"$1\" set \"$3%\" 2>/dev/null "
              + "|| printf '%s\\n' \"$2\" > \"/sys/class/backlight/$1/brightness\" 2>/dev/null || true",
                "_", root._dev, String(raw), String(v)]
            if (!setProc.running)
                setProc.running = true
        } else if (root._backend === "brightnessctl") {
            setProc.command = ["brightnessctl", "set", v + "%"]
            if (!setProc.running)
                setProc.running = true
        }
        postSet.restart()
    }

    // --- probe: find a backlight device (sysfs first, brightnessctl fallback)
    property var probeProc: Process {
        command: ["sh", "-c",
            "d=$(ls -1 /sys/class/backlight/ 2>/dev/null | head -n1); "
          + "echo \"D:$d\"; "
          + "brightnessctl -m 2>/dev/null | head -n1"]

        stdout: StdioCollector {
            onStreamFinished: {
                var lines = this.text.split("\n")
                var dev = ""
                var bcLine = ""
                for (var i = 0; i < lines.length; ++i) {
                    var t = lines[i].trim()
                    if (t.indexOf("D:") === 0)
                        dev = t.substring(2)
                    else if (t.length > 0 && bcLine === "")
                        bcLine = t
                }
                if (dev.length > 0) {
                    root._backend = "sysfs"
                    root._dev = dev
                    root.refresh()
                } else if (bcLine.length > 0) {
                    var f = bcLine.split(",")   // name,class,cur,pct,max
                    if (f.length >= 5) {
                        root._backend = "brightnessctl"
                        root._dev = f[0]
                        root._max = parseInt(f[4]) || 0
                        var pct = parseInt(f[3])
                        if (!isNaN(pct))
                            root.level = Math.max(0, Math.min(100, pct))
                        root.available = true
                    }
                } else {
                    root._backend = ""
                    root.available = false
                }
            }
        }
    }

    // --- sysfs backend: read max + current -------------------------------
    property var readProc: Process {
        command: ["sh", "-c",
            "cat /sys/class/backlight/" + root._dev + "/max_brightness "
          + "/sys/class/backlight/" + root._dev + "/brightness 2>/dev/null"]

        stdout: StdioCollector {
            onStreamFinished: {
                var lines = this.text.trim().split("\n")
                if (lines.length >= 2) {
                    var mx = parseInt(lines[0])
                    var cur = parseInt(lines[1])
                    if (!isNaN(mx) && mx > 0 && !isNaN(cur)) {
                        root._max = mx
                        root.level = Math.max(0, Math.min(100,
                            Math.round(cur * 100 / mx)))
                        root.available = true
                    }
                }
            }
        }
    }

    // --- brightnessctl backend: re-read ----------------------------------
    property var bcRead: Process {
        command: ["brightnessctl", "-m"]

        stdout: StdioCollector {
            onStreamFinished: {
                var f = this.text.trim().split(",")
                if (f.length >= 5) {
                    var pct = parseInt(f[3])
                    if (!isNaN(pct))
                        root.level = Math.max(0, Math.min(100, pct))
                    root.available = true
                }
            }
        }
    }

    // --- writes -----------------------------------------------------------
    property var setProc: Process {}

    // Read back shortly after a write to confirm.
    property var postSet: Timer {
        interval: 700
        onTriggered: root.refresh()
    }

    // Cheap periodic sync for changes made outside the shell.
    property var _timerN: Timer {
        interval: 10000
        running: true
        repeat: true
        onTriggered: root.refresh()
    }

    Component.onCompleted: root.refresh()
}
