import QtQuick
import QtQuick.Layouts
import Quickshell
import A21s.Theme
import A21s.Services
import A21s.Widgets
import "NotificationGroups.js" as Groups

// Notification sheet of the pull-down shade (One UI 7-style).
//
// A plain Item, NOT a window: Shade.qml embeds it in the shade PanelWindow and
// sizes it via anchors. Composition, top to bottom: big clock header with a
// Do-Not-Disturb button, then the grouped list — one card per app, filed under
// Conversations / Devices / Apps / Promotions (see NotificationGroups.js) — and
// a floating clear-all button.
//
// It owns an a21s NotificationServer facade so the list always has the live
// tracked model even if the shade is created late; the underlying Quickshell
// server is a per-process singleton, so this sees the same notifications the
// StatusBar facade collects.
Item {
    id: root

    property alias notificationServer: notifServer

    NotificationServer { id: notifServer }

    // Bumped by a timer so the "x ago" labels age while the sheet is open.
    property int minuteTick: 0

    readonly property var sections: Groups.build(notifServer.trackedNotifications.values)

    SystemClock {
        id: clock
        precision: SystemClock.Minutes
    }

    Timer {
        interval: 30000
        running: ShellState.shadeOpen && notifServer.count > 0
        repeat: true
        onTriggered: root.minuteTick++
    }

    implicitWidth: layout.implicitWidth
    implicitHeight: layout.implicitHeight

    ColumnLayout {
        id: layout
        anchors.fill: parent
        spacing: Theme.spaceS

        // --- Clock header ------------------------------------------------
        // The one oversized number in the shell; the date sits on its baseline
        // and the bell toggles Do Not Disturb (crossed + danger when on).
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceS

            Text {
                Layout.alignment: Qt.AlignBottom
                text: Qt.formatTime(clock.date, "HH:mm")
                color: Theme.text
                font.pixelSize: Theme.fontClock
                font.family: Theme.fontFamily
                font.weight: Font.Medium
                font.letterSpacing: -2
            }

            Text {
                Layout.alignment: Qt.AlignBottom
                Layout.bottomMargin: Theme.spaceS + 4
                text: Qt.formatDate(clock.date, "ddd d MMM")
                color: Theme.textDim
                font.pixelSize: Theme.fontCaption
                font.family: Theme.fontFamily
            }

            Item { Layout.fillWidth: true }

            Rectangle {
                Layout.alignment: Qt.AlignBottom
                width: Theme.shadeControlButton
                height: width
                radius: width / 2
                color: Theme.shadeCard
                border.width: 1
                border.color: SystemControl.dndEnabled
                    ? Qt.alpha(Theme.danger, 0.55) : Theme.shadeLine

                Behavior on border.color { ColorAnimation { duration: Theme.animFast } }

                MaterialIcon {
                    anchors.centerIn: parent
                    name: SystemControl.dndEnabled ? "notifications_off" : "notifications"
                    size: Theme.iconS + 2
                    color: SystemControl.dndEnabled ? Theme.danger : Theme.text
                }
                TapHandler {
                    onTapped: SystemControl.setDndEnabled(!SystemControl.dndEnabled)
                }
            }
        }

        // --- Grouped list -------------------------------------------------
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Flickable {
                id: scroller
                anchors.fill: parent
                clip: true
                contentWidth: width
                contentHeight: Math.max(sectionsColumn.implicitHeight, height)
                boundsBehavior: Flickable.StopAtBounds
                // Only take over the vertical drag when the sheet actually
                // overflows; otherwise the shade's own pull-to-close keeps it.
                interactive: sectionsColumn.implicitHeight > height

                Column {
                    id: sectionsColumn
                    width: scroller.width
                    spacing: Theme.spaceS

                    Repeater {
                        model: root.sections

                        delegate: Column {
                            id: sectionColumn
                            required property var modelData
                            width: sectionsColumn.width
                            spacing: Theme.spaceS

                            RowLayout {
                                width: parent.width
                                spacing: Theme.spaceXS

                                Text {
                                    text: sectionColumn.modelData.title.toUpperCase()
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontMicro
                                    font.family: Theme.fontFamily
                                    font.letterSpacing: 1.6
                                    font.weight: Font.DemiBold
                                }
                                Text {
                                    text: String(sectionColumn.modelData.count)
                                    color: Theme.textFaint
                                    font.pixelSize: Theme.fontMicro
                                    font.family: Theme.fontFamily
                                }
                                Rectangle {
                                    Layout.fillWidth: true
                                    Layout.alignment: Qt.AlignVCenter
                                    implicitHeight: 1
                                    color: Theme.shadeLine
                                }
                            }

                            Repeater {
                                model: sectionColumn.modelData.cards

                                delegate: NotificationCard {
                                    width: sectionColumn.width
                                    height: implicitHeight
                                    server: notifServer
                                    refreshToken: root.minuteTick
                                }
                            }
                        }
                    }

                    // Scrolls the last card clear of the floating clear-all button.
                    Item {
                        width: 1
                        height: clearFab.height + Theme.spaceM
                    }
                }
            }

            Column {
                anchors.centerIn: parent
                visible: notifServer.count === 0
                spacing: Theme.spaceXS

                MaterialIcon {
                    anchors.horizontalCenter: parent.horizontalCenter
                    name: "notifications_off"
                    size: Theme.iconL
                    color: Theme.textFaint
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: qsTr("No notifications")
                    color: Theme.textDim
                    font.pixelSize: Theme.fontCaption
                    font.family: Theme.fontFamily
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: qsTr("You're all caught up")
                    color: Theme.textFaint
                    font.pixelSize: Theme.fontStatus
                    font.family: Theme.fontFamily
                }
            }
        }
    }

    // --- Clear all ---------------------------------------------------------
    // Floats over the list at the bottom, the way One UI puts "Clear" there.
    Rectangle {
        id: clearFab

        visible: notifServer.count > 0
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: Theme.spaceXS
        }
        width: Theme.iconL + Theme.spaceM + 8
        height: width
        radius: width / 2
        color: Theme.shadeFab
        border.width: 1
        border.color: fabArea.pressed ? Qt.alpha(Theme.text, 0.3) : Theme.shadeLine

        MaterialIcon {
            anchors.centerIn: parent
            name: "delete"
            size: Theme.iconS
            color: Theme.text
        }

        MouseArea {
            id: fabArea
            anchors.fill: parent
            onClicked: notifServer.dismissAll()
        }
    }
}
