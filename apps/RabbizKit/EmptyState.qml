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

    MaterialIcon {
        anchors.horizontalCenter: parent.horizontalCenter
        name: root.icon
        size: 56
        color: Theme.textFaint
    }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        text: root.title
        color: Theme.textDim
        font.pixelSize: Theme.fontBody
        font.family: Theme.fontFamily
        horizontalAlignment: Text.AlignHCenter
        width: parent.width
    }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        visible: root.hint.length > 0
        text: root.hint
        color: Theme.textFaint
        font.pixelSize: Theme.fontCaption
        font.family: Theme.fontFamily
        horizontalAlignment: Text.AlignHCenter
        width: parent.width
        wrapMode: Text.Wrap
    }
}
