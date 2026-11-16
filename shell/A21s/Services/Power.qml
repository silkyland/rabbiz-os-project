pragma Singleton
import QtQuick
import Quickshell.Io

// Poll fresh gauge/UDC state: UPower's battery polling alone is too slow for
// cable feedback, and net battery current can be negative while USB is attached.
QtObject {
    id: root
    property var _transition: ChargingEvents.create()
    property int _notificationId: 0

    function start() {
        root.refresh()
        poll.start()
    }

    function refresh() {
        if (!sampleProc.running) sampleProc.running = true
    }

    function acceptSample(raw) {
        var lines = raw.trim().split("\n")
        if (lines.length < 2) return
        var percentage = Number(lines[1])
        if (!isFinite(percentage) || percentage < 0 || percentage > 100) return
        var usbState = lines.length > 2 ? lines[2].trim() : ""
        var connected = ChargingEvents.connection(lines[0].trim(), usbState)
        root._transition = ChargingEvents.sample(root._transition, connected, Date.now())
        if (root._transition.event === "started") {
            var title = lines[0].trim() === "Full" ? qsTr("Fully charged")
                : lines[0].trim() === "Charging" ? qsTr("Charging") : qsTr("Power connected")
            notifyProc.command = ["notify-send", "--print-id", "--replace-id", String(root._notificationId),
                "--app-name", "RabbizOS Power", "--icon", "battery-good-charging-symbolic",
                "--expire-time", "0", title + " · " + Math.round(percentage) + "%",
                qsTr("Connected to power")]
            notifyProc.running = true
        } else if (root._transition.event === "stopped" && root._notificationId > 0) {
            closeProc.command = ["busctl", "--user", "call", "org.freedesktop.Notifications",
                "/org/freedesktop/Notifications", "org.freedesktop.Notifications",
                "CloseNotification", "u", String(root._notificationId)]
            closeProc.running = true
            root._notificationId = 0
        }
    }

    property var pollTimer: Timer {
        id: poll
        interval: 2000
        repeat: true
        onTriggered: root.refresh()
    }

    property var sampleProc: Process {
        command: ["sh", "-c",
            "for b in /sys/class/power_supply/*; do "
          + "[ -r \"$b/type\" ] && [ \"$(cat \"$b/type\")\" = Battery ] || continue; "
          + "cat \"$b/status\" \"$b/capacity\"; break; done; "
          + "for u in /sys/class/udc/*; do [ -r \"$u/state\" ] && cat \"$u/state\"; break; done"]
        stdout: StdioCollector { onStreamFinished: root.acceptSample(this.text) }
    }

    property var notifyProc: Process {
        stdout: StdioCollector {
            onStreamFinished: {
                var value = parseInt(this.text.trim())
                if (value > 0) {
                    root._notificationId = value
                    console.info("Power notification posted:", value)
                }
            }
        }
        onExited: function(code) {
            if (code !== 0) console.warn("Power notification failed:", code)
        }
    }
    property var closeProc: Process {}
}
