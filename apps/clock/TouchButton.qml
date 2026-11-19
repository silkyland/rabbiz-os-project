import QtQuick
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets

// 48 px round icon action. The kit's IconButton is 44 px, just under the phone tap-target
// rule in docs/BASE-APPS.md, so row actions (delete) and the banner dismiss use this.
Rectangle {
    id: root
    property string icon: ""
    property color tint: Theme.text
    property bool filled: false
    signal clicked()

    readonly property int hitSize: Theme.spaceXL      // 48 px: the phone tap-target floor

    width: hitSize
    height: hitSize
    radius: hitSize / 2
    color: root.filled ? Qt.alpha(ShellPrefs.accentColor, 0.18) : Theme.card
    border.width: 1
    border.color: Qt.alpha(Theme.text, 0.10)
    scale: tap.pressed ? 0.90 : 1.0
    Behavior on scale { SpringAnimation { spring: 6.5; damping: 0.48 } }

    Rectangle {
        anchors.fill: parent
        radius: parent.radius
        color: root.tint
        opacity: tap.pressed ? 0.14 : 0.0
        Behavior on opacity { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }
    }

    MaterialIcon {
        anchors.centerIn: parent
        name: root.icon
        size: Theme.fontBody - 4
        color: root.tint
    }
    TapHandler { id: tap; onTapped: root.clicked() }
}
