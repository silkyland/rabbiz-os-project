import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import A21s.Theme
import A21s.Services
import A21s.Widgets as Components
import A21s.Windowing as Windowing

// Standalone Calculator app — launched by `a21s-calculator`.
ApplicationWindow {
    id: win
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    title: "Calculator"
    color: Theme.bg
    font.family: Theme.fontFamily

    property string expression: ""
    property string displayValue: "0"
    property bool justEvaluated: false

    function appendToken(tok) {
        if (justEvaluated && /[0-9.]/.test(tok)) {
            expression = ""
            displayValue = ""
        }
        justEvaluated = false

        if (displayValue === "0" && /[0-9]/.test(tok))
            displayValue = tok
        else if (displayValue === "Error")
            displayValue = tok
        else
            displayValue += tok
    }

    function clearAll() {
        expression = ""
        displayValue = "0"
        justEvaluated = false
    }

    function backspace() {
        if (justEvaluated || displayValue === "Error") {
            clearAll()
            return
        }
        if (displayValue.length <= 1)
            displayValue = "0"
        else
            displayValue = displayValue.slice(0, -1)
    }

    function evaluate() {
        var sanitized = displayValue
            .replace(/×/g, "*")
            .replace(/÷/g, "/")
            .replace(/−/g, "-")
        if (!/^[0-9+\-*/.%()\s]+$/.test(sanitized)) {
            displayValue = "Error"
            return
        }
        try {
            var res = Function("\"use strict\"; return (" + sanitized + ")")()
            if (typeof res !== "number" || !isFinite(res)) {
                displayValue = "Error"
            } else {
                expression = displayValue + " ="
                displayValue = String(Math.round(res * 1e10) / 1e10)
            }
            justEvaluated = true
        } catch (e) {
            displayValue = "Error"
            justEvaluated = true
        }
    }

    ColumnLayout {
        anchors {
            fill: parent
            margins: Theme.spaceM
            bottomMargin: Theme.navZoneHeight + Theme.spaceM
        }
        spacing: Theme.spaceM

        // Top bar
        RowLayout {
            Layout.fillWidth: true
            Components.MaterialIcon {
                name: "calculate"
                size: Theme.iconS
                color: ShellPrefs.accentColor
            }
            Text {
                text: "Calculator"
                color: Theme.text
                font.pixelSize: Theme.fontTitle
                font.weight: Font.Medium
            }
            Item { Layout.fillWidth: true }
        }

        // Display card
        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 260
            radius: Theme.radiusL
            color: Theme.surface
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.08)

            ColumnLayout {
                anchors {
                    fill: parent
                    margins: Theme.spaceL
                }
                spacing: Theme.spaceS

                Item { Layout.fillHeight: true }

                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignRight
                    text: win.expression
                    color: Theme.textDim
                    font.pixelSize: Theme.fontTitle
                    elide: Text.ElideLeft
                }

                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignRight
                    text: win.displayValue
                    color: Theme.text
                    font.pixelSize: 88
                    font.weight: Font.Light
                    minimumPixelSize: 40
                    fontSizeMode: Text.HorizontalFit
                    maximumLineCount: 1
                }
            }
        }

        // Keypad grid
        GridLayout {
            Layout.fillWidth: true
            columns: 4
            columnSpacing: Theme.spaceS
            rowSpacing: Theme.spaceS

            Repeater {
                model: [
                    { label: "C",  kind: "fn" },
                    { label: "⌫",  kind: "fn" },
                    { label: "%",  kind: "op" },
                    { label: "÷",  kind: "op" },
                    { label: "7",  kind: "num" },
                    { label: "8",  kind: "num" },
                    { label: "9",  kind: "num" },
                    { label: "×",  kind: "op" },
                    { label: "4",  kind: "num" },
                    { label: "5",  kind: "num" },
                    { label: "6",  kind: "num" },
                    { label: "−",  kind: "op" },
                    { label: "1",  kind: "num" },
                    { label: "2",  kind: "num" },
                    { label: "3",  kind: "num" },
                    { label: "+",  kind: "op" },
                    { label: "(",  kind: "num" },
                    { label: "0",  kind: "num" },
                    { label: ".",  kind: "num" },
                    { label: "=",  kind: "eq" }
                ]

                delegate: Rectangle {
                    required property var modelData
                    Layout.fillWidth: true
                    height: 124
                    radius: Theme.radiusL + 8
                    color: modelData.kind === "eq" ? ShellPrefs.accentColor
                         : modelData.kind === "op" ? Qt.alpha(ShellPrefs.accentColor, 0.20)
                         : modelData.kind === "fn" ? Theme.surfaceHigh
                         : Theme.card
                    border.width: 1
                    border.color: Qt.alpha(Theme.text, 0.08)

                    scale: keyTap.pressed ? 0.92 : 1.0
                    Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

                    Text {
                        anchors.centerIn: parent
                        text: modelData.label
                        color: modelData.kind === "eq" ? Theme.primaryContent
                             : modelData.kind === "op" ? ShellPrefs.accentColor
                             : modelData.kind === "fn" ? Theme.danger
                             : Theme.text
                        font.pixelSize: Theme.fontHeadline
                        font.weight: modelData.kind === "eq" ? Font.DemiBold : Font.Normal
                    }

                    TapHandler {
                        id: keyTap
                        onTapped: {
                            if (modelData.label === "C")
                                win.clearAll()
                            else if (modelData.label === "⌫")
                                win.backspace()
                            else if (modelData.label === "=")
                                win.evaluate()
                            else
                                win.appendToken(modelData.label)
                        }
                    }
                }
            }
        }
    }

    Windowing.AppGestureLayer {
        anchors.fill: parent
        onBackTriggered: win.close()
    }
}
