import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import A21s.Theme
import A21s.Services
import A21s.Components as Components

// Standalone Notes app — launched by `a21s-notes`.
// Persists to ~/.config/a21s-shell/notes.json; supports Android edge-back.
ApplicationWindow {
    id: win
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    title: "Notes"
    color: Theme.bg
    font.family: Theme.fontFamily

    readonly property string notesPath: (Quickshell.env("HOME") || "/home/gamer")
        + "/.config/a21s-shell/notes.json"

    property var notes: [
        { id: "1", title: "Welcome to a21s-shell", body: "Swipe from the left or right edge to go back." }
    ]
    property int editingIndex: -1

    Process { id: saveProc }

    FileView {
        id: notesFile
        path: win.notesPath
        onLoaded: {
            try {
                var parsed = JSON.parse(text())
                if (Array.isArray(parsed))
                    win.notes = parsed
            } catch (e) {}
        }
    }

    function persist() {
        var payload = JSON.stringify(win.notes, null, 2)
        saveProc.command = [
            "sh", "-c",
            "mkdir -p \"$(dirname \"$1\")\" && printf '%s\n' \"$2\" > \"$1.tmp\" && mv \"$1.tmp\" \"$1\"",
            "_", win.notesPath, payload
        ]
        saveProc.running = true
    }

    function createNote() {
        var next = win.notes.slice()
        next.unshift({ id: String(Date.now()), title: "Untitled note", body: "" })
        win.notes = next
        persist()
        openEditor(0)
    }

    function openEditor(idx) {
        editingIndex = idx
        titleInput.text = win.notes[idx].title
        bodyInput.text = win.notes[idx].body
        stack.push(editorPage)
    }

    function saveCurrentAndBack() {
        if (editingIndex >= 0 && editingIndex < win.notes.length) {
            var next = win.notes.slice()
            next[editingIndex] = {
                id: next[editingIndex].id,
                title: titleInput.text.trim() || "Untitled note",
                body: bodyInput.text
            }
            win.notes = next
            persist()
        }
        editingIndex = -1
        if (stack.depth > 1)
            stack.pop()
    }

    function deleteCurrent() {
        if (editingIndex >= 0 && editingIndex < win.notes.length) {
            var next = win.notes.slice()
            next.splice(editingIndex, 1)
            win.notes = next
            persist()
        }
        editingIndex = -1
        if (stack.depth > 1)
            stack.pop()
    }

    StackView {
        id: stack
        anchors {
            fill: parent
            bottomMargin: Theme.navZoneHeight
        }
        initialItem: listPage
    }

    Component {
        id: listPage
        Item {
            ColumnLayout {
                anchors { fill: parent; margins: Theme.spaceM }
                spacing: Theme.spaceM

                RowLayout {
                    Layout.fillWidth: true
                    Components.MaterialIcon {
                        name: "edit_note"
                        size: Theme.iconS + 4
                        color: ShellPrefs.accentColor
                    }
                    Text {
                        Layout.fillWidth: true
                        text: "Notes"
                        color: Theme.text
                        font.pixelSize: Theme.fontTitle
                        font.weight: Font.Medium
                    }
                    Rectangle {
                        width: Theme.iconM + Theme.spaceS
                        height: Theme.iconM
                        radius: Theme.radiusPill
                        color: ShellPrefs.accentColor
                        Components.MaterialIcon {
                            anchors.centerIn: parent
                            name: "add"
                            size: Theme.iconS
                            color: Theme.primaryContent
                        }
                        TapHandler { onTapped: win.createNote() }
                    }
                }

                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: Theme.spaceS
                    model: win.notes
                    delegate: Rectangle {
                        required property int index
                        required property var modelData
                        width: ListView.view.width
                        implicitHeight: noteCol.implicitHeight + Theme.spaceM * 2
                        radius: Theme.radiusL
                        color: Theme.card
                        border.width: 1
                        border.color: Qt.alpha(Theme.text, 0.07)

                        ColumnLayout {
                            id: noteCol
                            anchors {
                                left: parent.left
                                right: parent.right
                                verticalCenter: parent.verticalCenter
                                leftMargin: Theme.spaceM
                                rightMargin: Theme.spaceM
                            }
                            spacing: 4
                            Text {
                                Layout.fillWidth: true
                                text: modelData.title
                                color: Theme.text
                                font.pixelSize: Theme.fontBody
                                font.weight: Font.Medium
                                elide: Text.ElideRight
                            }
                            Text {
                                Layout.fillWidth: true
                                text: modelData.body || "No content"
                                color: Theme.textDim
                                font.pixelSize: Theme.fontStatus
                                elide: Text.ElideRight
                                maximumLineCount: 2
                                wrapMode: Text.Wrap
                            }
                        }

                        TapHandler { onTapped: win.openEditor(index) }
                    }
                }
            }
        }
    }

    Component {
        id: editorPage
        Item {
            ColumnLayout {
                anchors { fill: parent; margins: Theme.spaceM }
                spacing: Theme.spaceM

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceS

                    Rectangle {
                        width: Theme.iconM; height: Theme.iconM; radius: Theme.radiusPill
                        color: Theme.surfaceHigh
                        Components.MaterialIcon {
                            anchors.centerIn: parent
                            name: "arrow_back"
                            size: Theme.iconS - 4
                            color: Theme.text
                        }
                        TapHandler { onTapped: win.saveCurrentAndBack() }
                    }

                    Item { Layout.fillWidth: true }

                    Rectangle {
                        width: Theme.iconM; height: Theme.iconM; radius: Theme.radiusPill
                        color: Qt.alpha(Theme.danger, 0.18)
                        Components.MaterialIcon {
                            anchors.centerIn: parent
                            name: "delete"
                            size: Theme.iconS - 4
                            color: Theme.danger
                        }
                        TapHandler { onTapped: win.deleteCurrent() }
                    }
                }

                TextInput {
                    id: titleInput
                    Layout.fillWidth: true
                    color: Theme.text
                    font.pixelSize: Theme.fontHeadline - 8
                    font.weight: Font.DemiBold
                    clip: true
                }

                Rectangle { Layout.fillWidth: true; height: 1; color: Theme.separator }

                TextArea {
                    id: bodyInput
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    color: Theme.text
                    font.pixelSize: Theme.fontBody
                    placeholderText: "Write something…"
                    placeholderTextColor: Theme.textFaint
                    wrapMode: TextArea.Wrap
                    background: Rectangle { color: "transparent" }
                }
            }
        }
    }

    Components.AppGestureLayer {
        anchors.fill: parent
        onBackTriggered: {
            if (stack.depth > 1)
                win.saveCurrentAndBack()
            else
                win.close()
        }
    }
}
