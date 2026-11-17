import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services

// Large quick-settings tile (One UI 7 style): a full-pill card with a round icon
// well, two text lines and an optional expand chevron. Tap toggles, press-and-hold
// fires the long-press action (Wi-Fi/Bluetooth use it to open the connectivity
// panel). ON state inverts the well to Theme.text and tints the glyph with the
// accent, so the accent stays the only colour on the sheet.
Rectangle {
    id: root

    property string materialIcon: ""
    property string label: ""
    property string secondary: ""
    property bool active: false
    property bool chevron: true

    signal tapped()
    signal longPressed()

    // pressAndHold also emits clicked on release; this suppresses that.
    property bool _lp: false

    implicitWidth: 280
    implicitHeight: Theme.shadeTileHeight
    radius: height / 2
    color: root.active ? Theme.shadeCardOn : Theme.shadeCard
    border.width: 1
    border.color: root.active ? Qt.alpha(Theme.text, 0.18) : Theme.shadeLine

    Behavior on color { ColorAnimation { duration: Theme.animFast } }
    Behavior on border.color { ColorAnimation { duration: Theme.animFast } }

    scale: pressArea.pressed ? 0.97 : 1.0
    Behavior on scale {
        NumberAnimation { duration: Theme.animFast; easing.type: Easing.OutCubic }
    }

    RowLayout {
        anchors {
            fill: parent
            leftMargin: Theme.spaceXS + 2
            rightMargin: Theme.spaceM
        }
        spacing: Theme.spaceS

        Rectangle {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: Theme.shadeIconWell
            Layout.preferredHeight: Theme.shadeIconWell
            radius: width / 2
            color: root.active ? Theme.text : Theme.shadeWell

            Behavior on color { ColorAnimation { duration: Theme.animFast } }

            MaterialIcon {
                anchors.centerIn: parent
                name: root.materialIcon
                size: Math.round(Theme.shadeIconWell * 0.5)
                color: root.active ? ShellPrefs.accentInk : Theme.text
                Behavior on color { ColorAnimation { duration: Theme.animFast } }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            spacing: 0

            Text {
                Layout.fillWidth: true
                text: root.label
                color: Theme.text
                font.pixelSize: Theme.fontCaption + 2
                font.family: Theme.fontFamily
                font.weight: Font.DemiBold
                elide: Text.ElideRight
                maximumLineCount: 1
            }

            Text {
                Layout.fillWidth: true
                visible: text.length > 0
                text: root.secondary
                color: Theme.textDim
                font.pixelSize: Theme.fontStatus
                font.family: Theme.fontFamily
                elide: Text.ElideRight
                maximumLineCount: 1
            }
        }

        MaterialIcon {
            Layout.alignment: Qt.AlignVCenter
            visible: root.chevron
            name: "unfold_more"
            size: Theme.fontBody
            color: Theme.textFaint
        }
    }

    MouseArea {
        id: pressArea
        anchors.fill: parent
        onPressed: root._lp = false
        onClicked: {
            if (root._lp)
                return
            root.tapped()
        }
        onPressAndHold: {
            root._lp = true
            root.longPressed()
        }
    }
}
