import QtQuick
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets

// 48 px circular icon action, used in app headers and list rows.
Rectangle {
    id: root
    property string icon: ""
    property bool filled: false
    property color tint: Theme.text
    signal clicked()

    implicitWidth: 48
    implicitHeight: 48
    width: implicitWidth
    height: implicitHeight
    radius: height / 2
    color: filled ? Qt.alpha(ShellPrefs.accentColor, 0.18) : Theme.surfaceHigh
    border.width: filled ? 0 : 1
    border.color: Qt.alpha(Theme.text, 0.10)
    scale: tap.pressed ? 0.90 : 1.0
    Behavior on scale { SpringAnimation { spring: 6.5; damping: 0.48 } }
    Behavior on color { ColorAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

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
        size: 24
        color: root.tint
    }
    TapHandler { id: tap; onTapped: root.clicked() }
}
