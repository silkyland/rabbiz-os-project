pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Hardware telemetry, radio/airplane mode, torch/flashlight, DND,
// CPU governor & power actions backed by Linux sysfs / procfs / systemd
// with graceful interactive placeholders where hardware drivers are pending.
QtObject {
    id: root

    // --- Live telemetry (polled every 3s) ---
    property int cpuPercent: 0
    property int memPercent: 0
    property string memUsedText: "-- / --"
    property int cpuFreqMhz: 0
    property string cpuGovernor: "performance"
    property string diskFreeText: "--"

    // --- Quick settings / hardware state ---
    property bool airplaneMode: false
    readonly property bool dndEnabled: ShellPrefs.dndEnabled
    property bool flashlightOn: false
    property bool flashlightHardware: false
    property bool autoRotate: false
    property bool mobileData: true

    // --- Internals ---
    property real _prevTotal: 0
    property real _prevIdle: 0
    property int _savedBrightness: 60
    property bool _hadSavedBrightness: false
    property string _torchPath: ""
    property int _preSleepBrightness: 75
    property string _preSleepGovernor: "performance"
    property string _lastPowerCmd: ""
    property bool _powerCmdPrimed: false

    readonly property string _powerCmdPath: {
        var rt = Quickshell.env("XDG_RUNTIME_DIR") || "/run/user/1001"
        return rt + "/a21s-power.cmd"
    }

    function refreshTelemetry() {
        if (!telemetryProc.running)
            telemetryProc.running = true
    }

    function setAirplaneMode(on) {
        root.airplaneMode = Boolean(on)
        Network.setWifiEnabled(!root.airplaneMode)
        Bluetooth.setPowered(!root.airplaneMode)
        if (root.airplaneMode)
            root.mobileData = false
        _runCmd(["sh", "-c",
            "rfkill " + (root.airplaneMode ? "block" : "unblock") + " all 2>/dev/null || true"])
    }

    function setDndEnabled(on) {
        var next = Boolean(on)
        if (ShellPrefs.dndEnabled !== next) {
            ShellPrefs.dndEnabled = next
            ShellPrefs.save()
        }
    }

    function setFlashlight(on) {
        var next = Boolean(on)
        root.flashlightOn = next
        if (root.flashlightHardware && root._torchPath.length > 0) {
            var val = next ? "255" : "0"
            _runCmd(["sh", "-c",
                "echo " + val + " > " + root._torchPath + "/brightness 2>/dev/null || true"])
            ShellState.torchOverlayOpen = false
        } else {
            // Built-in OLED screen torch fallback when kernel camera flash LED is not yet wired
            if (next) {
                root._savedBrightness = Brightness.level
                root._hadSavedBrightness = true
                if (Brightness.available)
                    Brightness.setLevel(100)
                ShellState.torchOverlayOpen = true
            } else {
                ShellState.torchOverlayOpen = false
                if (root._hadSavedBrightness && Brightness.available)
                    Brightness.setLevel(root._savedBrightness)
                root._hadSavedBrightness = false
            }
        }
    }

    function toggleAutoRotate() {
        root.autoRotate = !root.autoRotate
        _runCmd(["sh", "-c",
            "if command -v wlr-randr >/dev/null 2>&1; then "
          + "wlr-randr --output DSI-1 --transform " + (root.autoRotate ? "90" : "normal") + " 2>/dev/null || true; "
          + "fi"])
    }

    function toggleMobileData() {
        if (root.airplaneMode)
            return
        root.mobileData = !root.mobileData
        if (Telephony.modemAvailable) {
            _runCmd(["sh", "-c",
                "mmcli -m 0 " + (root.mobileData ? "--enable" : "--disable") + " 2>/dev/null || true"])
        }
    }

    function setGovernor(name) {
        if (!name || name.length === 0)
            return
        root.cpuGovernor = name
        _runCmd(["sh", "-c",
            "for g in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do "
          + "echo " + name + " > \"$g\" 2>/dev/null || true; done"])
    }

    function toggleGovernor() {
        setGovernor(root.cpuGovernor === "performance" ? "schedutil" : "performance")
    }

    function _applyDisplayPower(blPower, brightnessRaw, gov, savedRaw) {
        var saveSnippet = savedRaw >= 0
            ? "RT=\"${XDG_RUNTIME_DIR:-/run/user/1001}\"; "
              + "mkdir -p \"$RT\" 2>/dev/null || true; "
              + "printf '%s\\n' '" + savedRaw + "' > \"$RT/a21s-saved-brightness\"; "
            : ""
        var govSnippet = (gov && gov.length > 0)
            ? "; for g in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do "
              + "echo " + gov + " > \"$g\" 2>/dev/null || true; done"
            : ""
        var blOrder = blPower === 0
            ? "echo 0 > \"$b/bl_power\" 2>/dev/null || true; echo " + brightnessRaw + " > \"$b/brightness\" 2>/dev/null || true"
            : "echo " + brightnessRaw + " > \"$b/brightness\" 2>/dev/null || true; echo " + blPower + " > \"$b/bl_power\" 2>/dev/null || true"
        powerProc.command = ["sh", "-c",
            saveSnippet
          + "for b in /sys/class/backlight/*; do [ -d \"$b\" ] || continue; "
          + blOrder + "; done"
          + govSnippet]
        powerProc.running = true
    }

    function lockScreen() {
        ShellState.powerMenuOpen = false
        ShellState.shadeProgress = 0
        ShellState.lockScreenOpen = true
    }

    function unlockScreen() {
        if (ShellState.sleeping)
            wake()
        ShellState.lockScreenOpen = false
    }

    function sleep() {
        if (ShellState.sleeping)
            return
        if (root.flashlightOn)
            root.setFlashlight(false)
        var lvl = Brightness.level > 5 ? Brightness.level
            : (root._preSleepBrightness > 5 ? root._preSleepBrightness : 75)
        root._preSleepBrightness = lvl
        if (root.cpuGovernor && root.cpuGovernor !== "schedutil")
            root._preSleepGovernor = root.cpuGovernor

        ShellState.powerMenuOpen = false
        ShellState.shadeProgress = 0
        ShellState.lockScreenOpen = true
        ShellState.sleeping = true
        root.cpuGovernor = "schedutil"
        Brightness.level = 0

        var rawMax = Brightness._max > 0 ? Brightness._max : 255
        var savedRaw = Math.max(12, Math.round(lvl / 100 * rawMax))
        _applyDisplayPower(4, 0, "schedutil", savedRaw)
    }

    function wake() {
        var restoreLvl = root._preSleepBrightness > 5 ? root._preSleepBrightness : 75
        var restoreGov = root._preSleepGovernor.length > 0 ? root._preSleepGovernor : "performance"
        ShellState.sleeping = false
        ShellState.lockScreenOpen = true
        root.cpuGovernor = restoreGov
        Brightness.level = restoreLvl

        var rawMax = Brightness._max > 0 ? Brightness._max : 255
        var restoreRaw = Math.max(15, Math.round(restoreLvl / 100 * rawMax))
        _applyDisplayPower(0, restoreRaw, restoreGov, -1)
        Brightness.postSet.restart()
    }

    function toggleSleep() {
        if (ShellState.sleeping)
            wake()
        else
            sleep()
    }

    function handlePowerCommand(raw) {
        var trimmed = (raw || "").trim()
        if (trimmed.length === 0)
            return
        if (!root._powerCmdPrimed) {
            root._powerCmdPrimed = true
            root._lastPowerCmd = trimmed
            return
        }
        if (trimmed === root._lastPowerCmd)
            return
        root._lastPowerCmd = trimmed

        var colon = trimmed.indexOf(":")
        var act = (colon >= 0 ? trimmed.substring(0, colon) : trimmed).trim()
        if (act === "toggle")
            toggleSleep()
        else if (act === "sleep")
            sleep()
        else if (act === "wake")
            wake()
        else if (act === "lock")
            lockScreen()
        else if (act === "menu") {
            if (ShellState.sleeping)
                wake()
            ShellState.powerMenuOpen = true
        }
    }

    function switchToSteamOS() {
        ShellState.powerMenuOpen = false
        _runCmd(["sh", "-c", "a21s-steam >/dev/null 2>&1 &"])
    }

    function reloadShell() {
        ShellState.powerMenuOpen = false
        _runCmd(["sh", "-c",
            "OLD=$PPID; (sleep 0.15; kill \"$OLD\" 2>/dev/null || true; sleep 0.30; "
          + "exec env QT_QUICK_BACKEND=software QT_QUICK_CONTROLS_MOBILE=1 "
          + "QML_IMPORT_PATH=/usr/share/a21s-shell QML2_IMPORT_PATH=/usr/share/a21s-shell "
          + "setsid -f quickshell -p /usr/share/a21s-shell/shell.qml >/dev/null 2>&1 </dev/null) &"])
    }

    function reboot() {
        ShellState.powerMenuOpen = false
        _runCmd(["systemctl", "reboot"])
    }

    function powerOff() {
        ShellState.powerMenuOpen = false
        _runCmd(["systemctl", "poweroff"])
    }

    function _runCmd(argv) {
        if (!actionProc.running) {
            actionProc.command = argv
            actionProc.running = true
        } else {
            _queuedCmd = argv
        }
    }

    property var _queuedCmd: null

    property var actionProc: Process {
        function _drain() {
            if (root._queuedCmd !== null && !actionProc.running) {
                var argv = root._queuedCmd
                root._queuedCmd = null
                actionProc.command = argv
                actionProc.running = true
            }
        }
        stdout: StdioCollector { onStreamFinished: actionProc._drain() }
        stderr: StdioCollector { onStreamFinished: actionProc._drain() }
    }

    property var telemetryProc: Process {
        command: ["sh", "-c",
            "head -n1 /proc/stat 2>/dev/null; "
          + "echo '=M='; "
          + "grep -E '^(MemTotal|MemAvailable):' /proc/meminfo 2>/dev/null; "
          + "echo '=C='; "
          + "cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq 2>/dev/null || echo 0; "
          + "cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo performance; "
          + "echo '=D='; "
          + "df -h / 2>/dev/null | awk 'NR==2{print $4}'; "
          + "echo '=L='; "
          + "ls -d /sys/class/leds/*torch* /sys/class/leds/*flash* 2>/dev/null | head -n1"]

        stdout: StdioCollector {
            onStreamFinished: {
                var parts = this.text.split("=M=")
                var statLine = (parts[0] || "").trim()
                var afterM = (parts[1] || "").split("=C=")
                var memBlock = afterM[0] || ""
                var afterC = (afterM[1] || "").split("=D=")
                var cpuBlock = (afterC[0] || "").trim().split("\n")
                var afterD = (afterC[1] || "").split("=L=")
                var diskStr = (afterD[0] || "").trim()
                var ledPath = (afterD[1] || "").trim()

                // 1) CPU usage delta from /proc/stat
                var cols = statLine.split(/\s+/)
                if (cols.length >= 5 && cols[0] === "cpu") {
                    var total = 0
                    for (var i = 1; i < cols.length; ++i) {
                        var v = parseFloat(cols[i])
                        if (!isNaN(v)) total += v
                    }
                    var idle = (parseFloat(cols[4]) || 0) + (parseFloat(cols[5]) || 0)
                    if (root._prevTotal > 0 && total > root._prevTotal) {
                        var dt = total - root._prevTotal
                        var di = idle - root._prevIdle
                        root.cpuPercent = Math.max(0, Math.min(100, Math.round(100 * (dt - di) / dt)))
                    }
                    root._prevTotal = total
                    root._prevIdle = idle
                }

                // 2) Memory usage
                var mTotal = 0, mAvail = 0
                var ml = memBlock.split("\n")
                for (var j = 0; j < ml.length; ++j) {
                    var line = ml[j].trim()
                    if (line.indexOf("MemTotal:") === 0)
                        mTotal = parseInt(line.replace(/\D+/g, "")) || 0
                    else if (line.indexOf("MemAvailable:") === 0)
                        mAvail = parseInt(line.replace(/\D+/g, "")) || 0
                }
                if (mTotal > 0) {
                    var usedKb = Math.max(0, mTotal - mAvail)
                    root.memPercent = Math.max(0, Math.min(100, Math.round(usedKb * 100 / mTotal)))
                    root.memUsedText = (usedKb / 1048576).toFixed(1) + " / " + (mTotal / 1048576).toFixed(1) + " GB"
                }

                // 3) CPU frequency & governor
                if (cpuBlock.length >= 1) {
                    var khz = parseInt(cpuBlock[0])
                    if (!isNaN(khz) && khz > 0)
                        root.cpuFreqMhz = Math.round(khz / 1000)
                }
                if (cpuBlock.length >= 2 && cpuBlock[1].trim().length > 0)
                    root.cpuGovernor = cpuBlock[1].trim()

                // 4) Rootfs free space & torch LED path
                if (diskStr.length > 0)
                    root.diskFreeText = diskStr
                root._torchPath = ledPath
                root.flashlightHardware = ledPath.length > 0
            }
        }
    }

    property var powerProc: Process {}

    property var powerCmdInitProc: Process {
        command: ["sh", "-c",
            "F=\"$1\"; mkdir -p \"$(dirname \"$F\")\" 2>/dev/null || true; "
          + "[ -f \"$F\" ] || printf 'idle:0\\n' > \"$F\"; echo ok",
            "_", root._powerCmdPath]
        stdout: StdioCollector {
            onStreamFinished: {
                powerCmdWatcher.reload()
            }
        }
    }

    property var powerCmdWatcher: FileView {
        path: root._powerCmdPath
        watchChanges: true
        onLoaded: root.handlePowerCommand(text())
        onFileChanged: reload()
    }

    property var _pollTimer: Timer {
        interval: 3000
        running: true
        repeat: true
        onTriggered: root.refreshTelemetry()
    }

    Component.onCompleted: {
        root.refreshTelemetry()
        powerCmdInitProc.running = true
    }
}
