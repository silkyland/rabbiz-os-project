import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Rabbiz.Theme
import Rabbiz.Services
import "Calc.js" as Calc
import RabbizKit

// Calculator — in-house replacement for kalk (docs/BASE-APPS.md).
// Tone reference for the other base apps: shared kit, Theme tokens only, no literals for
// colors except the keypad glyph strings, history persisted under ~/.local/share.
ApplicationWindow {
    id: win
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    title: "Calculator"
    color: Theme.bg
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontBody

    Binding { target: Theme; property: "light"; value: ShellPrefs.lightTheme }

    property string expression: ""
    property string result: "0"
    property bool showHistory: false
    property var history: []

    readonly property string displayExpr: win.expression.length > 0 ? win.expression : "0"
    readonly property var live: Calc.evaluate(win.normalize(win.expression))
    // Empty expression shows "0"; a valid expression shows its live result; an incomplete
    // one shows what was typed (not an error), and only a failed "=" turns red.
    readonly property string mainText: win.expression.length === 0 ? "0"
        : win.live.ok ? Calc.format(win.live.value)
        : win.result === "Error" ? "Error" : win.expression
    readonly property color mainColor: win.result === "Error" && !win.live.ok ? Theme.danger : Theme.text

    function normalize(e) {
        return String(e).replace(/×/g, "*").replace(/÷/g, "/")
                        .replace(/−/g, "-").replace(/,/g, "")
    }

    function press(key) {
        if (key === "C") { win.expression = ""; win.result = "0"; return }
        if (key === "⌫") { win.expression = win.expression.slice(0, -1); return }
        if (key === "=") { win.equals(); return }
        if (key === "±") {
            if (win.expression.length === 0) { win.expression = "-"; return }
            if (win.expression.startsWith("-")) win.expression = win.expression.slice(1)
            else win.expression = "-" + win.expression
            return
        }
        win.expression += key
    }

    function equals() {
        var r = Calc.evaluate(win.normalize(win.expression))
        if (!r.ok) { win.result = "Error"; return }
        var text = Calc.format(r.value)
        win.history.unshift({ expr: win.expression, result: text })
        if (win.history.length > 20) win.history = win.history.slice(0, 20)
        win.saveHistory()
        win.result = text
        win.expression = text
    }

    function saveHistory() {
        saveProc.command = ["sh", "-c",
            "mkdir -p \"$(dirname \"$1\")\" && printf '%s' \"$2\" > \"$1.tmp\" && mv \"$1.tmp\" \"$1\"",
            "_", win.historyPath, JSON.stringify(win.history)]
        saveProc.running = true
    }

    readonly property string historyPath: (Quickshell.env("HOME") || "/home/rabbiz")
        + "/.local/share/rabbiz-calculator/history.json"

    Process { id: saveProc }
    FileView {
        id: historyFile
        path: win.historyPath
        onLoaded: {
            try { win.history = JSON.parse(text()) || [] } catch (e) { win.history = [] }
        }
        Component.onCompleted: historyFile.reload()
    }

    ColumnLayout {
        anchors {
            fill: parent
            topMargin: Theme.spaceM
            leftMargin: Theme.spaceM
            rightMargin: Theme.spaceM
            bottomMargin: Theme.spaceL + Theme.spaceS
        }
        spacing: Theme.spaceM

        AppHeader {
            Layout.fillWidth: true
            title: "Calculator"
            subtitle: "Tap keys, or type on a keyboard"
            IconButton {
                icon: win.showHistory ? "calculate" : "history"
                filled: win.showHistory
                onClicked: win.showHistory = !win.showHistory
            }
        }

        // ---- display ----------------------------------------------------
        Card {
            Layout.fillWidth: true
            padding: Theme.spaceL
            implicitHeight: 192

            Column {
                width: parent.width
                spacing: Theme.spaceS
                Flickable {
                    id: exprFlick
                    width: parent.width
                    height: exprText.implicitHeight
                    contentWidth: Math.max(width, exprText.implicitWidth)
                    contentHeight: height
                    clip: true
                    flickableDirection: Flickable.HorizontalFlick
                    boundsBehavior: Flickable.StopAtBounds

                    function scrollToEnd() {
                        contentX = Math.max(0, contentWidth - width)
                    }
                    onContentWidthChanged: scrollToEnd()

                    Text {
                        id: exprText
                        width: Math.max(exprFlick.width, implicitWidth)
                        horizontalAlignment: Text.AlignRight
                        text: win.displayExpr
                        color: Theme.textDim
                        font.pixelSize: Theme.fontBody
                        font.family: Theme.fontFamily
                    }
                }
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignRight
                    text: win.mainText
                    color: win.mainColor
                    font.pixelSize: 68
                    fontSizeMode: Text.HorizontalFit
                    minimumPixelSize: 36
                    font.family: Theme.fontFamily
                    font.weight: Font.Light
                    elide: Text.ElideLeft
                    Behavior on color { ColorAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }
                }
            }
        }

        // ---- history (collapsed by default) -----------------------------
        Rectangle {
            visible: win.showHistory
            Layout.fillWidth: true
            Layout.fillHeight: true
            radius: Theme.radiusL
            color: Theme.card
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.08)

            ColumnLayout {
                anchors { fill: parent; margins: Theme.spaceS }
                spacing: 2

                RowLayout {
                    Layout.fillWidth: true
                    SectionLabel { Layout.fillWidth: true; text: "HISTORY" }
                    Rectangle {
                        height: 48
                        width: clearLabel.implicitWidth + Theme.spaceM * 2
                        radius: height / 2
                        color: "transparent"
                        scale: clearTap.pressed ? 0.94 : 1.0
                        Behavior on scale { SpringAnimation { spring: 6.5; damping: 0.48 } }

                        Rectangle {
                            anchors.fill: parent
                            radius: parent.radius
                            color: ShellPrefs.accentColor
                            opacity: clearTap.pressed ? 0.14 : 0.0
                            Behavior on opacity { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }
                        }
                        Text {
                            id: clearLabel
                            anchors.centerIn: parent
                            text: "Clear"
                            color: ShellPrefs.accentColor
                            font.pixelSize: Theme.fontCaption
                            font.family: Theme.fontFamily
                            font.weight: Font.Medium
                        }
                        TapHandler {
                            id: clearTap
                            onTapped: { win.history = []; win.saveHistory() }
                        }
                    }
                }

                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: win.history
                    spacing: 2
                    delegate: RowItem {
                        required property var modelData
                        width: ListView.view.width
                        title: modelData.result
                        caption: modelData.expr
                        showChevron: false
                        onClicked: { win.expression = modelData.result; win.showHistory = false }
                    }
                    EmptyState {
                        visible: win.history.length === 0
                        anchors.centerIn: parent
                        icon: "history"
                        title: "No calculations yet"
                        hint: "Results you compute are kept here"
                    }
                }
            }
        }

        // ---- keypad -----------------------------------------------------
        GridLayout {
            visible: !win.showHistory
            Layout.fillWidth: true
            Layout.fillHeight: true
            columns: 4
            rowSpacing: Theme.spaceS
            columnSpacing: Theme.spaceS

            Repeater {
                model: [
                    { k: "C", kind: "action" }, { k: "⌫", kind: "action" },
                    { k: "%", kind: "op" }, { k: "÷", kind: "op" },
                    { k: "7", kind: "num" }, { k: "8", kind: "num" }, { k: "9", kind: "num" }, { k: "×", kind: "op" },
                    { k: "4", kind: "num" }, { k: "5", kind: "num" }, { k: "6", kind: "num" }, { k: "−", kind: "op" },
                    { k: "1", kind: "num" }, { k: "2", kind: "num" }, { k: "3", kind: "num" }, { k: "+", kind: "op" },
                    { k: "±", kind: "action" }, { k: "0", kind: "num" }, { k: ".", kind: "num" }, { k: "=", kind: "equals" }
                ]
                delegate: Rectangle {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.preferredHeight: 84
                    radius: modelData.kind === "equals" ? Theme.radiusL : Theme.radiusM
                    color: modelData.kind === "equals" ? ShellPrefs.accentColor
                         : modelData.kind === "op" ? Qt.alpha(ShellPrefs.accentColor, 0.18)
                         : modelData.kind === "action" ? Theme.surfaceHigh
                         : Theme.card
                    border.width: modelData.kind === "equals" ? 0 : 1
                    border.color: Qt.alpha(Theme.text, 0.07)
                    scale: keyTap.pressed ? 0.93 : 1.0
                    Behavior on scale { SpringAnimation { spring: 7.0; damping: 0.48 } }

                    Rectangle {
                        anchors.fill: parent
                        radius: parent.radius
                        color: modelData.kind === "equals" ? ShellPrefs.onAccentColor : Theme.text
                        opacity: keyTap.pressed ? 0.14 : 0.0
                        Behavior on opacity { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }
                    }

                    Text {
                        anchors.centerIn: parent
                        text: modelData.k
                        color: modelData.kind === "equals" ? ShellPrefs.onAccentColor
                             : modelData.kind === "op" ? ShellPrefs.accentColor
                             : Theme.text
                        font.pixelSize: modelData.kind === "equals" ? 34 : 30
                        font.family: Theme.fontFamily
                        font.weight: modelData.kind === "num" ? Font.Normal : Font.Medium
                    }
                    TapHandler { id: keyTap; onTapped: win.press(modelData.k) }
                }
            }
        }
    }
}
