import QtQuick
import QtQuick.Layouts
import A21s.Theme

// One quick-settings tile: rounded rect, glyph + label (+ optional
// secondary line). `active` switches tileOn/tileOff colors. `stub`
// tiles (no backend yet) dim the label and show "soon"; every tap
// fires `tapped` and plays a short pulse animation.
Rectangle {
    id: root

    property string glyph: ""               // text/emoji icon
    property string label: ""
    property string secondary: ""           // small line under label
    property bool active: false
    property bool stub: false               // no backend; pulse only
    property Component glyphDelegate: null  // optional custom icon

    signal tapped()

    radius: Theme.radiusM
    color: active ? Theme.tileOn : Theme.tileOff
    Behavior on color { ColorAnimation { duration: Theme.animFast } }

    implicitWidth: Theme.iconApp + Theme.spaceM
    implicitHeight: tileCol.implicitHeight + Theme.spaceM

    scale: pressArea.pressed ? 0.94 : 1.0
    Behavior on scale {
        NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase }
    }

    SequentialAnimation {
        id: pulse
        NumberAnimation { target: root; property: "opacity"; to: 0.35; duration: Theme.animFast }
        NumberAnimation { target: root; property: "opacity"; to: 1.0;  duration: Theme.animMed }
    }

    ColumnLayout {
        id: tileCol
        anchors {
            left: parent.left
            right: parent.right
            verticalCenter: parent.verticalCenter
            leftMargin: Theme.spaceXS
            rightMargin: Theme.spaceXS
        }
        spacing: Theme.spaceXS

        Loader {
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredHeight: Theme.iconS
            sourceComponent: root.glyphDelegate ? root.glyphDelegate : glyphText
        }

        Text {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            text: root.label
            color: root.stub ? Theme.textFaint
                 : root.active ? Theme.tileOnText : Theme.tileOffText
            font.pixelSize: Theme.fontCaption
            elide: Text.ElideRight
            maximumLineCount: 1
        }

        Text {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            visible: text.length > 0
            text: root.stub && root.secondary === "" ? "soon" : root.secondary
            color: root.active ? Theme.tileOnText : Theme.textFaint
            font.pixelSize: Theme.fontStatus
            elide: Text.ElideRight
            maximumLineCount: 1
        }
    }

    Component {
        id: glyphText
        Text {
            text: root.glyph
            font.pixelSize: Theme.fontTitle
            color: root.active ? Theme.tileOnText : Theme.tileOffText
        }
    }

    MouseArea {
        id: pressArea
        anchors.fill: parent
        onClicked: {
            pulse.restart()
            root.tapped()
        }
    }
}
