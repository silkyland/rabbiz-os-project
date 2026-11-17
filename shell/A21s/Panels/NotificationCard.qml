import QtQuick
import QtQuick.Layouts
import Quickshell
import A21s.Theme
import A21s.Widgets

// One grouped notification card for the shade sheet (NotificationCenter.qml).
//
// `modelData` is a card row from NotificationGroups.js: every notification of
// one app collapsed into a single card, newest first. A single notification
// renders as a plain card (summary + body + actions); a group renders its app
// name, a count and one preview row — tap to expand, and the × dismisses the
// whole group the way Android does.
Rectangle {
    id: card

    required property var modelData
    // Facade that delivered the notifications — used for the "x ago" label.
    property NotificationServer server: null
    // Bumped by the sheet every minute so the relative time re-evaluates.
    property int refreshToken: 0
    // Lock-screen preview: action chips can launch a window, so they stay behind
    // the unlock. Dismissing and expanding a group remain available.
    property bool actionsEnabled: true

    readonly property var items: modelData && modelData.items ? modelData.items : []
    readonly property int count: items.length
    readonly property var newest: count > 0 ? items[0] : null
    readonly property bool grouped: count > 1
    readonly property int previewRows: grouped ? (expanded ? Math.min(count, 4) : 1) : 1
    property bool expanded: false

    // appIcon may be an icon-theme name, an absolute path, or a URI.
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

    function dismissGroup() {
        for (var i = 0; i < card.items.length; ++i) {
            if (card.items[i])
                card.items[i].dismiss();
        }
    }

    function timeLabel() {
        if (!card.server || !card.newest)
            return "";
        // refreshToken is read so the label re-evaluates every minute.
        return card.refreshToken >= 0 ? card.server.timeAgo(card.newest) : "";
    }

    implicitHeight: content.implicitHeight + Theme.spaceM * 2
    radius: Theme.radiusCard
    color: Theme.shadeCard
    border.width: 1
    border.color: Theme.shadeLine
    opacity: modelData && modelData.dim ? 0.66 : 1.0

    TapHandler {
        enabled: card.grouped
        onTapped: card.expanded = !card.expanded
    }

    RowLayout {
        id: content
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            margins: Theme.spaceM
            // Room for the dismiss corner.
            rightMargin: Theme.spaceM + Theme.iconS
        }
        spacing: Theme.spaceS

        // App icon well (letter fallback while/instead of an icon).
        Rectangle {
            Layout.alignment: Qt.AlignTop
            Layout.preferredWidth: Theme.iconM + Theme.iconS
            Layout.preferredHeight: Theme.iconM + Theme.iconS
            radius: Theme.radiusM + 6
            color: Theme.shadeWell
            clip: true

            Text {
                anchors.centerIn: parent
                text: (card.newest && card.newest.appName.length > 0
                       ? card.newest.appName.charAt(0) : "?").toUpperCase()
                color: Theme.textDim
                font.pixelSize: Theme.fontTitle
                font.family: Theme.fontFamily
                font.weight: Font.DemiBold
            }
            Image {
                anchors.fill: parent
                source: card.newest ? card.resolveImage(card.newest.appIcon) : ""
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                visible: source.toString().length > 0 && status !== Image.Error
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            spacing: Theme.spaceXS

            // Head: title (app + count for groups, summary for a single one),
            // relative time, expand chevron.
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceXS

                Text {
                    Layout.fillWidth: true
                    text: card.grouped
                        ? (card.modelData.appName + " · " + card.count)
                        : (card.newest ? card.newest.summary : "")
                    color: Theme.text
                    font.pixelSize: Theme.fontCaption + 2
                    font.family: Theme.fontFamily
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                    maximumLineCount: 1
                }

                Text {
                    Layout.alignment: Qt.AlignVCenter
                    text: card.timeLabel()
                    color: Theme.textDim
                    font.pixelSize: Theme.fontMicro
                    font.family: Theme.fontFamily
                }

                MaterialIcon {
                    Layout.alignment: Qt.AlignVCenter
                    visible: card.grouped
                    name: "expand_more"
                    size: Theme.fontBody - 4
                    color: Theme.textFaint
                    rotation: card.expanded ? 180 : 0
                    Behavior on rotation {
                        NumberAnimation { duration: Theme.animFast; easing.type: Easing.OutCubic }
                    }
                }
            }

            // Rows: one preview row when collapsed, up to four when expanded.
            // A single notification renders its body instead.
            Repeater {
                model: card.previewRows

                delegate: ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0

                    Rectangle {
                        visible: index > 0 && card.grouped
                        Layout.fillWidth: true
                        Layout.topMargin: Theme.spaceXS
                        Layout.bottomMargin: Theme.spaceXS
                        implicitHeight: 1
                        color: Theme.shadeLine
                    }

                    Text {
                        visible: card.grouped
                        Layout.fillWidth: true
                        text: card.items[index] ? card.items[index].summary : ""
                        color: Theme.text
                        font.pixelSize: Theme.fontStatus + 1
                        font.family: Theme.fontFamily
                        font.weight: Font.Medium
                        elide: Text.ElideRight
                        maximumLineCount: 1
                    }

                    Text {
                        Layout.fillWidth: true
                        text: card.items[index] ? card.items[index].body : ""
                        visible: text.length > 0
                        color: card.grouped ? Theme.textFaint : Theme.textDim
                        font.pixelSize: card.grouped ? Theme.fontStatus : Theme.fontStatus + 1
                        font.family: Theme.fontFamily
                        wrapMode: Text.Wrap
                        maximumLineCount: card.grouped ? 1 : 2
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                    }
                }
            }

            // Action chips (e.g. "Reply", "Open") — single notifications only,
            // a collapsed group must stay one glanceable line.
            RowLayout {
                Layout.fillWidth: true
                visible: card.actionsEnabled && !card.grouped && actionRepeater.count > 0
                spacing: Theme.spaceS

                Repeater {
                    id: actionRepeater
                    model: card.newest ? card.newest.actions : []

                    delegate: Rectangle {
                        required property var modelData

                        implicitWidth: actionText.implicitWidth + Theme.spaceM
                        implicitHeight: actionText.implicitHeight + Theme.spaceXS * 2
                        radius: Theme.radiusPill
                        color: actionArea.pressed ? Theme.shadeCardOn : Theme.shadeWell

                        Text {
                            id: actionText
                            anchors.centerIn: parent
                            text: modelData.text
                            color: Theme.text
                            font.pixelSize: Theme.fontMicro + 2
                            font.family: Theme.fontFamily
                            font.weight: Font.Medium
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

    // Dismiss corner (top-right). Outside the content margins so the title and
    // the timestamp never sit under it.
    Rectangle {
        anchors {
            top: parent.top
            right: parent.right
            margins: Theme.spaceXS
        }
        width: Theme.iconS
        height: width
        radius: width / 2
        color: dismissArea.pressed ? Theme.shadeCardOn : "transparent"

        MaterialIcon {
            anchors.centerIn: parent
            name: "close"
            size: Theme.fontCaption + 6
            color: Qt.alpha(Theme.text, 0.55)
        }
        MouseArea {
            id: dismissArea
            anchors.fill: parent
            onClicked: card.dismissGroup()
        }
    }
}
