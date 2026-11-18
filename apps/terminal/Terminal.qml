import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets
import RabbizKit
import "TerminalLogic.js" as Term

// Terminal — in-house replacement for qmlkonsole (docs/BASE-APPS.md), MVP only.
// Each submitted line runs as ONE command through `script -qfec` so it gets a real PTY,
// then the app shows stdout+stderr (ANSI stripped) in a monospace scrollback. There is
// no persistent shell: `cd` and interactive programs do not continue between commands.
// The `timeout` wrapper makes sure no command can wedge the single slot forever.
ApplicationWindow {
    id: win
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    title: "Terminal"
    color: Theme.bg
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontBody

    Binding { target: Theme; property: "light"; value: ShellPrefs.lightTheme }

    // Theme owns every token but has no monospace family yet; this is the one font
    // literal in the app. Promote to Theme.monoFamily when the lead adds it.
    readonly property string monoFamily: "monospace"
    readonly property string home: Quickshell.env("HOME") || "/home/rabbiz"
    // Every command starts in $HOME (Process.workingDirectory), so the prompt is stable.
    readonly property string prompt: Term.formatPrompt(win.home, win.home)

    readonly property int timeoutSec: 20
    readonly property int maxHistory: 40
    readonly property int maxEntries: 30
    readonly property int maxOutputChars: 12000
    readonly property var chips: ["ls", "pwd", "df -h", "ip a", "uname -a"]
    readonly property string historyPath: win.home + "/.local/share/rabbiz-terminal/history.json"

    // Touch metric: one 48 px height shared by chips, input and action pills.
    readonly property int controlH: Theme.spaceXL
    readonly property bool canRun: !win.running && input.text.trim().length > 0

    property var commandHistory: []
    property int historyIndex: -1
    property string draftSaved: ""
    property bool running: false
    property string pendingOut: ""
    property int activeRow: -1

    // The OSK is an overlay under labwc/squeekboard, so leave room for it explicitly.
    // Squeekboard is input-method-v2 and does not report a keyboard rectangle to a
    // text-input-v3 app, so gate on focus and fall back to a share of the window.
    readonly property int oskInset: {
        if (!input.activeFocus)
            return Theme.spaceS
        var h = Qt.inputMethod.keyboardRectangle.height
        if (h <= 0)
            h = Math.round(win.height * 0.30)
        return Math.min(h, Math.round(win.height * 0.55))
    }

    ListModel { id: log }

    // -----------------------------------------------------------------------
    // Running a command
    // -----------------------------------------------------------------------

    function submit() {
        if (win.running) return
        var cmd = input.text.trim()
        if (cmd.length === 0) return
        win.runCommand(cmd)
    }

    function runCommand(command) {
        if (win.running) return
        var cmd = String(command).trim()
        if (cmd.length === 0) return

        win.commandHistory = Term.addHistory(win.commandHistory, cmd, win.maxHistory)
        win.saveHistory()
        win.historyIndex = -1
        win.draftSaved = ""

        log.append({
            command: cmd,
            output: "",
            status: "running",
            failed: false,
            note: ""
        })
        if (log.count > win.maxEntries)
            log.remove(0, log.count - win.maxEntries)
        win.activeRow = log.count - 1

        win.pendingOut = ""
        win.running = true

        // `-f` flushes, `-e` returns the child's exit code, `-q` suppresses the banner.
        // timeout keeps a stuck command from holding the single slot forever.
        runProc.command = ["timeout", "--foreground", String(win.timeoutSec),
                           "script", "-q", "-f", "-e", "-c", cmd, "/dev/null"]
        runProc.running = true

        input.text = ""
        win.scrollToEnd()
    }

    // Called once per command: either by Process.onExited or by the watchdog/stop path.
    function finish(code, stoppedByUser) {
        if (!win.running) return
        win.running = false
        var row = win.activeRow
        win.activeRow = -1
        var raw = win.pendingOut
        win.pendingOut = ""
        if (row < 0 || row >= log.count) return

        var capped = Term.capOutput(Term.cleanOutput(raw), win.maxOutputChars)
        var notes = []
        var exitNote = ""
        if (capped.truncated)
            notes.push("Output cut to " + win.maxOutputChars + " characters")
        if (stoppedByUser) {
            notes.push("Stopped")
        } else {
            exitNote = Term.describeExit(code, win.timeoutSec)
            if (exitNote.length > 0) notes.push(exitNote)
        }
        if (capped.text.length === 0 && notes.length === 0) notes.push("No output.")

        log.setProperty(row, "output", capped.text)
        log.setProperty(row, "status", stoppedByUser ? "stopped" : (code === 0 ? "done" : "failed"))
        log.setProperty(row, "failed", !stoppedByUser && code !== 0)
        log.setProperty(row, "note", notes.join(" · "))
        win.scrollToEnd()
    }

    function stopRunning() {
        if (!win.running) return
        runProc.running = false          // SIGTERM to timeout, which stops the command
        win.finish(-1, true)
    }

    function clearLog() {
        if (win.running) win.stopRunning()
        log.clear()
        win.historyIndex = -1
        win.draftSaved = ""
        input.text = ""
    }

    function scrollToEnd() {
        Qt.callLater(function () { logView.positionViewAtEnd() })
    }

    // -----------------------------------------------------------------------
    // History navigation (also wired to the ↑ / ↓ buttons)
    // -----------------------------------------------------------------------

    function historyPrev() {
        var r = Term.historyUp(win.commandHistory, win.historyIndex)
        if (r.text === null) return
        if (win.historyIndex === -1) win.draftSaved = input.text
        win.historyIndex = r.index
        input.text = r.text
        input.cursorPosition = input.text.length
    }

    function historyNext() {
        if (win.historyIndex === -1) return
        var r = Term.historyDown(win.commandHistory, win.historyIndex, win.draftSaved)
        win.historyIndex = r.index
        input.text = r.text
        input.cursorPosition = input.text.length
    }

    Process {
        id: runProc
        workingDirectory: win.home
        stdout: StdioCollector {
            onStreamFinished: win.pendingOut = win.pendingOut + this.text
        }
        stderr: StdioCollector {
            onStreamFinished: win.pendingOut = win.pendingOut + this.text
        }
        // Let both collectors flush before the output is cleaned and shown.
        onExited: function (code, status) {
            var exitCode = code
            Qt.callLater(function () { win.finish(exitCode, false) })
        }
    }

    function saveHistory() {
        // Atomic write (tmp + mv), same pattern as the Calculator history.
        saveProc.command = ["sh", "-c",
            "mkdir -p \"$(dirname \"$1\")\" && printf '%s' \"$2\" > \"$1.tmp\" && mv \"$1.tmp\" \"$1\"",
            "_", win.historyPath, JSON.stringify(win.commandHistory)]
        saveProc.running = true
    }

    Process { id: saveProc }

    FileView {
        id: historyFile
        path: win.historyPath
        onLoaded: {
            win.commandHistory = Term.loadHistory(text(), win.maxHistory)
            win.historyIndex = -1
        }
        Component.onCompleted: historyFile.reload()
    }

    // Last-resort escape hatch: if the wrapper itself hangs or never exits, free the slot.
    Timer {
        interval: (win.timeoutSec + 15) * 1000
        running: win.running
        onTriggered: {
            if (!win.running) return
            runProc.running = false
            win.finish(124, false)
        }
    }

    // -----------------------------------------------------------------------
    // Page
    // -----------------------------------------------------------------------

    ColumnLayout {
        anchors { fill: parent; margins: Theme.spaceM }
        spacing: Theme.spaceS

        AppHeader {
            Layout.fillWidth: true
            title: "Terminal"
            subtitle: win.running ? "Running…" : "One command at a time"

            // Clear: always visible, clears the scrollback (and stops a running command).
            Rectangle {
                width: clearRow.implicitWidth + Theme.spaceL
                height: win.controlH
                radius: height / 2
                color: Theme.surfaceHigh
                border.width: 1
                border.color: Qt.alpha(Theme.text, 0.10)
                opacity: log.count === 0 && !win.running ? 0.55 : 1.0
                scale: clearTap.pressed ? 0.96 : 1.0
                Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                Row {
                    id: clearRow
                    anchors.centerIn: parent
                    spacing: Theme.spaceXS
                    MaterialIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "delete_sweep"
                        size: Theme.fontCaption
                        color: Theme.textDim
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "Clear"
                        color: Theme.text
                        font.pixelSize: Theme.fontBody - 6
                        font.family: Theme.fontFamily
                        font.weight: Font.Medium
                    }
                }
                TapHandler { id: clearTap; onTapped: win.clearLog() }
            }
        }

        // ---- scrollback --------------------------------------------------
        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            radius: Theme.radiusL
            color: Theme.card
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.08)
            clip: true

            ListView {
                id: logView
                anchors { fill: parent; margins: Theme.spaceS }
                clip: true
                model: log
                spacing: 0
                cacheBuffer: 2000
                boundsBehavior: Flickable.StopAtBounds
                onCountChanged: win.scrollToEnd()

                delegate: Rectangle {
                    id: block
                    required property int index
                    required property string command
                    required property string output
                    required property string status
                    required property bool failed
                    required property string note

                    width: ListView.view.width
                    height: body.implicitHeight + Theme.spaceS * 2
                    radius: Theme.radiusS
                    color: block.status === "running" ? Qt.alpha(ShellPrefs.accentColor, 0.10)
                                                      : "transparent"

                    Column {
                        id: body
                        x: Theme.spaceS
                        y: Theme.spaceS
                        width: parent.width - Theme.spaceS * 2
                        spacing: Theme.spaceXS

                        Row {
                            width: parent.width
                            spacing: Theme.spaceS

                            Text {
                                id: promptText
                                text: win.prompt
                                color: ShellPrefs.accentColor
                                font.family: win.monoFamily
                                font.pixelSize: Theme.fontCaption
                                font.weight: Font.Medium
                            }
                            Text {
                                width: Math.max(0, parent.width - promptText.width - parent.spacing)
                                text: block.command
                                color: Theme.text
                                font.family: win.monoFamily
                                font.pixelSize: Theme.fontCaption
                                font.weight: Font.DemiBold
                                textFormat: Text.PlainText
                                wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                            }
                        }

                        Text {
                            width: parent.width
                            visible: block.output.length > 0
                            text: block.output
                            color: Theme.text
                            font.family: win.monoFamily
                            font.pixelSize: Theme.fontCaption
                            lineHeight: 1.15
                            textFormat: Text.PlainText
                            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
                        }

                        Text {
                            width: parent.width
                            visible: block.status === "running" || block.note.length > 0
                            text: block.status === "running" ? "Running…" : block.note
                            color: block.status === "running" ? ShellPrefs.accentColor
                                 : block.failed ? Theme.danger : Theme.textFaint
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontCaption - 4
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                    }
                }
            }

            EmptyState {
                visible: log.count === 0
                anchors.centerIn: parent
                width: parent.width - Theme.spaceL * 2
                icon: "code"
                title: "No commands yet"
                hint: "Type a command below, or tap a suggestion. Each line runs on its own in your home folder."
            }
        }

        // ---- command suggestions ----------------------------------------
        Flickable {
            id: chipFlick
            Layout.fillWidth: true
            Layout.preferredHeight: win.controlH
            contentWidth: chipsRow.width
            contentHeight: height
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.HorizontalFlick

            Row {
                id: chipsRow
                height: chipFlick.height
                spacing: Theme.spaceS

                Repeater {
                    model: win.chips
                    delegate: Rectangle {
                        required property var modelData

                        height: parent.height
                        width: chipLabel.implicitWidth + Theme.spaceL
                        radius: height / 2
                        color: Theme.surfaceHigh
                        border.width: 1
                        border.color: Qt.alpha(Theme.text, 0.10)
                        opacity: win.running ? 0.45 : 1.0
                        scale: chipTap.pressed ? 0.96 : 1.0
                        Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                        Text {
                            id: chipLabel
                            anchors.centerIn: parent
                            text: String(modelData)
                            color: ShellPrefs.accentColor
                            font.family: win.monoFamily
                            font.pixelSize: Theme.fontCaption
                        }
                        TapHandler {
                            id: chipTap
                            onTapped: if (!win.running) win.runCommand(String(modelData))
                        }
                    }
                }
            }
        }

        // ---- input line --------------------------------------------------
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceS

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: win.controlH
                radius: Theme.radiusM
                color: Theme.surface
                border.width: 1
                border.color: input.activeFocus ? Qt.alpha(ShellPrefs.accentColor, 0.65)
                                                : Qt.alpha(Theme.text, 0.10)

                TextField {
                    id: input
                    anchors { fill: parent; leftMargin: Theme.spaceM; rightMargin: Theme.spaceM }
                    padding: 0
                    placeholderText: "Type a command…"
                    placeholderTextColor: Theme.textFaint
                    color: Theme.text
                    font.family: win.monoFamily
                    font.pixelSize: Theme.fontBody - 4
                    verticalAlignment: TextInput.AlignVCenter
                    selectByMouse: true
                    inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                    background: Rectangle { color: "transparent" }
                    onAccepted: win.submit()
                    Keys.onUpPressed: win.historyPrev()
                    Keys.onDownPressed: win.historyNext()
                }
            }

            // ↑ / ↓ history buttons: 48 px targets, useable without a hardware keyboard.
            Rectangle {
                Layout.preferredWidth: win.controlH
                Layout.preferredHeight: win.controlH
                radius: Theme.radiusM
                color: Theme.surfaceHigh
                border.width: 1
                border.color: Qt.alpha(Theme.text, 0.10)
                opacity: win.commandHistory.length > 0 ? 1.0 : 0.40
                scale: upTap.pressed ? 0.94 : 1.0
                Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                MaterialIcon {
                    anchors.centerIn: parent
                    name: "keyboard_arrow_up"
                    size: Theme.fontCaption + 2
                    color: Theme.text
                }
                TapHandler {
                    id: upTap
                    onTapped: if (win.commandHistory.length > 0) win.historyPrev()
                }
            }

            Rectangle {
                Layout.preferredWidth: win.controlH
                Layout.preferredHeight: win.controlH
                radius: Theme.radiusM
                color: Theme.surfaceHigh
                border.width: 1
                border.color: Qt.alpha(Theme.text, 0.10)
                opacity: win.historyIndex !== -1 ? 1.0 : 0.40
                scale: downTap.pressed ? 0.94 : 1.0
                Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                MaterialIcon {
                    anchors.centerIn: parent
                    name: "keyboard_arrow_down"
                    size: Theme.fontCaption + 2
                    color: Theme.text
                }
                TapHandler {
                    id: downTap
                    onTapped: if (win.historyIndex !== -1) win.historyNext()
                }
            }

            // Run / Stop: the single slot is obvious, including how to get out of it.
            Rectangle {
                Layout.preferredHeight: win.controlH
                Layout.preferredWidth: runRow.implicitWidth + Theme.spaceL
                radius: height / 2
                color: win.running ? Theme.surfaceHigh
                     : (win.canRun ? ShellPrefs.accentColor : Theme.surfaceHigh)
                border.width: win.running ? 1 : 0
                border.color: Qt.alpha(Theme.danger, 0.55)
                scale: runTap.pressed ? 0.96 : 1.0
                Behavior on scale { NumberAnimation { duration: Theme.animFast } }
                Behavior on color { ColorAnimation { duration: Theme.animFast } }

                Row {
                    id: runRow
                    anchors.centerIn: parent
                    spacing: Theme.spaceXS
                    MaterialIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: win.running ? "stop" : "play_arrow"
                        size: Theme.fontCaption + 2
                        color: win.running ? Theme.danger
                             : (win.canRun ? ShellPrefs.onAccentColor : Theme.textDim)
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: win.running ? "Stop" : "Run"
                        color: win.running ? Theme.danger
                             : (win.canRun ? ShellPrefs.onAccentColor : Theme.textDim)
                        font.pixelSize: Theme.fontBody - 6
                        font.family: Theme.fontFamily
                        font.weight: Font.Medium
                    }
                }
                TapHandler {
                    id: runTap
                    onTapped: {
                        if (win.running) win.stopRunning()
                        else win.submit()
                    }
                }
            }
        }

        // ---- honesty line + room for the on-screen keyboard --------------
        Text {
            Layout.fillWidth: true
            text: "This is a single-command terminal, not a full shell session: cd and interactive apps do not persist. Commands stop after "
                  + win.timeoutSec + " s."
            color: Theme.textFaint
            font.pixelSize: Theme.fontCaption - 4
            font.family: Theme.fontFamily
            wrapMode: Text.Wrap
        }

        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: win.oskInset - Theme.spaceS
        }
    }
}
