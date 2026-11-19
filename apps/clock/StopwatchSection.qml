import QtQuick
import QtQuick.Layouts
import Rabbiz.Theme
import RabbizKit
import "ClockLogic.js" as ClockLogic

// Stopwatch tab: centiseconds, pause/resume that keeps the banked time, and laps newest
// first. The 50 ms tick only runs while the clock is running.
Item {
    id: root
    property bool running: false
    property double accumMs: 0
    property double startedAtMs: 0
    property double nowMs: Date.now()
    property var laps: []

    readonly property double elapsed: ClockLogic.elapsedMs(
        root.accumMs, root.running ? root.startedAtMs : null, root.nowMs)
    readonly property string elapsedText: ClockLogic.formatStopwatch(root.elapsed)
    readonly property string statusText: root.running ? "Running"
        : root.elapsed > 0 ? "Paused" : "Ready"

    function start() {
        if (root.running) return
        root.startedAtMs = Date.now()
        root.nowMs = root.startedAtMs
        root.running = true
        tick.restart()
    }

    function pause() {
        if (!root.running) return
        root.accumMs = ClockLogic.elapsedMs(root.accumMs, root.startedAtMs, Date.now())
        root.running = false
        tick.stop()
        root.nowMs = Date.now()
    }

    function toggle() {
        if (root.running) root.pause()
        else root.start()
    }

    function reset() {
        root.running = false
        tick.stop()
        root.accumMs = 0
        root.startedAtMs = 0
        root.laps = []
        root.nowMs = Date.now()
    }

    function lap() {
        if (!root.running) return
        var total = ClockLogic.elapsedMs(root.accumMs, root.startedAtMs, Date.now())
        var previous = root.laps.length > 0 ? root.laps[0].total : 0
        var entry = { n: root.laps.length + 1, split: total - previous, total: total }
        root.laps = [entry].concat(root.laps)
        root.nowMs = Date.now()
    }

    Timer {
        id: tick
        interval: 50
        repeat: true
        onTriggered: root.nowMs = Date.now()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spaceM

        Card {
            Layout.fillWidth: true
            padding: Theme.spaceL

            ColumnLayout {
                width: parent.width
                spacing: Theme.spaceXS

                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: root.elapsedText
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Math.round(Theme.fontHeadline * 1.25)
                    font.weight: Font.Light
                }
                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: root.statusText
                    color: Theme.textDim
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontCaption - 2
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceS

            AccentButton {
                Layout.fillWidth: true
                text: root.running ? "Pause" : "Start"
                materialIcon: root.running ? "pause" : "play_arrow"
                onClicked: root.toggle()
            }
            SecondaryButton {
                Layout.fillWidth: true
                text: "Lap"
                materialIcon: "flag"
                enabled2: root.running
                onClicked: root.lap()
            }
            SecondaryButton {
                Layout.fillWidth: true
                text: "Reset"
                materialIcon: "refresh"
                enabled2: root.elapsed > 0
                onClicked: root.reset()
            }
        }

        SectionLabel {
            Layout.fillWidth: true
            text: "LAPS"
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ListView {
                anchors.fill: parent
                clip: true
                spacing: 2
                cacheBuffer: 800
                boundsBehavior: Flickable.StopAtBounds
                model: root.laps

                delegate: RowItem {
                    required property var modelData
                    width: ListView.view.width
                    icon: ""
                    showChevron: false
                    title: "Lap " + modelData.n
                    caption: "Total " + ClockLogic.formatStopwatch(modelData.total)
                    value: ClockLogic.formatStopwatch(modelData.split)
                }
            }

            EmptyState {
                visible: root.laps.length === 0
                anchors.centerIn: parent
                width: parent.width * 0.82
                icon: "flag"
                title: "No laps yet"
                hint: "Tap Lap while the stopwatch runs to record a split."
            }
        }
    }
}
