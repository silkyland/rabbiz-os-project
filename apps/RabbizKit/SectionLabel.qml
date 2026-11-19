import QtQuick
import Rabbiz.Theme

// Uppercase dim section heading. Same metrics as the shell's Panel sections.
Text {
    color: Theme.textDim
    font.pixelSize: Theme.fontCaption - 2
    font.family: Theme.fontFamily
    font.weight: Font.DemiBold
    font.letterSpacing: 1.4
    elide: Text.ElideRight
    text: ""
}
