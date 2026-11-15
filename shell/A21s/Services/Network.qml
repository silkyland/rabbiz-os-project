pragma Singleton
import QtQuick
import Quickshell.Io

// Wi-Fi state + control. Backed by NetworkManager via `nmcli`
// (Quickshell.Io.Process). usb0 must stay unmanaged — never touch it.
// Contract (do not change property/signal names):
QtObject {
    id: root

    property bool wifiEnabled: false
    property bool wifiConnected: false
    property string activeSsid: ""
    property int signalStrength: -1        // 0-100, -1 = unknown
    property var networks: []              // [{ssid, signal, security, inUse}]

    // --- Connectivity panel extensions (additive) ------------------------
    readonly property var wifiNetworks: networks   // alias kept for panel API
    property bool wifiScanning: false
    property bool wifiPanelActive: false   // gates the ~8s rescan timer
    property bool wifiConnecting: false
    property string wifiLastError: ""      // "" = no error; else last connect failure
    property string wifiConnectingSsid: ""
    property var savedSsids: []            // NM profile names (saved networks)

    // --- internals -------------------------------------------------------
    property string _wifiDevice: ""
    property var _pendingArgv: null
    property var _pendingConn: null
    property string _connOut: ""

    // Split an nmcli -t line on ':' honoring backslash escapes (\: and \\).
    function _splitNm(line) {
        var out = [], cur = "", esc = false
        for (var i = 0; i < line.length; ++i) {
            var c = line.charAt(i)
            if (esc)        { cur += c; esc = false }
            else if (c === "\\") { esc = true }
            else if (c === ":")  { out.push(cur); cur = "" }
            else                 { cur += c }
        }
        out.push(cur)
        return out
    }

    function refresh() {
        if (!pollProc.running)
            pollProc.running = true
    }

    function scan() {                        // rescan; refresh picks up results
        root.wifiScanning = true
        _run(["nmcli", "device", "wifi", "rescan"])
    }

    function setWifiEnabled(on) {
        _run(["nmcli", "radio", "wifi", on ? "on" : "off"])
    }

    // 'arg' -> 'arg' with embedded quotes escaped for sh -c use.
    function _shq(s) {
        return "'" + String(s).replace(/'/g, "'\\''") + "'"
    }

    function wifiConnect(ssid, password) {
        var cmd = "timeout 40 nmcli device wifi connect " + _shq(ssid)
        if (password && password.length > 0)
            cmd += " password " + _shq(password)
        root._connOut = ""
        root.wifiConnecting = true
        root.wifiConnectingSsid = ssid
        root.wifiLastError = ""
        _runConn(["sh", "-c", cmd])
    }

    // Back-compat alias (Settings app uses connectTo).
    function connectTo(ssid, password) {
        wifiConnect(ssid, password)
    }

    function wifiForget(ssid) {
        _run(["sh", "-c", "nmcli connection delete id " + _shq(ssid)])
    }

    function disconnect() {
        if (root._wifiDevice.length > 0)
            _run(["nmcli", "device", "disconnect", root._wifiDevice])
        else
            refresh()
    }

    function _run(argv) {
        retryTimer.kick(retryTimer.ticks > 0 ? retryTimer.ticks : 2)
        if (cmdProc.running) {
            root._pendingArgv = argv         // serialize: run after current
            return
        }
        cmdProc.command = argv
        cmdProc.running = true
    }

    // --- poll: radio state + wifi device + AP list (cached scan results) ---
    property var pollProc: Process {
        command: ["sh", "-c",
            "nmcli -t -f WIFI g 2>/dev/null; "
          + "echo '=D='; "
          + "nmcli -t -f DEVICE,TYPE,STATE dev 2>/dev/null; "
          + "echo '=A='; "
          + "nmcli -t -f IN-USE,SSID,SIGNAL,SECURITY dev wifi list 2>/dev/null; "
          + "echo '=S='; "
          + "nmcli -t -f NAME,TYPE con show 2>/dev/null"]

        stdout: StdioCollector {
            onStreamFinished: {
                var parts = this.text.split("=D=")
                var radio = parts[0]
                var rest = parts.length > 1 ? parts[1].split("=A=") : ["", ""]
                var devLines = rest[0]
                var tail = rest.length > 1 ? rest[1].split("=S=") : ["", ""]
                var apLines = tail[0]
                var savLines = tail.length > 1 ? tail[1] : ""

                // 1) radio on/off
                root.wifiEnabled = radio.trim() === "enabled"

                // 2) find the wifi device + its NM state
                var devState = ""
                var dl = devLines.split("\n")
                for (var i = 0; i < dl.length; ++i) {
                    var f = root._splitNm(dl[i].trim())
                    if (f.length >= 3 && f[1] === "wifi") {
                        root._wifiDevice = f[0]
                        devState = f[2]
                        break
                    }
                }

                // 3) AP list -> networks[], deduped by SSID (strongest wins)
                var list = [], seen = {}, conn = null
                var al = apLines.split("\n")
                for (var j = 0; j < al.length; ++j) {
                    var line = al[j].trim()
                    if (line.length === 0)
                        continue
                    var g = root._splitNm(line)
                    if (g.length < 3)
                        continue
                    var inUse = g[0] === "*"
                    var ssid = g[1]
                    var sig = parseInt(g[2])
                    var sec = g.length > 3 ? g[3] : ""
                    if (sec === "--")
                        sec = ""
                    if (ssid.length === 0)
                        continue                 // hidden network
                    if (isNaN(sig))
                        sig = -1
                    var prev = seen[ssid]
                    if (!prev) {
                        var e = { "ssid": ssid, "signal": sig,
                                  "security": sec, "inUse": inUse }
                        seen[ssid] = e
                        list.push(e)
                    } else {
                        // Merge duplicate-SSID APs: strongest signal wins,
                        // inUse/security are sticky so the connected entry
                        // is never displaced by a stronger non-inUse AP.
                        if (inUse || sig > prev.signal || prev.security.length === 0) {
                            var m = { "ssid": ssid,
                                      "signal": Math.max(sig, prev.signal),
                                      "security": sec.length > 0 ? sec : prev.security,
                                      "inUse": prev.inUse || inUse }
                            seen[ssid] = m
                            list[list.indexOf(prev)] = m
                        }
                    }
                    if (inUse && seen[ssid].inUse)
                        conn = seen[ssid]
                }
                list.sort(function(a, b) {
                    if (a.inUse !== b.inUse)
                        return a.inUse ? -1 : 1
                    return b.signal - a.signal
                })
                root.networks = list
                root.wifiScanning = false

                // 3b) saved wifi profiles (for "Saved" + passwordless reconnect)
                var saved = []
                var sl = savLines.split("\n")
                for (var s = 0; s < sl.length; ++s) {
                    var sf = root._splitNm(sl[s].trim())
                    if (sf.length >= 2 && sf[1].indexOf("wireless") !== -1)
                        saved.push(sf[0])
                }
                root.savedSsids = saved

                // 4) connected state
                if (conn !== null) {
                    root.wifiConnected = true
                    root.activeSsid = conn.ssid
                    root.signalStrength = conn.signal
                } else {
                    root.wifiConnected = devState === "connected"
                    if (!root.wifiConnected) {
                        root.activeSsid = ""
                        root.signalStrength = -1
                    }
                }
            }
        }
    }

    // --- one-shot commands (radio, rescan, connect, disconnect) ------------
    property var cmdProc: Process {
        function _dispatch() {
            if (root._pendingArgv !== null && !cmdProc.running) {
                var argv = root._pendingArgv
                root._pendingArgv = null
                cmdProc.command = argv
                cmdProc.running = true
            }
        }
        stdout: StdioCollector {
            onStreamFinished: cmdProc._dispatch()
        }
        stderr: StdioCollector {
            onStreamFinished: cmdProc._dispatch()
        }
    }

    // --- connect process (captures stdout+stderr+exit for error UI) ------
    function _runConn(argv) {
        if (connProc.running) {
            root._pendingConn = argv
            return
        }
        connProc.command = argv
        connProc.running = true
    }

    property var connProc: Process {
        function _dispatch() {
            if (root._pendingConn !== null && !connProc.running) {
                var argv = root._pendingConn
                root._pendingConn = null
                connProc.command = argv
                connProc.running = true
            }
        }
        function _evaluate(code) {
            var out = root._connOut
            root._connOut = ""
            if (code === 0) {
                root.wifiLastError = ""
            } else {
                var m = /Error:\s*(.+)/.exec(out)
                root.wifiLastError = (code === 124 || code === 143)
                    ? "Connection timed out"
                    : (m !== null ? m[1].trim() : "Connection failed")
            }
            root.wifiConnecting = false
            retryTimer.kick(4)
            connProc._dispatch()
        }
        onExited: function(code, status) {
            var c = code          // let StdioCollectors flush before scoring
            Qt.callLater(function() { connProc._evaluate(c) })
        }
        stdout: StdioCollector { onStreamFinished: root._connOut += this.text }
        stderr: StdioCollector { onStreamFinished: root._connOut += this.text }
    }

    // ~8s rescan loop, only while the connectivity panel is open.
    property var scanTimer: Timer {
        interval: 8000
        repeat: true
        running: root.wifiPanelActive && root.wifiEnabled
        onTriggered: root.scan()
    }

    onWifiPanelActiveChanged: {
        if (wifiPanelActive && wifiEnabled)
            root.scan()
    }

    onWifiEnabledChanged: {
        if (wifiPanelActive && wifiEnabled)
            root.scan()
    }

    // A few delayed refreshes after each command so the UI settles
    // (radio toggles, rescans and associations are all async in NM).
    property var retryTimer: Timer {
        interval: 2000
        repeat: true
        property int ticks: 0
        function kick(n) { ticks = n; restart() }
        onTriggered: {
            root.refresh()
            if (--ticks <= 0)
                stop()
        }
    }

    property var _timerN: Timer {
        interval: 5000
        running: true
        repeat: true
        onTriggered: root.refresh()
    }

    Component.onCompleted: root.refresh()
}
