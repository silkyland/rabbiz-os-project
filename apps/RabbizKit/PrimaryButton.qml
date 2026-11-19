import QtQuick
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets

// Accent pill. Disabled state dims instead of removing the label.
Rectangle {
    id: root
    property string text: ""
    property bool enabled2: true
    property string materialIcon: ""
    signal clicked()

    implicitWidth: label.implicitWidth + (materialIcon.length > 0 ? 54 : 40)
    implicitHeight: 52
    radius: height / 2
    opacity: enabled2 ? 1.0 : 0.45
    color: ShellPrefs.accentColor
    scale: (tap.pressed && enabled2) ? 0.96 : 1.0
    Behavior on scale { SpringAnimation { spring: 6.5; damping: 0.48 } }
    Behavior on opacity { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

    Rectangle {
        anchors.fill: parent
        radius: parent.radius
        color: ShellPrefs.onAccentColor
        opacity: (tap.pressed && root.enabled2) ? 0.16 : 0.0
        Behavior on opacity { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }
    }

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
