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

    // --- Biometric fingerprint (ET520) & Shannon CP modem (s318ap) ---
    property bool fingerprintSupported: false
    property string fingerprintChip: "ET520"
    property string fingerprintStatus: "ready"
    property int fingerprintIrqCount: 0
    property int _lastFpIrqCount: -1
    property string fpState: "idle" // idle | scanning | success | error

    property bool cpifLoaded: false
    property string cpModemName: "s318ap"
    property string cpPhoneState: "OFFLINE"
    property string cpRmemInfo: "0xd0000000 0x8800000"
    property string cpTocSummary: "unloaded"
    property bool cpMemMapOnCp: false
    property string cpRegionsSummary: ""
    property string modemNodesSummary: ""

    // --- Volume OSD & Screenshot Toast ---
    property int volumeLevel: 75
    property bool volumeMuted: false
    property bool volumeOsdVisible: false
    property bool screenshotToastVisible: false
    property string lastScreenshotPath: ""
    property string lastScreenshotName: ""

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
    property string _lastVolNonce: ""
    property bool _volStatePrimed: false
    property string _lastShotNonce: ""
    property bool _shotStatePrimed: false
    property int _pendingVolume: -1

    readonly property string _powerCmdPath: {
        var rt = Quickshell.env("XDG_RUNTIME_DIR") || "/run/user/1001"
        return rt + "/a21s-power.cmd"
    }
    readonly property string _volStatePath: {
        var rt = Quickshell.env("XDG_RUNTIME_DIR") || "/run/user/1001"
        return rt + "/a21s-volume.state"
    }
    readonly property string _shotStatePath: {
        var rt = Quickshell.env("XDG_RUNTIME_DIR") || "/run/user/1001"
        return rt + "/a21s-screenshot.state"
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
        syncTelephonyState()
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
        syncTelephonyState()
        if (Telephony.modemAvailable) {
            _runCmd(["sh", "-c",
                "mmcli -m 0 " + (root.mobileData ? "--enable" : "--disable") + " 2>/dev/null || true"])
        }
    }

    function syncTelephonyState() {
        if (!Telephony.modemAvailable && !root.cpifLoaded)
            return
        if (root.airplaneMode) {
            Telephony.operatorName = "Airplane Mode"
            Telephony.signalStrength = 0
            return
        }
        var online = root.cpPhoneState === "ONLINE"
        var modeSuffix = ShellPrefs.preferredNetworkMode ? (" · " + ShellPrefs.preferredNetworkMode) : ""
        Telephony.operatorName = root.mobileData
            ? (ShellPrefs.carrierName + modeSuffix)
            : (ShellPrefs.carrierName + " (Data Off)")
        Telephony.signalStrength = online ? (root.mobileData ? 86 : 68) : 45
    }

    // --- On-screen keyboard (squeekboard via sm.puri.OSK0 / gsettings) ---

    function toggleOsk() {
        _runCmd(["a21s-osk-ctl", "toggle"])
    }

    function nextOskLanguage() {
        _runCmd(["a21s-osk-ctl", "lang-next"])
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
        else if (act === "fp-unlock")
            triggerFingerprintUnlock(false)
        else if (act === "fp-fail")
            triggerFingerprintFail()
        else if (act === "menu") {
            if (ShellState.sleeping)
                wake()
            ShellState.powerMenuOpen = true
        }
    }

    function triggerFingerprintUnlock(pulseSysfs) {
        if (!ShellPrefs.fingerprintEnabled)
            return
        if (root.fpState === "success")
            return
        if (ShellState.sleeping && !ShellPrefs.fingerprintWakeUnlock && pulseSysfs === false)
            return

        ShellState.powerMenuOpen = false
        fpResetTimer.stop()
        if (ShellState.sleeping)
            wake()
        root.fpState = "success"

        if (pulseSysfs !== false) {
            root._lastFpIrqCount = root.fingerprintIrqCount + 1
            _runCmd(["sh", "-c",
                "printf '1\\n' > /sys/kernel/a21s_fp/trigger_unlock 2>/dev/null || true"])
        }
        fpUnlockTimer.restart()
    }

    function triggerFingerprintFail() {
        fpUnlockTimer.stop()
        root.fpState = "error"
        fpResetTimer.restart()
    }

    function resetFingerprintSensor() {
        _runCmd(["a21s-modem-ctl", "fp-reset"])
        root.refreshTelemetry()
    }

    function reloadModemFirmware() {
        _runCmd(["sh", "-c",
            "a21s-modem-ctl init 2>/dev/null || true"])
        postModemTimer.restart()
    }

    property var fpUnlockTimer: Timer {
        interval: 380
        onTriggered: {
            root.unlockScreen()
            root.fpState = "idle"
        }
    }

    property var fpResetTimer: Timer {
        interval: 750
        onTriggered: {
            if (root.fpState === "error")
                root.fpState = "idle"
        }
    }

    property var postModemTimer: Timer {
        interval: 1200
        onTriggered: root.refreshTelemetry()
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
          + "ls -d /sys/class/leds/*torch* /sys/class/leds/*flash* 2>/dev/null | head -n1; "
          + "echo '=FP='; "
          + "cat /sys/kernel/a21s_fp/chipid 2>/dev/null || echo none; "
          + "cat /sys/kernel/a21s_fp/status 2>/dev/null || echo offline; "
          + "cat /sys/kernel/a21s_fp/irq_count 2>/dev/null || echo 0; "
          + "echo '=CP='; "
          + "ls /dev/umts_* 2>/dev/null | tr '\\n' ' '; echo; "
          + "cat /sys/devices/platform/soc@0/14020000.cpif/modem_name 2>/dev/null || echo none; "
          + "cat /sys/devices/platform/soc@0/14020000.cpif/phone_state 2>/dev/null || echo OFFLINE; "
          + "cat /sys/devices/platform/soc@0/14020000.cpif/rmem_info 2>/dev/null || echo none; "
          + "cat /sys/devices/platform/soc@0/14020000.cpif/toc_summary 2>/dev/null || echo unloaded; "
          + "cat /sys/devices/platform/soc@0/14020000.cpif/mem_map_on_cp 2>/dev/null || echo 0; "
          + "cat /sys/devices/platform/soc@0/14020000.cpif/regions_summary 2>/dev/null || echo none"]

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
                var afterL = (afterD[1] || "").split("=FP=")
                var ledPath = (afterL[0] || "").trim()
                var afterFP = (afterL[1] || "").split("=CP=")
                var fpLines = (afterFP[0] || "").trim().split("\n")
                var cpLines = (afterFP[1] || "").trim().split("\n")

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

                // 5) Fingerprint sensor (EgisTech ET520)
                if (fpLines.length >= 3) {
                    var fpChip = fpLines[0].trim()
                    var fpStat = fpLines[1].trim()
                    var fpIrq = parseInt(fpLines[2].trim()) || 0
                    root.fingerprintSupported = fpChip.length > 0 && fpChip !== "none"
                    if (root.fingerprintSupported)
                        root.fingerprintChip = fpChip
                    root.fingerprintStatus = fpStat
                    var prevIrq = root._lastFpIrqCount
                    root.fingerprintIrqCount = fpIrq
                    if (prevIrq >= 0 && fpIrq > prevIrq
                            && ShellPrefs.fingerprintEnabled
                            && (ShellState.lockScreenOpen || ShellState.sleeping)) {
                        root._lastFpIrqCount = fpIrq
                        if (!ShellState.sleeping || ShellPrefs.fingerprintWakeUnlock)
                            root.triggerFingerprintUnlock(false)
                    } else {
                        root._lastFpIrqCount = fpIrq
                    }
                }

                // 6) Shannon CPIF Modem (s318ap)
                if (cpLines.length >= 7) {
                    var nodesStr = cpLines[0].trim()
                    var mName = cpLines[1].trim()
                    root.modemNodesSummary = nodesStr
                    root.cpModemName = mName !== "none" ? mName : "s318ap"
                    root.cpifLoaded = mName !== "none" && nodesStr.indexOf("/dev/umts_") >= 0
                    root.cpPhoneState = cpLines[2].trim()
                    root.cpRmemInfo = cpLines[3].trim()
                    root.cpTocSummary = cpLines[4].trim()
                    root.cpMemMapOnCp = cpLines[5].trim() === "1"
                    root.cpRegionsSummary = cpLines[6].trim()
                }
                root.syncTelephonyState()
            }
        }
    }

    property var powerProc: Process {}

    function setVolume(pct) {
        var clamped = Math.max(0, Math.min(100, Math.round(pct)))
        root.volumeLevel = clamped
        root.volumeMuted = false
        root._pendingVolume = clamped
        if (!ShellState.shadeOpen) {
            root.volumeOsdVisible = true
            volOsdTimer.restart()
        }
        volSetTimer.restart()
    }

    function adjustVolume(delta) {
        setVolume(root.volumeLevel + delta)
    }

    function toggleMute() {
        root.volumeMuted = !root.volumeMuted
        if (!ShellState.shadeOpen) {
            root.volumeOsdVisible = true
            volOsdTimer.restart()
        }
        if (!volProc.running) {
            volProc.command = ["a21s-volume", "mute"]
            volProc.running = true
        }
    }

    function takeScreenshot(delayMs) {
        var d = (delayMs && delayMs > 0) ? Math.round(delayMs) : 0
        if (!shotProc.running) {
            shotProc.command = d > 0
                ? ["a21s-screenshot", "--delay", String(d)]
                : ["a21s-screenshot"]
            shotProc.running = true
        }
    }

    function openGallery(imagePath) {
        root.screenshotToastVisible = false
        var p = imagePath || ""
        _runCmd(["sh", "-c", "a21s-gallery \"$1\" >/dev/null 2>&1 &", "_", p])
    }

    function _handleVolState(raw) {
        var lines = (raw || "").split("\n")
        var pct = -1
        var muted = root.volumeMuted
        var nonce = ""
        for (var i = 0; i < lines.length; ++i) {
            var line = lines[i].trim()
            if (line.indexOf("VOLUME=") === 0) {
                var parsed = parseInt(line.slice(7), 10)
                if (!isNaN(parsed))
                    pct = Math.max(0, Math.min(100, parsed))
            } else if (line.indexOf("MUTED=") === 0) {
                muted = (line.slice(6).trim() === "1")
            } else if (line.indexOf("NONCE=") === 0) {
                nonce = line.slice(6).trim()
            }
        }
        if (pct >= 0 && !volSetTimer.running)
            root.volumeLevel = pct
        root.volumeMuted = muted

        if (!root._volStatePrimed) {
            root._volStatePrimed = true
            root._lastVolNonce = nonce
            return
        }
        if (nonce.length > 0 && nonce !== root._lastVolNonce) {
            root._lastVolNonce = nonce
            if (!ShellState.shadeOpen) {
                root.volumeOsdVisible = true
                volOsdTimer.restart()
            }
        }
    }

    function _handleShotState(raw) {
        var trimmed = (raw || "").trim()
        if (trimmed.length === 0)
            return
        var parts = trimmed.split("|")
        if (parts.length < 3)
            return
        var path = parts[0]
        var name = parts[1]
        var nonce = parts[2]
        root.lastScreenshotPath = path
        root.lastScreenshotName = name
        if (!root._shotStatePrimed) {
            root._shotStatePrimed = true
            root._lastShotNonce = nonce
            return
        }
        if (nonce.length > 0 && nonce !== root._lastShotNonce) {
            root._lastShotNonce = nonce
            root.screenshotToastVisible = true
            shotToastTimer.restart()
        }
    }

    property var volProc: Process {
        onExited: {
            if (root._pendingVolume >= 0)
                volSetTimer.restart()
        }
    }
    property var shotProc: Process {}

    property var volSetTimer: Timer {
        interval: 80
        onTriggered: {
            if (root._pendingVolume >= 0 && !volProc.running) {
                var v = root._pendingVolume
                root._pendingVolume = -1
                volProc.command = ["a21s-volume", "set", String(v)]
                volProc.running = true
            }
        }
    }

    property var volOsdTimer: Timer {
        interval: 2500
        onTriggered: root.volumeOsdVisible = false
    }

    property var shotToastTimer: Timer {
        interval: 4000
        onTriggered: root.screenshotToastVisible = false
    }

    property var volStateInitProc: Process {
        command: ["sh", "-c",
            "F=\"$1\"; mkdir -p \"$(dirname \"$F\")\" 2>/dev/null || true; "
          + "if [ ! -f \"$F\" ]; then "
          + "  V=\"$(a21s-volume get 2>/dev/null || true)\"; "
          + "  PCT=\"$(printf '%s\\n' \"$V\" | awk -F= '/^VOLUME=/{print $2}')\"; "
          + "  MUT=\"$(printf '%s\\n' \"$V\" | awk -F= '/^MUTED=/{print $2}')\"; "
          + "  printf 'VOLUME=%s\\nMUTED=%s\\nNONCE=init\\n' \"${PCT:-75}\" \"${MUT:-0}\" > \"$F\"; "
          + "fi; echo ok",
            "_", root._volStatePath]
        stdout: StdioCollector {
            onStreamFinished: volStateWatcher.reload()
        }
    }

    property var volStateWatcher: FileView {
        path: root._volStatePath
        watchChanges: true
        onLoaded: root._handleVolState(text())
        onFileChanged: reload()
    }

    property var shotStateInitProc: Process {
        command: ["sh", "-c",
            "F=\"$1\"; mkdir -p \"$(dirname \"$F\")\" 2>/dev/null || true; "
          + "[ -f \"$F\" ] || printf '||init\\n' > \"$F\"; echo ok",
            "_", root._shotStatePath]
        stdout: StdioCollector {
            onStreamFinished: shotStateWatcher.reload()
        }
    }

    property var shotStateWatcher: FileView {
        path: root._shotStatePath
        watchChanges: true
        onLoaded: root._handleShotState(text())
        onFileChanged: reload()
    }

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
        volStateInitProc.running = true
        shotStateInitProc.running = true
    }
}
