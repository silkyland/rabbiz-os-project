import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services
import A21s.Widgets

// Vertical stepper: label, big value, 56 px + / - buttons. Alarms and the timer are set
// without a text field, so the on-screen keyboard never covers the screen.
ColumnLayout {
    id: root
    property string label: ""
    property string valueText: ""
    signal step(int delta)

    spacing: Theme.spaceXS

    Text {
        Layout.fillWidth: true
        horizontalAlignment: Text.AlignHCenter
        text: root.label
        color: Theme.textDim
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontCaption - 2
        font.weight: Font.DemiBold
        font.letterSpacing: 1.2
    }

    Rectangle {
        Layout.fillWidth: true
        implicitHeight: 56
        radius: Theme.radiusM
        color: upTap.pressed ? Theme.card : Theme.surfaceHigh
        border.width: 1
        border.color: Qt.alpha(Theme.text, 0.08)

        MaterialIcon {
            anchors.centerIn: parent
            name: "expand_less"
            size: 28
            color: ShellPrefs.accentColor
        }
        TapHandler { id: upTap; onTapped: root.step(1) }
    }

    Text {
        Layout.fillWidth: true
        horizontalAlignment: Text.AlignHCenter
        text: root.valueText
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontHeadline
        font.weight: Font.Light
    }

    Rectangle {
        Layout.fillWidth: true
        implicitHeight: 56
        radius: Theme.radiusM
        color: downTap.pressed ? Theme.card : Theme.surfaceHigh
        border.width: 1
        border.color: Qt.alpha(Theme.text, 0.08)

        MaterialIcon {
            anchors.centerIn: parent
            name: "expand_more"
            size: 28
            color: ShellPrefs.accentColor
        }
        TapHandler { id: downTap; onTapped: root.step(-1) }
    }
}
