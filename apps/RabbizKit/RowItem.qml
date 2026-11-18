import QtQuick
import QtQuick.Layouts
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets

// Tappable list row: icon, title, caption, optional trailing value + chevron.
Rectangle {
    id: root
    property string icon: ""
    property string title: ""
    property string caption: ""
    property string value: ""
    property bool showChevron: true
    signal clicked()

    implicitHeight: 68
    radius: Theme.radiusM
    color: tap.pressed ? Theme.surfaceHigh : "transparent"

    RowLayout {
        anchors { fill: parent; leftMargin: Theme.spaceS; rightMargin: Theme.spaceS }
        spacing: Theme.spaceS

        Rectangle {
            visible: root.icon.length > 0
            width: visible ? 38 : 0
            height: 38
            radius: 12
            color: Qt.alpha(ShellPrefs.accentColor, 0.16)
            MaterialIcon {
                anchors.centerIn: parent
                name: root.icon
                size: 20
                color: ShellPrefs.accentColor
            }
        }
        Column {
            Layout.fillWidth: true
            spacing: 2
            Text {
                text: root.title
                color: Theme.text
                font.pixelSize: Theme.fontBody - 2
                font.family: Theme.fontFamily
                elide: Text.ElideRight
                width: parent.width
            }
            Text {
                visible: root.caption.length > 0
                text: root.caption
                color: Theme.textDim
                font.pixelSize: Theme.fontCaption - 2
                font.family: Theme.fontFamily
                elide: Text.ElideRight
                width: parent.width
            }
        }
        Text {
            visible: root.value.length > 0
            text: root.value
            color: Theme.textDim
            font.pixelSize: Theme.fontCaption
            font.family: Theme.fontFamily
        }
        MaterialIcon {
            visible: root.showChevron
            name: "chevron_right"
            size: 20
            color: Theme.textFaint
        }
    }
    TapHandler { id: tap; onTapped: root.clicked() }
}
