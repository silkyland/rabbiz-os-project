import QtQuick
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets

// Accent pill, 48 px tall — the local twin of the kit's PrimaryButton.
// Why a copy: apps/RabbizKit/PrimaryButton.qml uses MaterialIcon but does not import
// Rabbiz.Widgets, so loading it fails with "MaterialIcon is not a type" and takes the whole app
// down. RabbizKit is lead-owned, so the app keeps its own until that import is fixed; then this
// file can be deleted and the call sites switched back to RabbizKit.PrimaryButton.
Rectangle {
    id: root
    property string text: ""
    property string materialIcon: ""
    property bool enabled2: true
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
