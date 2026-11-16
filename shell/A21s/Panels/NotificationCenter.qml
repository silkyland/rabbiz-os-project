import QtQuick
import QtQuick.Layouts
import A21s.Theme

// Notification list for the pull-down shade (W6).
//
// This is a plain Item, NOT a window — W3's Shade embeds it inside the shade
// PanelWindow and sizes it via anchors/explicit width+height.
//
// It owns an a21s NotificationServer facade so the list always has the live
// tracked-notification model even if the shade is created late. The underlying
// Quickshell notification server is a per-process singleton, so this sees the
// same notifications StatusBar's facade collects.
Item {
    id: root

    // The facade, exposed so the shade (or a future popup banner owner) can
    // also hook into newNotification / dismissAll.
    property alias notificationServer: notifServer

    NotificationServer { id: notifServer }

    implicitWidth: layout.implicitWidth
    implicitHeight: header.implicitHeight
                    + (notifServer.count > 0
                       ? list.contentHeight
                       : empty.implicitHeight + Theme.spaceM)
                    + Theme.spaceS * 2

    ColumnLayout {
        id: layout
        anchors.fill: parent
        spacing: Theme.spaceS

        // Header row: title + "Clear all".
        RowLayout {
            id: header
            Layout.fillWidth: true
            spacing: Theme.spaceS

            Text {
                Layout.fillWidth: true
                text: qsTr("Notifications")
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontCaption
                font.weight: Font.DemiBold
            }
            Rectangle {
                visible: notifServer.count > 0
                implicitWidth: clearText.implicitWidth + Theme.spaceM
                implicitHeight: clearText.implicitHeight + Theme.spaceXS * 2
                radius: Theme.radiusPill
                color: clearArea.pressed ? Theme.surfaceHigh : Theme.tileOff

                Text {
                    id: clearText
                    anchors.centerIn: parent
                    text: qsTr("Clear all")
                    color: Theme.textDim
                    font.pixelSize: Math.round(Theme.fontStatus * 0.7)
                }
                MouseArea {
                    id: clearArea
                    anchors.fill: parent
                    onClicked: notifServer.dismissAll()
                }
            }
        }

        // List region — fills whatever the shade gives us.
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            implicitHeight: list.contentHeight

            ListView {
                id: list
                anchors.fill: parent
                visible: notifServer.count > 0
                clip: true
                spacing: Theme.spaceS
                // The server exposes an ObjectModel — a real QML model whose
                // single role is `modelData` (the Notification object).
                model: notifServer.trackedNotifications

                delegate: NotificationItem {
                    width: ListView.view.width
                    height: implicitHeight
                    server: notifServer
                }
            }

            Text {
                id: empty
                anchors.centerIn: parent
                visible: notifServer.count === 0
                text: qsTr("No notifications")
                color: Theme.textFaint
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontCaption
            }
        }
    }
}
