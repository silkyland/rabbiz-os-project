import QtQuick
import A21s.Theme
import A21s.Services
import A21s.Widgets

// Accent pill. Disabled state dims instead of removing the label.
Rectangle {
    id: root
    property string text: ""
    property bool enabled2: true
    property string materialIcon: ""
    signal clicked()

    implicitWidth: label.implicitWidth + (materialIcon.length > 0 ? 54 : 40)
    implicitHeight: 48
    radius: height / 2
    opacity: enabled2 ? 1.0 : 0.45
    color: ShellPrefs.accentColor
    scale: tap.pressed ? 0.96 : 1.0
    Behavior on scale { NumberAnimation { duration: Theme.animFast } }

    Row {
        anchors.centerIn: parent
        spacing: 8
        Item {
            visible: root.materialIcon.length > 0
            width: visible ? 20 : 0
            height: 20
            MaterialIcon {
                anchors.centerIn: parent
                name: root.materialIcon
                size: 20
                color: ShellPrefs.onAccentColor
            }
        }
        Text {
            id: label
            anchors.verticalCenter: parent.verticalCenter
            text: root.text
            color: ShellPrefs.onAccentColor
            font.pixelSize: Theme.fontBody - 4
            font.family: Theme.fontFamily
            font.weight: Font.Medium
        }
    }
    TapHandler { id: tap; onTapped: if (root.enabled2) root.clicked() }
}
