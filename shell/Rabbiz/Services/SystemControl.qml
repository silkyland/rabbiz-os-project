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
    property bool screenLandscape: false
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
    property int screenshotSerial: 0   // +1 per captured shot; drives the shutter flash
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
        if (ShellState.sleeping)                  // do not fork anything while asleep
            return
        if (!telemetryProc.running)
            telemetryProc.running = true
    }

    function setAirplaneMode(on) {
        root.airplaneMode = Boolean(on)
        Network.setWifiEnabled(!root.airplaneMode)
        Bluetooth.setPowered(!root.airplaneMode)
        // Radios by name, never `rfkill unblock all`: that also silently unblocks every
        // other rfkill switch on the device, and gamer cannot open /dev/rfkill anyway.
        Network.setWwanEnabled(!root.airplaneMode)
        root.mobileData = !root.airplaneMode
        syncTelephonyState()
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
                    Brightness.setTransientLevel(100)
                ShellState.torchOverlayOpen = true
            } else {
                ShellState.torchOverlayOpen = false
                if (root._hadSavedBrightness && Brightness.available)
                    Brightness.setTransientLevel(root._savedBrightness)
                root._hadSavedBrightness = false
            }
        }
    }

    // There is no accelerometer driver, so this is a manual screen rotation, not
    // auto-rotate; the UI labels it that way (review finding "fake auto-rotate").
    function toggleLandscape() {
        root.screenLandscape = !root.screenLandscape
        _runCmd(["sh", "-c",
            "if command -v wlr-randr >/dev/null 2>&1; then "
          + "wlr-randr --output DSI-1 --transform " + (root.screenLandscape ? "90" : "normal") + " 2>/dev/null || true; "
          + "fi"])
    }

    function toggleMobileData() {
        if (root.airplaneMode)
            return
        root.mobileData = !root.mobileData
        syncTelephonyState()
        // `mmcli --disable` powers the whole modem down (no voice, no SMS). The radio
        // switch only takes the data bearer away.
        Network.setWwanEnabled(root.mobileData)
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
        _runCmd(["rabbiz-osk-ctl", "toggle"])
    }

    function nextOskLanguage() {
        _runCmd(["rabbiz-osk-ctl", "lang-next"])
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
        var restoreGov = root._preSleepGovernor.length > 0 ? root._preSleepGovernor : "schedutil"
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

    function reloadShell() {
        ShellState.powerMenuOpen = false
        _runCmd(["sh", "-c",
            "OLD=$PPID; (sleep 0.15; kill \"$OLD\" 2>/dev/null || true; sleep 0.30; "
          + "exec env QT_QUICK_CONTROLS_MOBILE=1 "
          + "QML_IMPORT_PATH=/usr/share/rabbiz-shell QML2_IMPORT_PATH=/usr/share/rabbiz-shell "
          + "setsid -f rabbiz-shell >/dev/null 2>&1 </dev/null) &"])
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
            _cmdQueue.push(argv)             // FIFO: earlier commands are not dropped
        }
    }

    property var _cmdQueue: []

    property var actionProc: Process {
        environment: Appearance.appEnvironment
        function _drain() {
            if (root._cmdQueue.length > 0 && !actionProc.running) {
                var argv = root._cmdQueue.shift()
                actionProc.command = argv
                actionProc.running = true
            }
        }
        stdout: StdioCollector { onStreamFinished: actionProc._drain() }
        stderr: StdioCollector { onStreamFinished: actionProc._drain() }
    }

    property var telemetryProc: Process {
        // a21s-telemetry reads procfs/sysfs with bash builtins only: one process, no
        // per-value forks (the old sh -c chain forked ~18 processes every 3 s).
        command: ["a21s-telemetry"]

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
                ? ["rabbiz-screenshot", "--delay", String(d)]
                : ["rabbiz-screenshot"]
            shotProc.running = true
        }
    }

    function openGallery(imagePath) {
        root.screenshotToastVisible = false
        var p = imagePath || ""
        _runCmd(["sh", "-c", "xdg-open \"$1\" >/dev/null 2>&1 &", "_", p])
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
            root.screenshotSerial++
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
    property var shotProc: Process { environment: Appearance.appEnvironment }

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
        running: !ShellState.sleeping        // no polling while the phone sleeps
        repeat: true
        onTriggered: root.refreshTelemetry()
    }

    // QtObject has no default property, so the watcher must be a property.
    property var _sleepWatch: Connections {
        target: ShellState
        function onSleepingChanged() {
            if (!ShellState.sleeping)
                root.refreshTelemetry()      // resume polling at wake
        }
    }

    Component.onCompleted: {
        root.refreshTelemetry()
        powerCmdInitProc.running = true
        volStateInitProc.running = true
        shotStateInitProc.running = true
    }
}
