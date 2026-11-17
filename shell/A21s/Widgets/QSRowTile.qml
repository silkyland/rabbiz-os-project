import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services

// Compact tile used *inside* a grouped quick-settings card (e.g. Bluetooth +
// Connected devices share one rounded card). Same ON language as QSTile — the
// well inverts to Theme.text — at the smaller group size.
Item {
    id: root

    property string materialIcon: ""
    property string label: ""
    property string secondary: ""
    property bool active: false

    signal tapped()
    signal longPressed()

    property bool _lp: false

    implicitHeight: Theme.shadeGroupWell + Theme.spaceXS * 3

    RowLayout {
        anchors {
            fill: parent
            leftMargin: Theme.spaceS - 4
            rightMargin: Theme.spaceS - 4
        }
        spacing: Theme.spaceS - 2

        Rectangle {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: Theme.shadeGroupWell
            Layout.preferredHeight: Theme.shadeGroupWell
            radius: width / 2
            color: root.active ? Theme.text : Theme.shadeWell

            Behavior on color { ColorAnimation { duration: Theme.animFast } }

            MaterialIcon {
                anchors.centerIn: parent
                name: root.materialIcon
                size: Math.round(Theme.shadeGroupWell * 0.55)
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
                font.pixelSize: Theme.fontCaption
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
                font.pixelSize: Theme.fontStatus - 2
                font.family: Theme.fontFamily
                elide: Text.ElideRight
                maximumLineCount: 1
            }
        }
    }

    MouseArea {
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
