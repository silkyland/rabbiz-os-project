import QtQuick
import Quickshell
import A21s.Theme

// Reusable launcher tile: rounded-square icon + label, like a phone home/drawer
// app icon. `entry` is normally a Quickshell DesktopEntry, but any object with
// `name`/`icon` fields works (used for placeholder entries). Entries with an
// unresolvable icon fall back to a first-letter tile.
Item {
    id: appIcon
    required property var entry
    property bool labelVisible: true
    signal launched()

    readonly property string entryName: (entry && entry.name) ? String(entry.name) : ""
    readonly property url iconUrl: (entry && entry.icon) ? Quickshell.iconPath(String(entry.icon)) : ""

    implicitWidth: Theme.iconApp + Theme.spaceL
    implicitHeight: Theme.iconApp + (labelVisible ? Theme.fontCaption * 2 : 0) + Theme.spaceS

    Rectangle {
        id: tile
        anchors {
            top: parent.top
            horizontalCenter: parent.horizontalCenter
        }
        width: Theme.iconApp
        height: Theme.iconApp
        radius: Theme.radiusM
        color: Theme.card

        Image {
            id: image
            anchors.centerIn: parent
            width: parent.width - Theme.spaceM * 2
            height: parent.height - Theme.spaceM * 2
            source: appIcon.iconUrl
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            smooth: true
            visible: status === Image.Ready
        }

        Text {
            anchors.centerIn: parent
            visible: image.status !== Image.Ready
            text: appIcon.entryName.length > 0 ? appIcon.entryName.charAt(0).toUpperCase() : "?"
            color: Theme.primary
            font.pixelSize: Theme.fontTitle
            font.family: Theme.fontFamily
            font.bold: true
        }

        scale: tap.pressed ? 0.92 : 1.0
        Behavior on scale {
            NumberAnimation {
                duration: Theme.animFast
                easing.type: Theme.animEase
            }
        }
    }

    Text {
        id: label
        anchors {
            top: tile.bottom
            topMargin: Theme.spaceXS
            left: parent.left
            right: parent.right
        }
        visible: appIcon.labelVisible
        text: appIcon.entryName
        color: Theme.text
        font.pixelSize: Theme.fontCaption
        font.family: Theme.fontFamily
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        wrapMode: Text.WordWrap
        maximumLineCount: 2
    }

    TapHandler {
        id: tap
        onTapped: {
            if (appIcon.entry && typeof appIcon.entry.execute === "function")
                appIcon.entry.execute();
            appIcon.launched();
        }
    }
}
