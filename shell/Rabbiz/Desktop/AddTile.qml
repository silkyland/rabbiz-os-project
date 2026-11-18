import QtQuick
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets

// "Add app" affordance at the end of the ~/Desktop grid and inside the dock.
// Opens the app picker for that surface; hidden by the caller when the surface is full.
Item {
    id: add

    property int tileSize: Theme.iconApp
    property bool compact: false          // dock: circle, no label
    property string labelText: compact ? "" : "Add app"
    signal tapped()

    readonly property int circleSize: compact ? Theme.iconApp - 40 : tileSize

    implicitWidth: circleSize + (compact ? Theme.spaceS : Theme.spaceM)
    // Same box as an AppIcon cell (tile + one label line + the running pill gap), so
    // adding the tile does not change the grid's row height.
    implicitHeight: compact ? circleSize : tileSize + Theme.fontCaption + Theme.spaceS + 4

    Rectangle {
        id: ring
        anchors {
            top: parent.top
            horizontalCenter: parent.horizontalCenter
        }
        width: add.circleSize
        height: add.circleSize
        radius: Theme.radiusM + 4
        color: Qt.alpha(Theme.surfaceHigh, 0.35)
        border.width: 1.5
        border.color: press.pressed
            ? ShellPrefs.accentColor
            : Qt.alpha(Theme.text, 0.22)
        scale: press.pressed ? 0.92 : 1.0

        Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }
        Behavior on border.color { ColorAnimation { duration: Theme.animFast } }

        MaterialIcon {
            anchors.centerIn: parent
            name: "add"
            size: Math.round(add.circleSize * 0.44)
            color: press.pressed ? ShellPrefs.accentColor : Theme.textDim
        }
    }

    Text {
        anchors {
            top: ring.bottom
            topMargin: Theme.spaceXS
            left: parent.left
            right: parent.right
        }
        visible: !add.compact && add.labelText.length > 0
        text: add.labelText
        color: Theme.onWallpaperDim
        font.pixelSize: Theme.fontCaption
        font.family: Theme.fontFamily
        font.weight: Font.Medium
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
    }

    TapHandler {
        id: press
        onTapped: add.tapped()
    }
}
