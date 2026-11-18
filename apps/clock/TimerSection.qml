import QtQuick
import QtQuick.Layouts
import Rabbiz.Theme
import Rabbiz.Services
import RabbizKit
import "ClockLogic.js" as ClockLogic

// Timer tab. The countdown is driven by an absolute deadline and a 200 ms tick, so it cannot
// drift and keeps running while another section is open. Zero fires one notification and one
// in-app banner through the window (`alert`), never a sound loop.
Item {
    id: root
    property double durationMs: 5 * 60000
    property double remainingMs: 5 * 60000
    property bool running: false
    property double deadlineMs: 0

    signal alert(string title, string body)

    readonly property int minutes: Math.floor(root.durationMs / 60000)
    readonly property int seconds: Math.round((root.durationMs % 60000) / 1000)
    readonly property real progress: root.durationMs > 0
        ? Math.max(0, Math.min(1, root.remainingMs / root.durationMs)) : 0
    readonly property string remainingText: ClockLogic.formatDuration(root.remainingMs)
    readonly property string statusText: root.running ? "Running"
        : root.durationMs <= 0 ? "Choose a length"
        : root.remainingMs <= 0 ? "Finished"
        : root.remainingMs < root.durationMs ? "Paused"
        : "Ready"
    // While it runs, say when it ends — more useful than a bare "Running".
    readonly property string statusLine: root.running
        ? (root.statusText + " · ends " + ClockLogic.formatClock(new Date(root.deadlineMs), ShellPrefs.use24h))
        : root.statusText

    function setDuration(ms) {
        var next = Math.max(0, Math.round(ms))
        root.running = false
        tick.stop()
        root.durationMs = next
        root.remainingMs = next
        root.deadlineMs = 0
    }

    function start() {
        if (root.durationMs <= 0) return
        // Finished already? Starting again re-runs the chosen length instead of a dead end.
        if (root.remainingMs <= 0) root.remainingMs = root.durationMs
        root.deadlineMs = Date.now() + root.remainingMs
        root.running = true
        tick.restart()
    }

    function pause() {
        if (!root.running) return
        root.remainingMs = ClockLogic.countdownRemaining(root.deadlineMs, Date.now()).remaining
        root.running = false
        tick.stop()
    }

    function toggle() {
        if (root.running) root.pause()
        else root.start()
    }

    function reset() {
        root.running = false
        tick.stop()
        root.remainingMs = root.durationMs
    }

    function refresh() {
        if (!root.running) return
        var state = ClockLogic.countdownRemaining(root.deadlineMs, Date.now())
        if (state.done) {
            root.running = false
            tick.stop()
            root.remainingMs = 0
            root.alert("Timer", "Your " + ClockLogic.formatDuration(root.durationMs) + " timer is done")
            return
        }
        root.remainingMs = state.remaining
    }

    Timer {
        id: tick
        interval: 200
        repeat: true
        onTriggered: root.refresh()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spaceS

        // ---- the countdown --------------------------------------------------
        Card {
            Layout.fillWidth: true
            padding: Theme.spaceL

            ColumnLayout {
                width: parent.width
                spacing: Theme.spaceS

                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: root.remainingText
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Math.round(Theme.fontHeadline * 1.5)
                    font.weight: Font.Light
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 8
                    radius: height / 2
                    color: Theme.surfaceHigh

                    Rectangle {
                        width: root.progress * parent.width
                        height: parent.height
                        radius: parent.radius
                        color: ShellPrefs.accentColor
                    }
                }

                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: root.statusLine
                    color: Theme.textDim
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontCaption - 2
                }
            }
        }

        // Breathing space between the display and the controls, with a first-run hint so the
        // middle of the screen is never blank.
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Text {
                anchors.centerIn: parent
                width: parent.width * 0.8
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: "Choose a length below, then press Start."
                color: Theme.textDim
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody - 6
                opacity: (!root.running && root.remainingMs === root.durationMs) ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: Theme.animFast } }
            }
        }

        // ---- quick chips ----------------------------------------------------
        SectionLabel {
            Layout.fillWidth: true
            text: "QUICK SET"
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceS

            Repeater {
                model: [1, 5, 10, 25]

                delegate: Rectangle {
                    required property int modelData
                    Layout.fillWidth: true
                    Layout.preferredHeight: 48
                    radius: Theme.radiusPill
                    color: (root.durationMs === modelData * 60000 && !root.running)
                        ? Qt.alpha(ShellPrefs.accentColor, 0.22) : Theme.card
                    border.width: 1
                    border.color: (root.durationMs === modelData * 60000)
                        ? ShellPrefs.accentColor : Qt.alpha(Theme.text, 0.08)

                    Text {
                        anchors.centerIn: parent
                        text: modelData + " min"
                        color: (root.durationMs === modelData * 60000)
                            ? ShellPrefs.accentColor : Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontCaption
                        font.weight: Font.Medium
                    }
                    TapHandler { onTapped: root.setDuration(modelData * 60000) }
                }
            }
        }

        // ---- custom length --------------------------------------------------
        Card {
            Layout.fillWidth: true
            padding: Theme.spaceM

            RowLayout {
                width: parent.width
                spacing: Theme.spaceS

                Stepper {
                    Layout.fillWidth: true
                    label: "MINUTES"
                    valueText: ClockLogic.pad2(root.minutes)
                    onStep: function (delta) {
                        root.setDuration(ClockLogic.stepClamped(root.minutes, delta, 0, 99) * 60000
                                         + root.seconds * 1000)
                    }
                }
                Stepper {
                    Layout.fillWidth: true
                    label: "SECONDS"
                    valueText: ClockLogic.pad2(root.seconds)
                    onStep: function (delta) {
                        root.setDuration(root.minutes * 60000
                                         + ClockLogic.stepClamped(root.seconds, delta * 5, 0, 55) * 1000)
                    }
                }
            }
        }

        // ---- transport ------------------------------------------------------
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceS

            AccentButton {
                Layout.fillWidth: true
                text: root.running ? "Pause" : "Start"
                materialIcon: root.running ? "pause" : "play_arrow"
                enabled2: root.running || root.durationMs > 0
                onClicked: root.toggle()
            }
            SecondaryButton {
                Layout.fillWidth: true
                text: "Reset"
                materialIcon: "refresh"
                enabled2: root.running || root.remainingMs !== root.durationMs
                onClicked: root.reset()
            }
        }
    }
}
