import QtQuick
import Rabbiz.Theme
import Rabbiz.Widgets

// Round icon action for the photo viewer. 60 px so the thumb target is well above the
// 48 px floor even with the glyph centred; the kit's IconButton is 44 px for headers.
Rectangle {
    id: root
    property string icon: ""
    signal clicked()

    readonly property int hitSize: Theme.spaceXL + Theme.spaceS   // 48 + 12 = 60 px

    implicitWidth: hitSize
    implicitHeight: hitSize
    width: hitSize
    height: hitSize
    radius: hitSize / 2
    color: Theme.surfaceHigh
    border.width: 1
    border.color: Qt.alpha(Theme.text, 0.10)
    scale: tap.pressed ? 0.92 : 1.0
    Behavior on scale { NumberAnimation { duration: Theme.animFast } }

    MaterialIcon {
        anchors.centerIn: parent
        name: root.icon
        size: Theme.fontBody
        color: Theme.text
    }
    TapHandler { id: tap; onTapped: root.clicked() }
}
