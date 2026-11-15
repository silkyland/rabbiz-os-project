import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Services.Notifications
import A21s.Theme

// One notification card for the shade list (NotificationCenter.qml).
// The ListView injects `modelData` (the Notification object).
Rectangle {
    id: card

    required property Notification modelData
    // NotificationServer facade that delivered this notification, used for the
    // "x ago" label. `NotificationServer` here is the local a21s facade
    // (shell/components/NotificationServer.qml), not the Quickshell type.
    property NotificationServer server: null

    readonly property int smallFont: Math.round(Theme.fontStatus * 0.7)

    implicitHeight: layout.implicitHeight + Theme.spaceS * 2
    radius: Theme.radiusM
    color: Theme.card
    border.width: 1
    border.color: Theme.separator

    // appIcon may be an icon-theme name, an absolute path, or a URI.
    function iconSource() {
        var icon = card.modelData ? card.modelData.appIcon : "";
        return resolveImage(icon);
    }

    // notification.image is usually a file path or URI (e.g. IM avatar).
    function imageSource() {
        var img = card.modelData ? card.modelData.image : "";
        return resolveImage(img);
    }

    function resolveImage(ref) {
        if (!ref || ref.length === 0)
            return "";
        if (ref.indexOf("://") >= 0)
            return ref;                          // file://, image://, qrc://
        if (ref.indexOf("/") === 0)
            return "file://" + ref;              // absolute path
        var resolved = Quickshell.iconPath(ref); // icon-theme name
        if (resolved.length === 0)
            return "";
        if (resolved.indexOf("://") < 0)
            resolved = "file://" + resolved;
        return resolved;
    }

    RowLayout {
        id: layout
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
            margins: Theme.spaceS
            // Leave room for the dismiss corner.
            rightMargin: Theme.spaceS + Theme.iconS
        }
        spacing: Theme.spaceS

        // App icon well (letter fallback while/instead of an icon).
        Rectangle {
            Layout.preferredWidth: Theme.iconS
            Layout.preferredHeight: Theme.iconS
            Layout.alignment: Qt.AlignTop
            radius: Theme.radiusS
            color: Theme.surfaceHigh
            clip: true

            Text {
                anchors.centerIn: parent
                text: ((card.modelData && card.modelData.appName.length > 0)
                       ? card.modelData.appName.charAt(0) : "?").toUpperCase()
                color: Theme.textDim
                font.pixelSize: Theme.fontStatus
                font.weight: Font.DemiBold
            }
            Image {
                anchors.fill: parent
                source: card.iconSource()
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                visible: source.toString().length > 0 && status !== Image.Error
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Math.round(Theme.spaceXS / 2)

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceS

                Text {
                    Layout.fillWidth: true
                    text: card.modelData && card.modelData.appName.length > 0
                          ? card.modelData.appName : qsTr("System")
                    color: Theme.textDim
                    font.pixelSize: card.smallFont
                    elide: Text.ElideRight
                }
                Text {
                    visible: text.length > 0
                    text: card.server ? card.server.timeAgo(card.modelData) : ""
                    color: Theme.textFaint
                    font.pixelSize: card.smallFont
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceS

                Text {
                    Layout.fillWidth: true
                    text: card.modelData ? card.modelData.summary : ""
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontStatus
                    font.weight: Font.DemiBold
                    wrapMode: Text.Wrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                }
                // Optional big image (e.g. avatar) — only takes space when it
                // actually loaded.
                Rectangle {
                    visible: thumb.status === Image.Ready
                    Layout.preferredWidth: Theme.iconM
                    Layout.preferredHeight: Theme.iconM
                    Layout.alignment: Qt.AlignTop
                    radius: Theme.radiusS
                    color: Theme.surfaceHigh
                    clip: true

                    Image {
                        id: thumb
                        anchors.fill: parent
                        source: card.imageSource()
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                    }
                }
            }

            Text {
                Layout.fillWidth: true
                visible: text.length > 0
                text: card.modelData ? card.modelData.body : ""
                color: Theme.textDim
                font.pixelSize: Math.round(Theme.fontStatus * 0.75)
                wrapMode: Text.Wrap
                maximumLineCount: 4
                elide: Text.ElideRight
                textFormat: Text.PlainText
            }

            // Action chips (e.g. "Reply", "Open").
            RowLayout {
                Layout.fillWidth: true
                visible: actionRepeater.count > 0
                spacing: Theme.spaceS

                Repeater {
                    id: actionRepeater
                    model: card.modelData ? card.modelData.actions : []

                    delegate: Rectangle {
                        required property NotificationAction modelData

                        implicitWidth: actionText.implicitWidth + Theme.spaceM
                        implicitHeight: actionText.implicitHeight + Theme.spaceXS * 2
                        radius: Theme.radiusPill
                        color: actionArea.pressed ? Theme.surfaceHigh : Theme.tileOff

                        Text {
                            id: actionText
                            anchors.centerIn: parent
                            text: modelData.text
                            color: Theme.primary
                            font.pixelSize: card.smallFont
                        }
                        MouseArea {
                            id: actionArea
                            anchors.fill: parent
                            onClicked: modelData.invoke()
                        }
                    }
                }
            }
        }
    }

    // Dismiss corner (top-right). Notification.dismiss() removes the card from
    // the server's tracked model and hints "closed by user" to the app.
    Rectangle {
        anchors {
            top: parent.top
            right: parent.right
            margins: Theme.spaceXS
        }
        width: Math.round(Theme.iconS * 0.8)
        height: width
        radius: Theme.radiusPill
        color: dismissArea.pressed ? Theme.surfaceHigh : "transparent"

        Text {
            anchors.centerIn: parent
            text: "×"
            color: Theme.textDim
            font.pixelSize: Theme.fontStatus
        }
        MouseArea {
            id: dismissArea
            anchors.fill: parent
            onClicked: if (card.modelData) card.modelData.dismiss()
        }
    }
}
