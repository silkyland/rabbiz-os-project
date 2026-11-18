import QtQuick
import Rabbiz.Theme

// Google Material Icons Round (Apache 2.0) via bundled font ligatures.
// Usage: MaterialIcon { name: "wifi"; size: 36; color: Theme.text }
Text {
    id: icon

    property string name: ""
    property int size: Theme.iconS

    FontLoader {
        id: materialFont
        source: Qt.resolvedUrl("../../assets/fonts/MaterialIconsRound-Regular.otf")
    }

    text: icon.name
    font.family: materialFont.name
    font.pixelSize: icon.size
    color: Theme.text
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
    renderType: Text.NativeRendering
}
