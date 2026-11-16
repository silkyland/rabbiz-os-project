import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import A21s.Theme
import A21s.Services
import A21s.Widgets
import A21sKit
import "NotesLogic.js" as Notes

// Notes — in-house replacement for marknote (docs/BASE-APPS.md).
// Two screens in one maximized window: the list (search, newest first, floating New note) and
// the editor (title + plain-text body, autosave ~800 ms after typing stops). Notes live one per
// file under ~/.local/share/a21s-notes/<timestamp>.json, written atomically (tmp + mv) and read
// back as newline-separated records. All layout values come from Theme; the accent comes from
// ShellPrefs so the app follows the shell's accent, and Theme.light follows the shell theme.
ApplicationWindow {
    id: win
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    title: "Notes"
    color: Theme.bg
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontBody

    Binding { target: Theme; property: "light"; value: ShellPrefs.lightTheme }

    // ---- model ----------------------------------------------------------------
    property var notes: []              // newest first, as returned by Notes.sortNotes
    property bool loaded: false
    property bool loadFailed: false     // the directory could not be read (still usable: writes)
    property string query: ""
    property double nowMs: Date.now()   // refreshed so "5 mins ago" labels stay honest

    // ---- editor session -------------------------------------------------------
    property string editingId: ""       // "" = list screen
    property string editTitle: ""
    property string editBody: ""
    property double editCreatedAt: 0
    property double editUpdatedAt: 0
    property bool newDraft: false       // discard a brand-new note that was never typed into
    property bool dirty: false          // in-memory text differs from what is on disk
    property string saveState: "saved"  // idle | dirty | saving | saved | error
    property string deletingId: ""      // note awaiting delete confirmation
    property bool applyingDraft: false  // guard: programmatic text edits are not user edits
    property int autosaveMs: 800

    // ---- write queue: one shell job at a time, deletes ordered behind saves ----
    property var writeQueue: []
    property bool writing: false
    property string activeJobId: ""

    readonly property var visibleNotes: Notes.searchNotes(win.notes, win.query)
    readonly property var pendingNote: win.findNote(win.deletingId)
    readonly property string notesDir: (Quickshell.env("HOME") || "/home/gamer")
                                        + "/.local/share/a21s-notes"

    readonly property string listSubtitle: {
        if (!win.loaded) return "Opening\u2026"
        if (win.loadFailed && win.notes.length === 0) return "Storage unreadable"
        if (win.notes.length === 0) return "Your notes live here"
        if (win.query.trim().length > 0)
            return win.visibleNotes.length + " of " + win.notes.length + " notes"
        return win.notes.length === 1 ? "1 note" : win.notes.length + " notes"
    }

    readonly property string saveLabel: {
        switch (win.saveState) {
        case "dirty": return "Unsaved changes"
        case "saving": return "Saving\u2026"
        case "error": return "Not saved"
        case "saved": return "Saved"
        }
        return "Autosave on"
    }

    readonly property color saveColor: {
        if (win.saveState === "error") return Theme.danger
        if (win.saveState === "dirty") return Theme.warn
        if (win.saveState === "saving") return ShellPrefs.accentColor
        if (win.saveState === "saved") return Theme.success
        return Theme.textDim
    }

    readonly property string deleteMessage: {
        var note = win.pendingNote
        if (!note) return ""
        return "\u201C" + Notes.titleOf(note) + "\u201D will be removed from this device."
                + " This cannot be undone."
    }

    // ---- helpers -----------------------------------------------------------------
    function notePath(id) {
        return win.notesDir + "/" + id + ".json"
    }

    function findNote(id) {
        if (!id) return null
        for (var i = 0; i < win.notes.length; i++) {
            var note = win.notes[i]
            if (note && note.id === id) return note
        }
        return null
    }

    function upsert(note) {
        var out = []
        var replaced = false
        for (var i = 0; i < win.notes.length; i++) {
            var existing = win.notes[i]
            if (existing && existing.id === note.id) {
                out.push(note)
                replaced = true
            } else {
                out.push(existing)
            }
        }
        if (!replaced) out.push(note)
        win.notes = Notes.sortNotes(out)
    }

    function openNote(id) {
        var note = win.findNote(id)
        if (!note) return
        win.beginEdit(note, false)
    }

    function createNote() {
        // A new note must be visible once written, so drop any active search.
        searchField.text = ""
        win.query = ""
        var now = Date.now()
        win.beginEdit({
            id: Notes.nextId(win.notes, now),
            title: "",
            body: "",
            createdAt: now,
            updatedAt: now
        }, true)
    }

    function beginEdit(note, isNew) {
        saveTimer.stop()
        // Opening a note is not a typing action: drop focus so the OSK stays down.
        searchField.focus = false
        titleField.focus = false
        bodyArea.focus = false
        win.applyingDraft = true
        win.editingId = note.id
        win.editTitle = note.title
        win.editBody = note.body
        win.editCreatedAt = note.createdAt
        win.editUpdatedAt = note.updatedAt
        win.newDraft = isNew
        win.dirty = false
        win.saveState = isNew ? "idle" : "saved"
        win.deletingId = ""
        titleField.text = note.title
        bodyArea.text = note.body
        bodyArea.cursorPosition = 0
        bodyFlick.contentY = 0
        win.applyingDraft = false
        win.nowMs = Date.now()
        // Only a brand-new note grabs the keyboard; opening an old one is a reading action.
        if (isNew) Qt.callLater(function () { titleField.forceActiveFocus() })
    }

    function stopEditing() {
        saveTimer.stop()
        win.deletingId = ""
        win.editingId = ""
        win.newDraft = false
        win.dirty = false
        // Dropping focus is what dismisses squeekboard (text-input-v3).
        titleField.focus = false
        bodyArea.focus = false
        listPage.forceActiveFocus()
    }

    function draftChanged() {
        if (win.applyingDraft || win.editingId === "") return
        win.editTitle = titleField.text
        win.editBody = bodyArea.text
        win.dirty = true
        win.saveState = "dirty"
        saveTimer.restart()
    }

    function finishEditing() {
        saveTimer.stop()
        var blank = titleField.text.trim().length === 0 && bodyArea.text.trim().length === 0
        // A brand-new note the user never typed into is not worth a file on disk; if an
        // autosave already created it, remove it again so the list and disk agree.
        if (win.newDraft && blank) {
            if (win.findNote(win.editingId) !== null) win.deleteNote(win.editingId)
            else win.stopEditing()
            return
        }
        if (win.dirty) win.saveDraft()
        win.stopEditing()
    }

    function saveDraft() {
        saveTimer.stop()
        if (win.editingId === "") return
        var now = Date.now()
        var note = {
            id: win.editingId,
            title: win.editTitle,
            body: win.editBody,
            createdAt: win.editCreatedAt > 0 ? win.editCreatedAt : now,
            updatedAt: now
        }
        win.editCreatedAt = note.createdAt
        win.editUpdatedAt = now
        win.nowMs = now
        win.dirty = false
        win.saveState = "saving"
        win.upsert(note)
        win.writeQueue.push({ note: note })
        win.pumpWrites()
    }

    function askDelete() {
        if (win.editingId === "") return
        saveTimer.stop()
        // Hide the keyboard for the confirmation; editing resumes on the next tap.
        titleField.focus = false
        bodyArea.focus = false
        win.deletingId = win.editingId
    }

    function cancelDelete() {
        win.deletingId = ""
        // Keep the autosave promise: unsaved text is still written 800 ms from now.
        if (win.editingId !== "" && win.dirty) saveTimer.restart()
    }

    function confirmDelete() {
        var id = win.deletingId
        win.deletingId = ""
        if (id === "") return
        win.deleteNote(id)
    }

    // Removes a note from the list and from disk, behind any in-flight write for it.
    function deleteNote(id) {
        if (id === "") return
        saveTimer.stop()
        win.writeQueue = win.writeQueue.filter(function (job) {
            return !(job.note && job.note.id === id) && job.deleteId !== id
        })
        var keep = []
        for (var i = 0; i < win.notes.length; i++) {
            if (win.notes[i].id !== id) keep.push(win.notes[i])
        }
        win.notes = keep
        win.writeQueue.push({ deleteId: id })
        win.pumpWrites()
        if (win.editingId === id) win.stopEditing()
    }

    function pumpWrites() {
        if (win.writing || win.writeQueue.length === 0) return
        var job = win.writeQueue.shift()
        win.writing = true
        win.activeJobId = job.note ? job.note.id : (job.deleteId || "")
        if (job.deleteId) {
            removeProc.command = ["sh", "-c", "rm -f \"$1\"", "_", win.notePath(job.deleteId)]
            removeProc.running = true
        } else {
            saveProc.command = ["sh", "-c",
                "mkdir -p \"$1\" && printf '%s' \"$3\" > \"$2.tmp\" && mv \"$2.tmp\" \"$2\"",
                "_", win.notesDir, win.notePath(job.note.id), Notes.serialize(job.note)]
            saveProc.running = true
        }
    }

    function jobDone(code) {
        var id = win.activeJobId
        win.activeJobId = ""
        win.writing = false
        if (code !== 0) {
            // Keep the text in memory and let the indicator offer a retry.
            win.dirty = true
            if (id !== "" && id === win.editingId) win.saveState = "error"
        } else if (win.writeQueue.length === 0 && !win.dirty
                   && id !== "" && id === win.editingId) {
            win.saveState = "saved"
        }
        win.pumpWrites()
    }

    // ---- disk ------------------------------------------------------------------
    Process {
        id: loadProc
        // One note per line; the app writes JSON with no raw newlines, so this is lossless
        // and corrupt files are skipped by Notes.parseDump.
        command: ["sh", "-c",
            "d=\"$1\"; mkdir -p \"$d\"; for f in \"$d\"/*.json; do"
            + " if [ -f \"$f\" ]; then cat \"$f\"; echo; fi; done",
            "_", win.notesDir]
        stdout: StdioCollector {
            onStreamFinished: {
                loadGuard.stop()
                var merged = Notes.parseDump(this.text)
                // A note saved while the directory was being read must not vanish from the
                // list: the file is on disk either way, this only keeps the view honest.
                for (var i = 0; i < win.notes.length; i++) {
                    var seen = false
                    for (var j = 0; j < merged.length; j++) {
                        if (merged[j].id === win.notes[i].id) { seen = true; break }
                    }
                    if (!seen) merged.push(win.notes[i])
                }
                win.notes = Notes.sortNotes(merged)
                win.loadFailed = false
                win.loaded = true
                win.nowMs = Date.now()
            }
        }
        Component.onCompleted: {
            loadProc.running = true
            loadGuard.start()
        }
    }

    // Belt and braces: never leave the list stuck on "Opening…" if the read never reports.
    Timer {
        id: loadGuard
        interval: 1000
        repeat: false
        onTriggered: {
            if (win.loaded) return
            win.loadFailed = true
            win.loaded = true
        }
    }

    Process {
        id: saveProc
        onExited: function (code, status) { win.jobDone(code) }
    }

    Process {
        id: removeProc
        onExited: function (code, status) { win.jobDone(code) }
    }

    Timer {
        id: saveTimer
        interval: win.autosaveMs
        repeat: false
        onTriggered: win.saveDraft()
    }

    Timer {
        interval: 30000
        running: true
        repeat: true
        onTriggered: win.nowMs = Date.now()
    }

    // ---- list screen ------------------------------------------------------------
    Item {
        id: listPage
        anchors.fill: parent
        visible: win.editingId === ""

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: Theme.spaceM
            spacing: Theme.spaceM

            AppHeader {
                Layout.fillWidth: true
                title: "Notes"
                subtitle: win.listSubtitle
            }

            // Search: one 60 px box, clear button on the right, no separate "go" step.
            Rectangle {
                id: searchBox
                Layout.fillWidth: true
                Layout.preferredHeight: Theme.spaceXL + Theme.spaceS
                radius: Theme.radiusM
                color: Theme.surfaceHigh
                border.width: 1
                border.color: searchField.activeFocus
                              ? Qt.alpha(ShellPrefs.accentColor, 0.65)
                              : Qt.alpha(Theme.text, 0.08)

                MaterialIcon {
                    id: searchIcon
                    anchors {
                        left: parent.left
                        leftMargin: Theme.spaceM
                        verticalCenter: parent.verticalCenter
                    }
                    name: "search"
                    size: Theme.fontCaption
                    color: Theme.textDim
                }

                TextField {
                    id: searchField
                    anchors {
                        left: searchIcon.right
                        leftMargin: Theme.spaceS
                        right: clearSearchHit.left
                        top: parent.top
                        bottom: parent.bottom
                    }
                    padding: 0
                    background: null
                    color: Theme.text
                    placeholderText: "Search notes"
                    placeholderTextColor: Theme.textFaint
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody - 4
                    verticalAlignment: Text.AlignVCenter
                    selectByMouse: true
                    onTextChanged: win.query = text
                }

                Item {
                    id: clearSearchHit
                    width: Theme.spaceXL
                    anchors {
                        right: parent.right
                        top: parent.top
                        bottom: parent.bottom
                    }
                    visible: win.query.length > 0

                    MaterialIcon {
                        anchors.centerIn: parent
                        name: "close"
                        size: Theme.fontCaption
                        color: Theme.textDim
                    }
                    TapHandler {
                        onTapped: {
                            searchField.text = ""
                            win.query = ""
                            searchField.forceActiveFocus()
                        }
                    }
                }
            }

            // Plain Rectangle instead of Card: this block must fill the remaining height,
            // which Card's content-sized implicitHeight cannot do (see Card.qml).
            Rectangle {
                id: listCard
                Layout.fillWidth: true
                Layout.fillHeight: true
                radius: Theme.radiusL
                color: Theme.card
                border.width: 1
                border.color: Qt.alpha(Theme.text, 0.08)

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Theme.spaceS
                    spacing: Theme.spaceXS

                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: Theme.spaceS
                        Layout.rightMargin: Theme.spaceS

                        SectionLabel {
                            Layout.fillWidth: true
                            text: win.query.length > 0 ? "MATCHES" : "ALL NOTES"
                        }
                        SectionLabel {
                            visible: win.loaded && win.notes.length > 0
                            text: win.query.length > 0
                                  ? win.visibleNotes.length + " / " + win.notes.length
                                  : win.notes.length
                        }
                    }

                    Item {
                        Layout.fillWidth: true
                        Layout.fillHeight: true

                        ListView {
                            id: listView
                            anchors.fill: parent
                            clip: true
                            visible: win.visibleNotes.length > 0
                            model: win.visibleNotes
                            spacing: Theme.spaceXS
                            boundsBehavior: Flickable.StopAtBounds
                            // Software renderer: keep roughly two screens of rows ready.
                            cacheBuffer: Theme.iconL * 24
                            // Room for the floating button, so the last note stays tappable.
                            footer: Item {
                                width: listView.width
                                height: Theme.iconL + Theme.spaceL
                            }
                            delegate: RowItem {
                                required property var modelData
                                width: listView.width
                                height: Theme.iconL + Theme.spaceS
                                title: Notes.titleOf(modelData)
                                caption: Notes.previewOf(modelData)
                                value: Notes.formatEdited(modelData.updatedAt, win.nowMs)
                                showChevron: false
                                onClicked: win.openNote(modelData.id)
                            }
                        }

                        // No notes at all.
                        EmptyState {
                            visible: win.loaded && !win.loadFailed && win.notes.length === 0
                            anchors.centerIn: parent
                            icon: "note"
                            title: "No notes yet"
                            hint: "Tap New note to write your first one."
                        }

                        // The directory could not be read, but the app still works.
                        EmptyState {
                            visible: win.loaded && win.loadFailed && win.notes.length === 0
                            anchors.centerIn: parent
                            icon: "error_outline"
                            title: "Couldn't read your notes"
                            hint: "New notes still save to ~/.local/share/a21s-notes."
                        }

                        // Notes exist, but the search matched none: offer the way back.
                        Column {
                            visible: win.loaded && win.notes.length > 0
                                     && win.visibleNotes.length === 0
                            anchors.centerIn: parent
                            width: parent.width
                            spacing: Theme.spaceM

                            EmptyState {
                                icon: "search"
                                title: "No notes match"
                                hint: "Nothing contains \u201C" + win.query.trim() + "\u201D."
                            }

                            Rectangle {
                                id: clearSearchButton
                                anchors.horizontalCenter: parent.horizontalCenter
                                width: clearSearchLabel.implicitWidth + Theme.spaceL
                                height: Theme.spaceXL
                                radius: height / 2
                                color: Theme.surfaceHigh
                                border.width: 1
                                border.color: Qt.alpha(Theme.text, 0.10)

                                Text {
                                    id: clearSearchLabel
                                    anchors.centerIn: parent
                                    text: "Clear search"
                                    color: ShellPrefs.accentColor
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontCaption - 4
                                    font.weight: Font.Medium
                                }
                                TapHandler {
                                    onTapped: {
                                        searchField.text = ""
                                        win.query = ""
                                    }
                                }
                            }
                        }

                        // First frame: never a blank card.
                        EmptyState {
                            visible: !win.loaded
                            anchors.centerIn: parent
                            icon: "hourglass_empty"
                            title: "Opening your notes\u2026"
                            hint: "Reading from ~/.local/share/a21s-notes"
                        }
                    }
                }
            }
        }

        // Floating New note — 58 px tall, above the system gesture area.
        Rectangle {
            id: newButton
            anchors {
                right: parent.right
                bottom: parent.bottom
                rightMargin: Theme.spaceM
                bottomMargin: Theme.navZoneHeight + Theme.spaceM
            }
            width: newRow.implicitWidth + Theme.spaceL
            height: Theme.iconM + Theme.spaceXS
            radius: height / 2
            color: ShellPrefs.accentColor
            scale: newTap.pressed ? 0.96 : 1.0
            Behavior on scale { NumberAnimation { duration: Theme.animFast } }

            Row {
                id: newRow
                anchors.centerIn: parent
                spacing: Theme.spaceXS

                MaterialIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "add"
                    size: Theme.fontBody
                    color: ShellPrefs.onAccentColor
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: "New note"
                    color: ShellPrefs.onAccentColor
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody - 4
                    font.weight: Font.Medium
                }
            }
            TapHandler { id: newTap; onTapped: win.createNote() }
        }
    }

    // ---- editor screen ----------------------------------------------------------
    Item {
        id: editorPage
        anchors.fill: parent
        visible: win.editingId !== ""

        ColumnLayout {
            anchors.fill: parent
            anchors.leftMargin: Theme.spaceM
            anchors.rightMargin: Theme.spaceM
            anchors.topMargin: Theme.spaceM
            // Leaves the bottom two icon-rows clear for squeekboard and the nav gesture.
            anchors.bottomMargin: Theme.spaceXL * 2
            spacing: Theme.spaceM

            AppHeader {
                Layout.fillWidth: true
                title: "Edit note"
                subtitle: "Plain text"

                // Save indicator: colour + words, always visible, tappable to retry.
                Rectangle {
                    id: saveChip
                    anchors.verticalCenter: parent.verticalCenter
                    width: saveChipLabel.implicitWidth + Theme.spaceM
                    height: saveChipLabel.implicitHeight + Theme.spaceXS * 2
                    radius: height / 2
                    color: Qt.alpha(win.saveColor, 0.16)

                    Text {
                        id: saveChipLabel
                        anchors.centerIn: parent
                        text: win.saveLabel
                        color: win.saveColor
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontCaption - 4
                    }
                    TapHandler {
                        enabled: win.saveState === "error"
                        onTapped: win.saveDraft()
                    }
                }

                IconButton {
                    icon: "delete_outline"
                    tint: Theme.danger
                    onClicked: win.askDelete()
                }

                PrimaryButton {
                    text: "Done"
                    materialIcon: "check"
                    onClicked: win.finishEditing()
                }
            }

            // Optional title; when left empty the first line of the body is used instead.
            TextField {
                id: titleField
                Layout.fillWidth: true
                Layout.preferredHeight: Theme.iconL + Theme.spaceXS
                leftPadding: Theme.spaceM
                rightPadding: Theme.spaceM
                color: Theme.text
                placeholderText: "Title (optional)"
                placeholderTextColor: Theme.textFaint
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                font.weight: Font.Medium
                verticalAlignment: Text.AlignVCenter
                selectByMouse: true
                inputMethodHints: Qt.ImhNone
                background: Rectangle {
                    radius: Theme.radiusM
                    color: Theme.card
                    border.width: 1
                    border.color: titleField.activeFocus
                                  ? Qt.alpha(ShellPrefs.accentColor, 0.65)
                                  : Qt.alpha(Theme.text, 0.08)
                }
                onTextChanged: win.draftChanged()
            }

            Rectangle {
                id: bodyCard
                Layout.fillWidth: true
                Layout.fillHeight: true
                radius: Theme.radiusL
                color: Theme.card
                border.width: 1
                border.color: bodyArea.activeFocus
                              ? Qt.alpha(ShellPrefs.accentColor, 0.65)
                              : Qt.alpha(Theme.text, 0.08)
                clip: true

                // TextArea.flickable is Qt's own way to make a TextArea scrollable: the
                // Flickable gets the content size from the text, and the cursor stays in
                // view while typing (QtQuick.Controls attaches ensureCursorVisible).
                Flickable {
                    id: bodyFlick
                    anchors.fill: parent
                    contentWidth: width
                    clip: true
                    flickableDirection: Flickable.VerticalFlick
                    boundsBehavior: Flickable.StopAtBounds

                    TextArea.flickable: TextArea {
                        id: bodyArea
                        textFormat: TextEdit.PlainText
                        wrapMode: TextEdit.Wrap
                        color: Theme.text
                        placeholderText: "Start writing\u2026"
                        placeholderTextColor: Theme.textFaint
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontBody
                        leftPadding: Theme.spaceM
                        rightPadding: Theme.spaceM
                        topPadding: Theme.spaceM
                        // Keeps the last line above the keyboard edge.
                        bottomPadding: Theme.spaceXL
                        selectByMouse: true
                        inputMethodHints: Qt.ImhNone
                        onTextChanged: win.draftChanged()
                    }

                    ScrollBar.vertical: ScrollBar {
                        id: bodyBar
                        policy: ScrollBar.AsNeeded
                        width: Theme.spaceS
                        background: null
                        contentItem: Rectangle {
                            implicitWidth: Theme.spaceXS
                            implicitHeight: Theme.spaceXS
                            radius: Theme.spaceXS / 2
                            color: Qt.alpha(Theme.text, bodyBar.pressed ? 0.45 : 0.25)
                        }
                    }
                }
            }
        }
    }

    // ---- delete confirmation -----------------------------------------------------
    Item {
        id: deleteOverlay
        anchors.fill: parent
        visible: win.deletingId !== ""
        z: 100

        Rectangle {
            anchors.fill: parent
            color: Theme.scrim
            TapHandler { onTapped: win.cancelDelete() }
        }

        Card {
            id: deleteCard
            anchors.centerIn: parent
            width: parent.width - Theme.spaceXL * 2
            padding: Theme.spaceL

            // Swallow taps on the dialog so only the scrim (outside) cancels.
            TapHandler { }

            ColumnLayout {
                width: parent.width
                spacing: Theme.spaceM

                Text {
                    Layout.fillWidth: true
                    text: "Delete note?"
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontTitle
                    font.weight: Font.DemiBold
                    wrapMode: Text.Wrap
                }
                Text {
                    Layout.fillWidth: true
                    text: win.deleteMessage
                    color: Theme.textDim
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody - 6
                    wrapMode: Text.Wrap
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceS

                    Item { Layout.fillWidth: true }

                    Rectangle {
                        id: cancelDelete
                        implicitWidth: cancelDeleteLabel.implicitWidth + Theme.spaceL
                        implicitHeight: Theme.spaceXL
                        radius: height / 2
                        color: Theme.surfaceHigh
                        border.width: 1
                        border.color: Qt.alpha(Theme.text, 0.10)

                        Text {
                            id: cancelDeleteLabel
                            anchors.centerIn: parent
                            text: "Cancel"
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontBody - 4
                        }
                        TapHandler { onTapped: win.cancelDelete() }
                    }

                    Rectangle {
                        id: confirmDeleteButton
                        implicitWidth: confirmDeleteLabel.implicitWidth + Theme.spaceL
                        implicitHeight: Theme.spaceXL
                        radius: height / 2
                        color: Theme.danger

                        Text {
                            id: confirmDeleteLabel
                            anchors.centerIn: parent
                            text: "Delete"
                            color: Theme.primaryContent
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontBody - 4
                            font.weight: Font.Medium
                        }
                        TapHandler { onTapped: win.confirmDelete() }
                    }
                }
            }
        }
    }
}
