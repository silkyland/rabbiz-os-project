import QtQuick
import Rabbiz.Theme
import Rabbiz.Widgets

// Icon + one sentence (+ optional action) instead of a blank screen.
Column {
    id: root
    property string icon: "inbox"
    property string title: ""
    property string hint: ""
    spacing: Theme.spaceS
    width: parent ? parent.width : 0

    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        width: 88
        height: 88
        radius: 44
        color: Theme.surfaceHigh
        border.width: 1
        border.color: Qt.alpha(Theme.text, 0.06)

        MaterialIcon {
            anchors.centerIn: parent
            name: root.icon
            size: 44
            color: Theme.textDim
        }
    }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        text: root.title
        color: Theme.text
        font.pixelSize: Theme.fontBody
        font.family: Theme.fontFamily
        font.weight: Font.Medium
        horizontalAlignment: Text.AlignHCenter
        width: parent.width
        wrapMode: Text.Wrap
    }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        visible: root.hint.length > 0
        text: root.hint
        color: Theme.textDim
        font.pixelSize: Theme.fontCaption
        font.family: Theme.fontFamily
        horizontalAlignment: Text.AlignHCenter
        width: parent.width
        wrapMode: Text.Wrap
    }
}
