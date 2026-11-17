import QtQuick
import A21s.Theme
import A21s.Services

// Dense quick-settings icon tile (One UI 7 grid): a large circle with the glyph
// and the label underneath. ON inverts the circle to Theme.text with an accent
// glyph — the same language as QSTile/QSRowTile, at thumb size.
Item {
    id: root

    property string materialIcon: ""
    property string label: ""
    property bool active: false

    signal tapped()

    implicitWidth: Theme.shadeGridTile + Theme.spaceM
    implicitHeight: circle.height + Theme.spaceXS + textItem.implicitHeight

    Rectangle {
        id: circle
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.top
        width: Theme.shadeGridTile
        height: Theme.shadeGridTile
        radius: width / 2
        color: root.active ? Theme.text : Theme.shadeWell
        border.width: root.active ? 0 : 1
        border.color: Theme.shadeLine

        Behavior on color { ColorAnimation { duration: Theme.animFast } }

        scale: pressArea.pressed ? 0.92 : 1.0
        Behavior on scale {
            NumberAnimation { duration: Theme.animFast; easing.type: Easing.OutCubic }
        }

        MaterialIcon {
            anchors.centerIn: parent
            name: root.materialIcon
            size: Math.round(Theme.shadeGridTile * 0.46)
            color: root.active ? ShellPrefs.accentInk : Theme.text
            Behavior on color { ColorAnimation { duration: Theme.animFast } }
        }
    }

    Text {
        id: textItem
        anchors {
            top: circle.bottom
            topMargin: Theme.spaceXS
            left: parent.left
            right: parent.right
        }
        text: root.label
        color: root.active ? Theme.text : Theme.textDim
        font.pixelSize: Theme.fontStatus
        font.family: Theme.fontFamily
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        maximumLineCount: 2
        elide: Text.ElideRight
    }

    MouseArea {
        id: pressArea
        anchors.fill: parent
        onClicked: root.tapped()
    }
}
