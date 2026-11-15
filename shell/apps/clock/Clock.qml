import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import A21s.Theme
import A21s.Services
import A21s.Components as Components

// Standalone Clock app (Clock / Stopwatch / Timer) with horizontal swipe tabs
// and Android edge-back gesture.
ApplicationWindow {
    id: win
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    title: "Clock"
    color: Theme.bg
    font.family: Theme.fontFamily

    SystemClock {
        id: sysClock
        precision: SystemClock.Seconds
    }

    // Stopwatch state
    property int swElapsedMs: 0
    property bool swRunning: false
    property var swLaps: []

    Timer {
        interval: 50
        running: win.swRunning
        repeat: true
        onTriggered: win.swElapsedMs += 50
    }

    function formatMs(ms) {
        var totalSec = Math.floor(ms / 1000)
        var mins = Math.floor(totalSec / 60)
        var secs = totalSec % 60
        var cs = Math.floor((ms % 1000) / 10)
        return (mins < 10 ? "0" : "") + mins + ":"
             + (secs < 10 ? "0" : "") + secs + "."
             + (cs < 10 ? "0" : "") + cs
    }

    // Timer state
    property int timerRemainingSec: 300
    property bool timerRunning: false

    Timer {
        interval: 1000
        running: win.timerRunning && win.timerRemainingSec > 0
        repeat: true
        onTriggered: {
            if (win.timerRemainingSec > 0)
                win.timerRemainingSec -= 1
            if (win.timerRemainingSec === 0)
                win.timerRunning = false
        }
    }

    function formatSec(sec) {
        var m = Math.floor(sec / 60)
        var s = sec % 60
        return (m < 10 ? "0" : "") + m + ":" + (s < 10 ? "0" : "") + s
    }

    ColumnLayout {
        anchors {
            fill: parent
            margins: Theme.spaceM
            bottomMargin: Theme.navZoneHeight + Theme.spaceM
        }
        spacing: Theme.spaceM

        // Segmented tabs (tap or swipe SwipeView below)
        Rectangle {
            Layout.fillWidth: true
            height: Theme.iconM + Theme.spaceS
            radius: Theme.radiusPill
            color: Theme.surface
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.08)

            RowLayout {
                anchors.fill: parent
                anchors.margins: 4
                spacing: 4

                Repeater {
                    model: [
                        { label: "Clock", icon: "schedule" },
                        { label: "Stopwatch", icon: "timer" },
                        { label: "Timer", icon: "hourglass_bottom" }
                    ]
                    delegate: Rectangle {
                        required property int index
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        radius: Theme.radiusPill
                        color: pager.currentIndex === index ? ShellPrefs.accentColor : "transparent"

                        Row {
                            anchors.centerIn: parent
                            spacing: 6
                            Components.MaterialIcon {
                                anchors.verticalCenter: parent.verticalCenter
                                name: modelData.icon
                                size: Theme.fontBody
                                color: pager.currentIndex === index ? Theme.primaryContent : Theme.textDim
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: modelData.label
                                color: pager.currentIndex === index ? Theme.primaryContent : Theme.textDim
                                font.pixelSize: Theme.fontCaption
                                font.weight: Font.Medium
                            }
                        }

                        TapHandler {
                            onTapped: pager.currentIndex = index
                        }
                    }
                }
            }
        }

        // Swipeable pages
        SwipeView {
            id: pager
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true

            // Page 0: World / Local Clock
            Item {
                ColumnLayout {
                    anchors.centerIn: parent
                    spacing: Theme.spaceM

                    Text {
                        Layout.alignment: Qt.AlignHCenter
                        text: Qt.formatDateTime(sysClock.date, ShellPrefs.use24h ? "HH:mm:ss" : "h:mm:ss ap")
                        color: Theme.text
                        font.pixelSize: 96
                        font.weight: Font.Light
                    }
                    Text {
                        Layout.alignment: Qt.AlignHCenter
                        text: Qt.formatDateTime(sysClock.date, "dddd, MMMM d, yyyy")
                        color: Theme.textDim
                        font.pixelSize: Theme.fontTitle
                    }
                }
            }

            // Page 1: Stopwatch
            Item {
                ColumnLayout {
                    anchors.fill: parent
                    spacing: Theme.spaceL

                    Item { Layout.preferredHeight: Theme.spaceL }

                    Text {
                        Layout.alignment: Qt.AlignHCenter
                        text: win.formatMs(win.swElapsedMs)
                        color: Theme.text
                        font.pixelSize: 88
                        font.weight: Font.Light
                    }

                    RowLayout {
                        Layout.alignment: Qt.AlignHCenter
                        spacing: Theme.spaceM

                        Rectangle {
                            width: 180; height: 72; radius: Theme.radiusPill
                            color: win.swRunning ? Theme.danger : ShellPrefs.accentColor
                            Text {
                                anchors.centerIn: parent
                                text: win.swRunning ? "Pause" : "Start"
                                color: Theme.primaryContent
                                font.pixelSize: Theme.fontBody
                                font.weight: Font.DemiBold
                            }
                            TapHandler { onTapped: win.swRunning = !win.swRunning }
                        }

                        Rectangle {
                            width: 160; height: 72; radius: Theme.radiusPill
                            color: Theme.surfaceHigh
                            border.width: 1; border.color: Qt.alpha(Theme.text, 0.12)
                            Text {
                                anchors.centerIn: parent
                                text: win.swRunning ? "Lap" : "Reset"
                                color: Theme.text
                                font.pixelSize: Theme.fontBody
                            }
                            TapHandler {
                                onTapped: {
                                    if (win.swRunning) {
                                        var next = win.swLaps.slice()
                                        next.unshift({ n: next.length + 1, t: win.formatMs(win.swElapsedMs) })
                                        win.swLaps = next
                                    } else {
                                        win.swElapsedMs = 0
                                        win.swLaps = []
                                    }
                                }
                            }
                        }
                    }

                    ListView {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        spacing: Theme.spaceXS
                        model: win.swLaps
                        delegate: Rectangle {
                            required property var modelData
                            width: ListView.view.width
                            height: 64
                            radius: Theme.radiusM
                            color: Theme.card
                            RowLayout {
                                anchors { fill: parent; leftMargin: Theme.spaceM; rightMargin: Theme.spaceM }
                                Text { text: "Lap " + modelData.n; color: Theme.textDim; font.pixelSize: Theme.fontBody }
                                Item { Layout.fillWidth: true }
                                Text { text: modelData.t; color: Theme.text; font.pixelSize: Theme.fontBody; font.weight: Font.Medium }
                            }
                        }
                    }
                }
            }

            // Page 2: Timer
            Item {
                ColumnLayout {
                    anchors.centerIn: parent
                    spacing: Theme.spaceL

                    Text {
                        Layout.alignment: Qt.AlignHCenter
                        text: win.formatSec(win.timerRemainingSec)
                        color: win.timerRemainingSec === 0 ? Theme.warn : Theme.text
                        font.pixelSize: 110
                        font.weight: Font.Light
                    }

                    RowLayout {
                        Layout.alignment: Qt.AlignHCenter
                        spacing: Theme.spaceS
                        Repeater {
                            model: [
                                { label: "1m", sec: 60 },
                                { label: "5m", sec: 300 },
                                { label: "10m", sec: 600 },
                                { label: "25m", sec: 1500 }
                            ]
                            delegate: Rectangle {
                                required property var modelData
                                width: 110; height: 54; radius: Theme.radiusPill
                                color: Theme.card
                                border.width: 1; border.color: Qt.alpha(Theme.text, 0.10)
                                Text {
                                    anchors.centerIn: parent
                                    text: modelData.label
                                    color: Theme.text
                                    font.pixelSize: Theme.fontCaption
                                }
                                TapHandler {
                                    onTapped: {
                                        win.timerRunning = false
                                        win.timerRemainingSec = modelData.sec
                                    }
                                }
                            }
                        }
                    }

                    Rectangle {
                        Layout.alignment: Qt.AlignHCenter
                        width: 220; height: 76; radius: Theme.radiusPill
                        color: win.timerRunning ? Theme.danger : ShellPrefs.accentColor
                        Text {
                            anchors.centerIn: parent
                            text: win.timerRunning ? "Stop" : "Start Timer"
                            color: Theme.primaryContent
                            font.pixelSize: Theme.fontBody
                            font.weight: Font.DemiBold
                        }
                        TapHandler {
                            onTapped: {
                                if (win.timerRemainingSec > 0)
                                    win.timerRunning = !win.timerRunning
                            }
                        }
                    }
                }
            }
        }
    }

    Components.AppGestureLayer {
        anchors.fill: parent
        onBackTriggered: {
            if (pager.currentIndex > 0)
                pager.currentIndex = 0
            else
                win.close()
        }
    }
}
