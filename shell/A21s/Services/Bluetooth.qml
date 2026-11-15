pragma Singleton
import QtQuick
import Quickshell.Io

// Bluetooth state + control via `bluetoothctl` (all calls wrapped in
// `timeout` so a missing/unresponsive bluetoothd can't stall the UI).
// `devices` lists every device known to the adapter (paired + previously
// discovered); `paired` is reported true for all of them — bluetoothctl
// doesn't cheaply distinguish bonded vs. merely-seen devices here.
QtObject {
    id: root

    property bool powered: false
    property bool connected: false
    property string connectedName: ""
    property var devices: []               // [{mac, name, connected, paired}]

    // --- Connectivity panel extensions (additive) ------------------------
    property bool btPanelActive: false     // gates discovery scan cycles
    readonly property bool btScanning: scanProc.running
    property var btDevices: []             // [{mac,name,paired,connected,icon,rssi,battery}]
    property bool btBusy: false            // pair/connect/disconnect/remove in flight
    property string btBusyMac: ""
    property string btLastError: ""        // "" = no error

    property var _pendingArgv: null
    property var _pendingOp: null
    property string _pendingOpKind: ""
    property string _opKind: ""
    property string _opOut: ""
    property var _btMap: ({})              // mac -> btDevices entry (persists across cycles)

    function setPowered(on) {
        _run(["bluetoothctl", "power", on ? "on" : "off"])
    }

    function refresh() {
        if (!pollProc.running)
            pollProc.running = true
    }

    // 'arg' -> 'arg' with embedded quotes escaped for sh -c use.
    function _shq(s) {
        return "'" + String(s).replace(/'/g, "'\\''") + "'"
    }

    function btScan() {
        if (root.powered && !scanProc.running)
            scanProc.running = true
    }

    function btList() {
        if (root.powered && !listProc.running)
            listProc.running = true
    }

    function btConnect(mac) {
        root.btBusy = true
        root.btBusyMac = mac
        root.btLastError = ""
        _runOp("connect", ["sh", "-c",
            "timeout 15 bluetoothctl pair " + _shq(mac) + " 2>&1; "
          + "timeout 8 bluetoothctl trust " + _shq(mac) + " 2>&1; "
          + "echo '=X='; timeout 15 bluetoothctl connect " + _shq(mac) + " 2>&1"])
    }

    function btDisconnect(mac) {
        root.btBusy = true
        root.btBusyMac = mac
        root.btLastError = ""
        _runOp("disconnect", ["sh", "-c",
            "timeout 10 bluetoothctl disconnect " + _shq(mac) + " 2>&1"])
    }

    function btRemove(mac) {
        root.btBusy = true
        root.btBusyMac = mac
        root.btLastError = ""
        _runOp("remove", ["sh", "-c",
            "timeout 10 bluetoothctl remove " + _shq(mac) + " 2>&1"])
    }

    function _runOp(kind, argv) {
        if (opProc.running) {
            root._pendingOp = argv
            root._pendingOpKind = kind
            return
        }
        root._opKind = kind
        opProc.command = argv
        opProc.running = true
    }

    // Strip ANSI escapes + control chars bluetoothctl emits on a pty-less pipe.
    function _clean(s) {
        return s.replace(/\x1b\[[0-9;?]*[A-Za-z]/g, "")
                .replace(/[\x00-\x08\x0B-\x1F\x7F]/g, "")
    }

    function _btEntry(mac) {
        var e = root._btMap[mac]
        if (!e) {
            e = { "mac": mac, "name": "", "paired": false, "connected": false,
                  "icon": "", "rssi": -999, "battery": -1 }
            root._btMap[mac] = e
        }
        return e
    }

    // Drop placeholder names ("AA-BB-CC..." or raw MAC) so the UI can show
    // a friendlier fallback for unnamed devices.
    function _btName(raw, mac) {
        var n = (raw || "").trim()
        if (n.length === 0 || n === mac || n === mac.replace(/:/g, "-"))
            return ""
        return n
    }

    function _applyBtKv(e, key, val) {
        if (key === "Name" || key === "Alias") {
            var n = root._btName(val, e.mac)
            if (n.length > 0)
                e.name = n
        } else if (key === "Icon") {
            e.icon = val.trim()
        } else if (key === "Connected") {
            e.connected = val.indexOf("yes") === 0
        } else if (key === "Paired" || key === "Bonded" || key === "Trusted") {
            if (val.indexOf("yes") === 0)
                e.paired = true
        } else if (key === "RSSI") {                // "-77" or "0xffffffba (-70)"
            var rm = /\((-?\d+)\)/.exec(val)
            var r = rm !== null ? parseInt(rm[1]) : parseInt(val)
            if (!isNaN(r))
                e.rssi = r
        } else if (key === "Battery Percentage") {   // "0x52 (82)"
            var b = /\((\d+)\)/.exec(val)
            if (b !== null)
                e.battery = parseInt(b[1])
        }
    }

    // Parse a combined "=D=/=P=/=C=/=I=" sectioned dump (everything before
    // =D= is treated as raw `scan on` stream). Updates _btMap + btDevices.
    function _applyBtSections(text) {
        var map = root._btMap
        var seen = {}
        var secs = text.split("=D=")
        var scanBlock = secs[0]
        var r1 = secs.length > 1 ? secs[1].split("=P=") : ["", ""]
        var devLines = r1[0]
        var r2 = r1.length > 1 ? r1[1].split("=C=") : ["", ""]
        var pairLines = r2[0]
        var r3 = r2.length > 1 ? r2[1].split("=I=") : ["", ""]
        var connLines = r3[0]
        var infoBlock = r3.length > 1 ? r3[1] : ""

        // 1) discovery stream: [NEW]/[CHG]/[DEL] Device AA:BB.. lines
        var sl = scanBlock.split("\n")
        for (var i = 0; i < sl.length; ++i) {
            var line = root._clean(sl[i]).trim()
            var m = /\[(NEW|CHG|DEL)\]\s+Device\s+([0-9A-Fa-f:]{17})\s*(.*)$/.exec(line)
            if (m === null)
                continue
            var mac = m[2].toUpperCase(), rest = m[3]
            if (m[1] === "DEL") {
                if (map[mac] && !map[mac].paired && !map[mac].connected)
                    delete map[mac]
                continue
            }
            var e = root._btEntry(mac)
            seen[mac] = true
            if (m[1] === "NEW") {
                var nn = root._btName(rest, mac)
                if (nn.length > 0)
                    e.name = nn
            } else {
                var kv = /^([A-Za-z. ]+):\s*(.*)$/.exec(rest)
                if (kv !== null)
                    root._applyBtKv(e, kv[1].trim(), kv[2])
            }
        }

        // 2) `devices` list -> every known MAC (incl. silently discovered)
        var dl = devLines.split("\n")
        for (var j = 0; j < dl.length; ++j) {
            var d = root._parseDevice(root._clean(dl[j]).trim())
            if (d !== null) {
                var de = root._btEntry(d.mac)
                seen[d.mac] = true
                var dn = root._btName(d.name, d.mac)
                if (dn.length > 0 && de.name.length === 0)
                    de.name = dn
            }
        }

        // 3) paired / connected sets are authoritative -> reset first
        for (var k in map)
            map[k].connected = false
        var pl = pairLines.split("\n")
        for (var p = 0; p < pl.length; ++p) {
            var pd = root._parseDevice(root._clean(pl[p]).trim())
            if (pd !== null) {
                var pe = root._btEntry(pd.mac)
                pe.paired = true
                seen[pd.mac] = true
            }
        }
        var cl = connLines.split("\n")
        for (var c = 0; c < cl.length; ++c) {
            var cd = root._parseDevice(root._clean(cl[c]).trim())
            if (cd !== null) {
                var ce = root._btEntry(cd.mac)
                ce.connected = true
                ce.paired = true
                seen[cd.mac] = true
            }
        }

        // 4) per-device `info` dumps, separated by "@MAC" header lines
        var cur = null
        var il = infoBlock.split("\n")
        for (var q = 0; q < il.length; ++q) {
            var iline = root._clean(il[q])
            var am = /^@([0-9A-Fa-f:]{17})\s*$/.exec(iline.trim())
            if (am !== null) {
                cur = root._btEntry(am[1].toUpperCase())
                seen[am[1].toUpperCase()] = true
                continue
            }
            if (cur === null)
                continue
            var f = /^\s*([A-Za-z. ]+):\s*(.*)$/.exec(iline)
            if (f !== null)
                root._applyBtKv(cur, f[1].trim(), f[2])
        }

        // 5) drop entries the adapter has fully forgotten; paired devices
        // only disappear via `remove`, which also drops them from `devices`
        var listsRan = devLines.trim().length > 0 || pairLines.trim().length > 0
        for (var dm in map) {
            if (seen[dm] === true || map[dm].connected)
                continue
            if (map[dm].paired && !listsRan)
                continue
            delete map[dm]
        }

        // 6) materialize sorted list: connected, paired, then by rssi/name
        var list = []
        for (var km in map) {
            var ent = map[km]
            if (ent.name.length === 0)
                ent.name = ent.mac
            list.push(ent)
        }
        list.sort(function(a, b) {
            if (a.connected !== b.connected)
                return a.connected ? -1 : 1
            if (a.paired !== b.paired)
                return a.paired ? -1 : 1
            if (a.rssi !== b.rssi)
                return b.rssi - a.rssi
            return a.name < b.name ? -1 : (a.name > b.name ? 1 : 0)
        })
        root.btDevices = list
    }

    function _run(argv) {
        retryTimer.kick(retryTimer.ticks > 0 ? retryTimer.ticks : 2)
        if (cmdProc.running) {
            root._pendingArgv = argv
            return
        }
        cmdProc.command = argv
        cmdProc.running = true
    }

    // "Device AA:BB:CC:DD:EE:FF Some Name" -> {mac, name}
    function _parseDevice(line) {
        var m = /^Device\s+([0-9A-Fa-f:]{17})\s*(.*)$/.exec(line)
        if (m === null)
            return null
        return { "mac": m[1], "name": m[2] }
    }

    property var pollProc: Process {
        command: ["sh", "-c",
            "timeout 5 bluetoothctl show 2>/dev/null; "
          + "echo '=C='; "
          + "timeout 5 bluetoothctl devices Connected 2>/dev/null; "
          + "echo '=D='; "
          + "timeout 5 bluetoothctl devices 2>/dev/null"]

        stdout: StdioCollector {
            onStreamFinished: {
                var parts = this.text.split("=C=")
                var show = parts[0]
                var rest = parts.length > 1 ? parts[1].split("=D=") : ["", ""]
                var connLines = rest[0]
                var allLines = rest.length > 1 ? rest[1] : ""

                // controller state
                if (show.indexOf("No default controller") !== -1) {
                    root.powered = false
                    root.connected = false
                    root.connectedName = ""
                    root.devices = []
                    return
                }
                root.powered = show.indexOf("Powered: yes") !== -1

                // connected devices -> set of MACs
                var connSet = {}, connName = ""
                var cl = connLines.split("\n")
                for (var i = 0; i < cl.length; ++i) {
                    var cd = root._parseDevice(cl[i].trim())
                    if (cd !== null) {
                        connSet[cd.mac] = true
                        if (connName === "")
                            connName = cd.name.length > 0 ? cd.name : cd.mac
                    }
                }
                root.connected = Object.keys(connSet).length > 0
                root.connectedName = connName

                // all known devices
                var list = []
                var al = allLines.split("\n")
                for (var j = 0; j < al.length; ++j) {
                    var d = root._parseDevice(al[j].trim())
                    if (d !== null) {
                        list.push({ "mac": d.mac,
                                    "name": d.name.length > 0 ? d.name : d.mac,
                                    "connected": connSet[d.mac] === true,
                                    "paired": true })
                    }
                }
                list.sort(function(a, b) {
                    if (a.connected !== b.connected)
                        return a.connected ? -1 : 1
                    return a.name < b.name ? -1 : 1
                })
                root.devices = list
            }
        }
    }

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

    // --- Discovery scan cycle (~8s scan + authoritative list/info dump) --
    // bluetoothctl must stay alive on stdin while discovery runs; piping
    // `scan on` -> sleep -> `scan off` into one process does exactly that.
    // Its output carries ANSI escapes, cleaned in _applyBtSections/_clean.
    property var scanProc: Process {
        command: ["sh", "-c",
            "(printf 'pairable on\\n'; printf 'scan on\\n'; sleep 8; printf 'scan off\\n') "
          + "| timeout 20 bluetoothctl 2>/dev/null "
          + "| sed 's/\\x1b\\[[0-9;?]*[A-Za-z]//g' "
          + "| grep -aE '\\[(NEW|CHG|DEL)\\] Device '; "
          + "echo '=D='; timeout 5 bluetoothctl devices 2>/dev/null; "
          + "echo '=P='; timeout 5 bluetoothctl devices Paired 2>/dev/null; "
          + "echo '=C='; timeout 5 bluetoothctl devices Connected 2>/dev/null; "
          + "echo '=I='; for m in $(timeout 5 bluetoothctl devices Paired 2>/dev/null "
          + "| awk '{print $2}' | head -10); do echo \"@$m\"; "
          + "timeout 3 bluetoothctl info \"$m\" 2>/dev/null "
          + "| grep -aE '^[[:space:]]*(Name|Alias|Icon|Connected|RSSI|Battery Percentage):'; done"]

        onExited: root.refresh()          // tile view picks up new connects
        stdout: StdioCollector {
            onStreamFinished: root._applyBtSections(this.text)
        }
    }

    // Fast list/info refresh without scanning (panel open, after bt ops).
    property var listProc: Process {
        command: ["sh", "-c",
            "echo '=D='; timeout 5 bluetoothctl devices 2>/dev/null; "
          + "echo '=P='; timeout 5 bluetoothctl devices Paired 2>/dev/null; "
          + "echo '=C='; timeout 5 bluetoothctl devices Connected 2>/dev/null; "
          + "echo '=I='; for m in $(timeout 5 bluetoothctl devices Paired 2>/dev/null "
          + "| awk '{print $2}' | head -10); do echo \"@$m\"; "
          + "timeout 3 bluetoothctl info \"$m\" 2>/dev/null "
          + "| grep -aE '^[[:space:]]*(Name|Alias|Icon|Connected|RSSI|Battery Percentage):'; done"]

        stdout: StdioCollector {
            onStreamFinished: root._applyBtSections(this.text)
        }
    }

    // --- pair/trust/connect/disconnect/remove op (serialized, error-aware)
    property var opProc: Process {
        function _dispatch() {
            if (root._pendingOp !== null && !opProc.running) {
                var argv = root._pendingOp
                root._pendingOp = null
                root._opKind = root._pendingOpKind
                opProc.command = argv
                opProc.running = true
            }
        }
        function _evaluate() {
            var out = root._clean(root._opOut).replace(/\[bluetoothctl\]>?\s*/g, "")
            root._opOut = ""
            // For connect ops only the section after '=X=' is decisive
            // (pair may legitimately fail with AlreadyExists).
            var x = out.lastIndexOf("=X=")
            var tail = x >= 0 ? out.substring(x + 3) : out
            var kind = root._opKind
            var ok = kind === "connect"
                ? tail.indexOf("Connection successful") !== -1
                : kind === "disconnect"
                ? (tail.indexOf("Successful disconnected") !== -1
                   || tail.indexOf("Connected: no") !== -1)
                : kind === "remove"
                ? tail.indexOf("has been removed") !== -1
                : true
            if (ok) {
                root.btLastError = ""
            } else {
                var err = ""
                var tl = tail.split("\n")
                for (var i = 0; i < tl.length; ++i) {
                    var l = tl[i].trim()
                    var f = /(Failed to \w+:?\s*.+|org\.bluez\.Error\.\w+|Device .+ not (found|available)|Unable to .+)/.exec(l)
                    if (f !== null) {
                        err = l.replace(/^[^A-Za-z]*/, "").trim()
                        break
                    }
                }
                root.btLastError = err.length > 0 ? err : "Operation failed"
            }
            root.btBusy = root._pendingOp !== null
            if (!root.btBusy)
                root.btBusyMac = ""
            root.refresh()
            root.btList()
            opProc._dispatch()
        }
        onExited: Qt.callLater(opProc._evaluate)
        stdout: StdioCollector { onStreamFinished: root._opOut += this.text }
        stderr: StdioCollector { onStreamFinished: root._opOut += this.text }
    }

    // Scan cycles run back-to-back with a short gap while the panel is open.
    property var scanTimer: Timer {
        interval: 3500
        repeat: true
        running: root.btPanelActive && root.powered
        onTriggered: root.btScan()
    }

    onBtPanelActiveChanged: {
        if (btPanelActive && powered) {
            root.btList()                 // instant content, then discovery
            root.btScan()
        }
    }

    onPoweredChanged: {
        if (btPanelActive && powered) {
            root.btList()
            root.btScan()
        }
    }

    // Delayed re-polls after `power on/off` — the adapter needs a moment.
    property var retryTimer: Timer {
        interval: 1500
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
        interval: 8000
        running: true
        repeat: true
        onTriggered: root.refresh()
    }

    Component.onCompleted: root.refresh()
}
