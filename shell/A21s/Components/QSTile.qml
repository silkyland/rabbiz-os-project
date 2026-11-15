import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services

// Stitch-designed Pixel/Android 15 pill Quick Settings tile:
// 2-column horizontal pill with left icon + right title & secondary status.
Rectangle {
    id: root

    property string glyph: ""               // fallback text/emoji icon
    property string materialIcon: ""        // MaterialIconsRound name
    property string label: ""
    property string secondary: ""           // small line under label
    property bool active: false
    property bool stub: false               // no backend; pulse only
    property Component glyphDelegate: null  // optional custom icon

    signal tapped()

    radius: Theme.radiusPill
    color: active ? ShellPrefs.accentColor : Theme.surfaceHigh
    border.width: active ? 0 : 1
    border.color: Theme.separator
    Behavior on color { ColorAnimation { duration: Theme.animFast } }

    implicitWidth: 280
    implicitHeight: 88

    scale: pressArea.pressed ? 0.96 : 1.0
    Behavior on scale {
        NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase }
    }

    SequentialAnimation {
        id: pulse
        NumberAnimation { target: root; property: "opacity"; to: 0.40; duration: Theme.animFast }
        NumberAnimation { target: root; property: "opacity"; to: 1.0;  duration: Theme.animMed }
    }

    RowLayout {
        anchors {
            fill: parent
            leftMargin: Theme.spaceM
            rightMargin: Theme.spaceM
        }
        spacing: Theme.spaceS + 4

        Loader {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: Theme.iconS
            Layout.preferredHeight: Theme.iconS
            sourceComponent: root.glyphDelegate ? root.glyphDelegate
                : (root.materialIcon.length > 0 ? matIconComp : glyphText)
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            spacing: 2

            Text {
                Layout.fillWidth: true
                text: root.label
                color: root.stub ? Theme.textFaint
                     : root.active ? ShellPrefs.onAccentColor : Theme.text
                font.pixelSize: Theme.fontCaption + 2
                font.family: Theme.fontFamily
                font.weight: Font.DemiBold
                elide: Text.ElideRight
                maximumLineCount: 1
            }

            Text {
                Layout.fillWidth: true
                visible: text.length > 0
                text: root.stub && root.secondary === "" ? "Off" : root.secondary
                color: root.active ? Qt.alpha(ShellPrefs.onAccentColor, 0.80) : Theme.textDim
                font.pixelSize: Theme.fontStatus
                font.family: Theme.fontFamily
                elide: Text.ElideRight
                maximumLineCount: 1
            }
        }
    }

    Component {
        id: matIconComp
        MaterialIcon {
            name: root.materialIcon
            size: Theme.iconS - 2
            color: root.stub ? Theme.textFaint
                 : root.active ? ShellPrefs.onAccentColor : Theme.text
        }
    }

    Component {
        id: glyphText
        Text {
            text: root.glyph
            font.pixelSize: Theme.fontTitle
            color: root.active ? ShellPrefs.onAccentColor : Theme.text
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
    }

    MouseArea {
        id: pressArea
        anchors.fill: parent
        onClicked: {
            if (root.stub)
                pulse.restart()
            root.tapped()
        }
    }
}
