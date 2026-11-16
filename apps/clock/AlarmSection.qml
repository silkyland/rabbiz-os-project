import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services
import A21sKit
import "ClockLogic.js" as ClockLogic

// Alarm tab. Persistence and the once-a-day ringing tick live in Clock.qml; this file only
// edits the list and hands the complete new array back through `alarmsEdited`, so there is
// exactly one writer for ~/.local/share/a21s-clock/alarms.json.
Item {
    id: root
    property var alarms: []
    property bool adding: false
    property int draftHour: 7
    property int draftMinute: 30

    signal alarmsEdited(var list)

    function commit() {
        var now = new Date()
        var alarm = ClockLogic.makeAlarm(root.draftHour, root.draftMinute, "alarm-" + Date.now())
        // Saving an alarm for the minute that is already running should not ring instantly.
        if (alarm.hour === now.getHours() && alarm.minute === now.getMinutes())
            alarm.firedDay = ClockLogic.dayKey(now)
        root.alarmsEdited(ClockLogic.addAlarm(root.alarms, alarm))
        root.adding = false
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spaceS

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceS

            SectionLabel {
                Layout.fillWidth: true
                text: "ALARMS"
            }
            AccentButton {
                text: root.adding ? "Close" : "New alarm"
                materialIcon: root.adding ? "close" : "add"
                onClicked: root.adding = !root.adding
            }
        }

        // ---- add panel: steppers only, no keyboard ---------------------------
        Card {
            visible: root.adding
            Layout.fillWidth: true
            padding: Theme.spaceM

            ColumnLayout {
                width: parent.width
                spacing: Theme.spaceS

                Text {
                    Layout.fillWidth: true
                    horizontalAlignment: Text.AlignHCenter
                    text: ClockLogic.pad2(root.draftHour) + ":" + ClockLogic.pad2(root.draftMinute)
                    color: ShellPrefs.accentColor
                    font.family: Theme.fontFamily
                    font.pixelSize: Math.round(Theme.fontHeadline * 1.3)
                    font.weight: Font.Light
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceS

                    Stepper {
                        Layout.fillWidth: true
                        label: "HOUR"
                        valueText: ClockLogic.pad2(root.draftHour)
                        onStep: function (delta) {
                            root.draftHour = ClockLogic.stepWrapped(root.draftHour, delta, 0, 23)
                        }
                    }
                    Stepper {
                        Layout.fillWidth: true
                        label: "MINUTE"
                        valueText: ClockLogic.pad2(root.draftMinute)
                        onStep: function (delta) {
                            root.draftMinute = ClockLogic.stepWrapped(root.draftMinute, delta, 0, 59)
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceS

                    SecondaryButton {
                        Layout.fillWidth: true
                        text: "Cancel"
                        onClicked: root.adding = false
                    }
                    AccentButton {
                        Layout.fillWidth: true
                        text: "Save alarm"
                        materialIcon: "check"
                        onClicked: root.commit()
                    }
                }
            }
        }

        // ---- alarm list ------------------------------------------------------
        ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.alarms.length > 0
            clip: true
            spacing: Theme.spaceS
            cacheBuffer: 1200
            model: root.alarms

            delegate: Rectangle {
                required property var modelData
                width: ListView.view.width
                height: 84
                radius: Theme.radiusM
                color: Theme.card
                border.width: 1
                border.color: Qt.alpha(Theme.text, 0.08)
                opacity: modelData.enabled ? 1.0 : 0.62

                RowLayout {
                    anchors { fill: parent; leftMargin: Theme.spaceM; rightMargin: Theme.spaceS }
                    spacing: Theme.spaceS

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2

                        Text {
                            Layout.fillWidth: true
                            text: ClockLogic.pad2(modelData.hour) + ":" + ClockLogic.pad2(modelData.minute)
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontTitle
                            font.weight: Font.Light
                            elide: Text.ElideRight
                        }
                        Text {
                            Layout.fillWidth: true
                            text: modelData.enabled ? "Every day" : "Off"
                            color: Theme.textDim
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontCaption - 4
                            elide: Text.ElideRight
                        }
                    }

                    ToggleSwitch {
                        checked: modelData.enabled
                        onToggled: root.alarmsEdited(
                            ClockLogic.setEnabled(root.alarms, modelData.id, !modelData.enabled))
                    }

                    TouchButton {
                        icon: "delete"
                        onClicked: root.alarmsEdited(
                            ClockLogic.removeAlarm(root.alarms, modelData.id))
                    }
                }
            }
        }

        // ---- empty state (also the first-run screen) -------------------------
        Item {
            visible: !root.adding && root.alarms.length === 0
            Layout.fillWidth: true
            Layout.fillHeight: true

            EmptyState {
                anchors.centerIn: parent
                width: parent.width * 0.82
                icon: "alarm"
                title: "No alarms yet"
                hint: "Tap New alarm and set a time with the steppers. Alarms ring every day while Clock is open."
            }
        }
    }
}
