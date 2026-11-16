import QtQuick
import A21s.Theme
import A21s.Services
import A21s.Widgets

// 44 px circular icon action, used in app headers and list rows.
Rectangle {
    id: root
    property string icon: ""
    property bool filled: false
    property color tint: Theme.text
    signal clicked()

    width: 44; height: 44; radius: 22
    color: filled ? Qt.alpha(ShellPrefs.accentColor, 0.18) : Theme.surfaceHigh
    border.width: filled ? 0 : 1
    border.color: Qt.alpha(Theme.text, 0.10)
    scale: tap.pressed ? 0.92 : 1.0
    Behavior on scale { NumberAnimation { duration: Theme.animFast } }

    MaterialIcon {
        anchors.centerIn: parent
        name: root.icon
        size: 22
        color: root.tint
    }
    TapHandler { id: tap; onTapped: root.clicked() }
}
