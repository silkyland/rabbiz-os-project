import QtQuick
import A21s.Theme

// SMS/MMS bubble. Incoming: left, surfaceHigh. Outgoing: right, primary.
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
        color: bubble.incoming ? Theme.surfaceHigh : Theme.primary
        implicitWidth: Math.min(msg.implicitWidth + Theme.spaceM * 2,
                                bubble.maxBubbleWidth)
        implicitHeight: msg.implicitHeight + Theme.spaceM

        Text {
            id: msg

            anchors {
                fill: parent
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
                topMargin: Theme.spaceS
                bottomMargin: Theme.spaceS
            }
            text: bubble.body
            color: bubble.incoming ? Theme.text : Theme.primaryContent
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontBody
            wrapMode: Text.Wrap
        }
    }

    MouseArea {
        anchors.fill: rect
        onPressAndHold: bubble.longPressed()
    }
}
