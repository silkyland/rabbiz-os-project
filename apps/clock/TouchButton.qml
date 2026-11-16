import QtQuick
import A21s.Theme
import A21s.Services
import A21s.Widgets

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
    color: root.filled ? Qt.alpha(ShellPrefs.accentColor, 0.18)
         : tap.pressed ? Theme.surfaceHigh : Theme.card
    border.width: 1
    border.color: Qt.alpha(Theme.text, 0.10)
    scale: tap.pressed ? 0.94 : 1.0
    Behavior on scale { NumberAnimation { duration: Theme.animFast } }

    MaterialIcon {
        anchors.centerIn: parent
        name: root.icon
        size: Theme.fontBody - 4
        color: root.tint
    }
    TapHandler { id: tap; onTapped: root.clicked() }
}
