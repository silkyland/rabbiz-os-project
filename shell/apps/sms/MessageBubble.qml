import QtQuick
import Rabbiz.Theme
import Rabbiz.Services

// SMS/MMS bubble. Incoming: left, Theme.card. Outgoing: right, ShellPrefs.accentColor.
Item {
    id: bubble

    property string body: ""
    property bool incoming: true
    // Cap so long messages wrap instead of spanning the screen.
    property int maxBubbleWidth: 480

    signal longPressed()

    implicitWidth: rect.implicitWidth
    implicitHeight: rect.implicitHeight

    Rectangle {
        id: rect

        anchors.left: bubble.incoming ? parent.left : undefined
        anchors.right: bubble.incoming ? undefined : parent.right
        radius: Theme.radiusL
        color: bubble.incoming ? Theme.card : ShellPrefs.accentColor
        border.width: bubble.incoming ? 1 : 0
        border.color: Qt.alpha(Theme.text, 0.08)
        implicitWidth: Math.min(msg.implicitWidth + Theme.spaceM * 2,
                                bubble.maxBubbleWidth)
        implicitHeight: msg.implicitHeight + Theme.spaceM + 4
        scale: bubbleMouse.pressed ? 0.975 : 1.0
        Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

        // MD3 state-layer press feedback
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: bubble.incoming ? Theme.text : ShellPrefs.onAccentColor
            opacity: bubbleMouse.pressed ? 0.10 : 0.0
            Behavior on opacity { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }
        }

        Text {
            id: msg

            anchors {
                fill: parent
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
                topMargin: Theme.spaceS + 2
                bottomMargin: Theme.spaceS + 2
            }
            text: bubble.body
            color: bubble.incoming ? Theme.text : ShellPrefs.onAccentColor
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontBody
            wrapMode: Text.Wrap
        }
    }

    MouseArea {
        id: bubbleMouse
        anchors.fill: rect
        onPressAndHold: bubble.longPressed()
    }
}
