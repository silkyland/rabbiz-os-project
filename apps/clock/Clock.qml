import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets
import RabbizKit
import "ClockLogic.js" as ClockLogic

// Clock — in-house replacement for kclock (docs/BASE-APPS.md §3).
// One window, three sections. The window owns the three things that must not be duplicated:
//   * one 1 s tick for the header clock and the alarm check,
//   * one notification path (notify-send through Quickshell.Io.Process, FIFO so two alerts in
//     the same second both appear),
//   * one in-app banner, so Alarm and Timer announce themselves the same way.
// Alarm persistence is ~/.local/share/rabbiz-clock/alarms.json, written atomically (tmp + mv).
ApplicationWindow {
    id: win
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    title: "Clock"
    color: Theme.bg
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontBody

    Binding { target: Theme; property: "light"; value: ShellPrefs.lightTheme }

    property var now: new Date()
    property string section: "alarm"        // alarm | timer | stopwatch
    property var alarms: []
    property var notifyQueue: []
    property bool saveAgain: false
    property string bannerTitle: ""
    property string bannerBody: ""
    property string bannerIcon: "notifications_active"

    readonly property string clockText: ClockLogic.formatClock(win.now, ShellPrefs.use24h)
    readonly property string dateText: ClockLogic.formatDate(win.now)
    readonly property string alarmHint: ClockLogic.nextAlarmLine(win.alarms, win.now)
    readonly property int clockFontSize: Math.round(Theme.fontHeadline * 1.75)
    readonly property string alarmsPath: (Quickshell.env("HOME") || "/home/gamer")
        + "/.local/share/rabbiz-clock/alarms.json"

    // ---- notifications -----------------------------------------------------

    function notify(title, body) {
        if (notifyProc.running) {
            win.notifyQueue.push([title, body])
            return
        }
        notifyProc.command = ["notify-send", "-a", "Clock", title, body]
        notifyProc.running = true
    }

    // ---- in-app banner -----------------------------------------------------

    function raiseAlert(title, body, icon) {
        win.bannerTitle = title
        win.bannerBody = body
        win.bannerIcon = icon && icon.length > 0 ? icon : "notifications_active"
        bannerTimer.restart()
    }

    function dismissBanner() {
        bannerTimer.stop()
        win.bannerTitle = ""
        win.bannerBody = ""
    }

    // ---- alarms ------------------------------------------------------------

    function checkAlarms() {
        for (var i = 0; i < win.alarms.length; ++i) {
            var alarm = win.alarms[i]
            if (!ClockLogic.alarmDue(alarm, win.now))
                continue
            var timeText = ClockLogic.alarmTimeText(alarm)
            win.alarms = ClockLogic.markFired(win.alarms, alarm.id, win.now)
            win.saveAlarms()
            win.notify("Alarm", timeText)
            win.raiseAlert("Alarm " + timeText, "Ringing now. Tap to dismiss.", "alarm")
        }
    }

    function saveAlarms() {
        if (alarmsSaveProc.running) {          // a later write must not be dropped
            win.saveAgain = true
            return
        }
        alarmsSaveProc.command = ["sh", "-c",
            "mkdir -p \"$(dirname \"$1\")\" && printf '%s' \"$2\" > \"$1.tmp\" && mv \"$1.tmp\" \"$1\"",
            "_", win.alarmsPath, JSON.stringify(win.alarms)]
        alarmsSaveProc.running = true
    }

    Timer {
        id: tick
        interval: 1000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: {
            win.now = new Date()
            win.checkAlarms()
        }
    }

    Timer {
        id: bannerTimer
        interval: 8000
        onTriggered: win.dismissBanner()
    }

    Process {
        id: notifyProc
        onExited: {
            if (win.notifyQueue.length === 0)
                return
            var next = win.notifyQueue.shift()
            notifyProc.command = ["notify-send", "-a", "Clock", next[0], next[1]]
            notifyProc.running = true
        }
    }

    Process {
        id: alarmsSaveProc
        onExited: {
            if (win.saveAgain) {
                win.saveAgain = false
                win.saveAlarms()
            }
        }
    }

    // First run: make sure the file exists so FileView has something to load.
    Process {
        id: alarmsInitProc
        command: ["sh", "-c",
            "F=\"$1\"; mkdir -p \"$(dirname \"$F\")\" 2>/dev/null; " +
            "if [ ! -f \"$F\" ]; then printf '[]' > \"$F\"; fi",
            "_", win.alarmsPath]
        onExited: alarmsFile.reload()
    }

    FileView {
        id: alarmsFile
        path: win.alarmsPath
        onLoaded: win.alarms = ClockLogic.parseAlarms(text())
    }

    Component.onCompleted: alarmsInitProc.running = true

    // ---- page --------------------------------------------------------------

    ColumnLayout {
        anchors {
            fill: parent
            margins: Theme.spaceM
            bottomMargin: Theme.spaceL + Theme.spaceS
        }
        spacing: Theme.spaceM

        // The header is the clock itself: big time, date, and the next alarm when there is
        // one. No separate AppHeader — the time is the title of this app.
        Card {
            Layout.fillWidth: true
            padding: Theme.spaceL

            ColumnLayout {
                width: parent.width
                spacing: Theme.spaceXS

                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: win.clockText
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: win.clockFontSize
                    font.weight: Font.Light
                }
                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: win.dateText
                    color: Theme.textDim
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody - 4
                }
                Text {
                    visible: win.alarmHint.length > 0
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: win.alarmHint
                    color: Theme.textFaint
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontCaption - 2
                    elide: Text.ElideRight
                }
            }
        }

        // ---- segmented control: no in-app back button (system back minimizes) ----
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 56
            radius: Theme.radiusPill
            color: Theme.surfaceHigh
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.06)

            RowLayout {
                anchors { fill: parent; margins: 4 }
                spacing: 4

                Repeater {
                    model: [
                        { key: "alarm", label: "Alarm" },
                        { key: "timer", label: "Timer" },
                        { key: "stopwatch", label: "Stopwatch" }
                    ]

                    delegate: Rectangle {
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        radius: Theme.radiusPill
                        color: win.section === modelData.key
                            ? ShellPrefs.accentColor : "transparent"

                        Text {
                            anchors.centerIn: parent
                            text: modelData.label
                            color: win.section === modelData.key
                                ? ShellPrefs.onAccentColor : Theme.textDim
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontBody - 6
                            font.weight: Font.Medium
                        }
                        TapHandler { onTapped: win.section = modelData.key }
                    }
                }
            }
        }

        // ---- sections ----------------------------------------------------------
        AlarmSection {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: win.section === "alarm"
            alarms: win.alarms
            onAlarmsEdited: function (list) {
                win.alarms = list
                win.saveAlarms()
            }
        }

        TimerSection {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: win.section === "timer"
            onAlert: function (title, body) {
                win.notify(title, body)
                win.raiseAlert(title, body, "timer")
            }
        }

        StopwatchSection {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: win.section === "stopwatch"
        }
    }

    // ---- banner overlay: floats over the page, so firing an alarm never moves the layout ---
    Rectangle {
        id: banner
        // Top overlay, Android heads-up style: it covers the clock for a few seconds rather
        // than the transport buttons the user needs to reach next.
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            topMargin: Theme.spaceM
            leftMargin: Theme.spaceM
            rightMargin: Theme.spaceM
        }
        height: bannerRow.implicitHeight + Theme.spaceM * 2
        radius: Theme.radiusL
        color: Theme.surfaceHigh
        border.width: 1
        border.color: Qt.alpha(ShellPrefs.accentColor, 0.55)
        opacity: win.bannerTitle.length > 0 ? 1 : 0
        visible: opacity > 0.01
        Behavior on opacity { NumberAnimation { duration: Theme.animMed } }

        RowLayout {
            id: bannerRow
            anchors { fill: parent; margins: Theme.spaceM }
            spacing: Theme.spaceS

            Rectangle {
                width: 44
                height: 44
                radius: width / 2
                color: Qt.alpha(ShellPrefs.accentColor, 0.18)

                MaterialIcon {
                    anchors.centerIn: parent
                    name: win.bannerIcon
                    size: 24
                    color: ShellPrefs.accentColor
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2

                Text {
                    Layout.fillWidth: true
                    text: win.bannerTitle
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody - 4
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }
                Text {
                    Layout.fillWidth: true
                    text: win.bannerBody
                    color: Theme.textDim
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontCaption - 2
                    elide: Text.ElideRight
                }
            }

            TouchButton {
                icon: "close"
                filled: true
                tint: ShellPrefs.accentColor
                onClicked: win.dismissBanner()
            }
        }

        TapHandler { onTapped: win.dismissBanner() }
    }
}
