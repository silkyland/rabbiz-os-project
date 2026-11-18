import QtQuick
import Rabbiz.Theme

// The tile that follows the finger while an icon is being moved.
// A non-interactive AppIcon (same entry, same surface, so the user's label/icon
// overrides match the source) lifted above the dock and the grid.
Item {
    id: ghost

    property var entry: null
    property string surface: "desktop"
    property int tileSize: Theme.iconApp - 4

    visible: false
    width: proxy.implicitWidth
    height: proxy.implicitHeight

    // Cheap soft shadow: a translucent rounded square behind the tile (no GPU effect
    // needed — this runs on the software renderer too).
    Rectangle {
        width: proxy.tileItem.width + 12
        height: proxy.tileItem.height + 12
        x: proxy.tileItem.x + proxy.tileItem.width / 2 - width / 2
        y: proxy.tileItem.y + proxy.tileItem.height / 2 - height / 2 + 10
        radius: proxy.tileItem.radius + 6
        color: Qt.alpha("#000000", 0.32)
    }

    AppIcon {
        id: proxy
        anchors.fill: parent
        entry: ghost.entry
        surface: ghost.surface
        interactive: false
        tileSize: ghost.tileSize
        labelVisible: true
    }
}
