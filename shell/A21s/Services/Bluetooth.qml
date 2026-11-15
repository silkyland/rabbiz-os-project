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

    property var _pendingArgv: null

    function setPowered(on) {
        _run(["bluetoothctl", "power", on ? "on" : "off"])
    }

    function refresh() {
        if (!pollProc.running)
            pollProc.running = true
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
