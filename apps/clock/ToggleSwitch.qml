import QtQuick
import A21s.Theme
import A21s.Services

// 56 x 32 switch with a 48 px tall hit area. The app cannot rely on QtQuick.Controls' Switch
// styling (it follows the platform style, which is not pinned on the device), so the alarm
// rows use this small, predictable control instead.
Item {
    id: root
    property bool checked: false
    signal toggled()

    implicitWidth: 56
    implicitHeight: 48

    Rectangle {
        id: track
        anchors.centerIn: parent
        width: 56
        height: 32
        radius: height / 2
        color: root.checked ? ShellPrefs.accentColor : Theme.surfaceHigh
        border.width: root.checked ? 0 : 1
        border.color: Qt.alpha(Theme.text, 0.14)

        Rectangle {
            width: 26
            height: 26
            radius: width / 2
            anchors.verticalCenter: parent.verticalCenter
            x: root.checked ? track.width - width - 3 : 3
            color: root.checked ? ShellPrefs.onAccentColor : Theme.textDim
            Behavior on x {
                NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase }
            }
        }
    }

    TapHandler { onTapped: root.toggled() }
}
