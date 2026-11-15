pragma Singleton
import QtQuick
import Quickshell.Io

// Screen brightness via systemd-logind D-Bus (busctl), falling back to
// brightnessctl / a direct sysfs write. No backlight driver exists on the
// A21s yet (M5) — probe fails, `available` stays false and the UI shows
// the slider disabled. Contract: 0-100 integer level.
QtObject {
    id: root

    property int level: 60
    property bool available: false   // false until a backlight device exists

    // internals: _backend is "sysfs" | "brightnessctl" | ""
    property string _backend: ""
    property string _dev: ""         // /sys/class/backlight/<_dev>
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

    function setLevel(v) {
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
              + "backlight " + root._dev + " " + raw + " 2>/dev/null "
              + "|| brightnessctl -d " + root._dev + " set " + v + "% 2>/dev/null "
              + "|| echo " + raw + " > /sys/class/backlight/" + root._dev
              + "/brightness 2>/dev/null || true"]
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
