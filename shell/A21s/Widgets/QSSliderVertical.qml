import QtQuick
import A21s.Theme
import A21s.Services

// Vertical quick-settings slider (brightness / volume) for the shade: a tall
// pill that fills from the bottom with the accent, glyph riding the fill edge.
// Drag anywhere on the pill to set; the drag preview is local so an externally
// bound `value` keeps its binding until the backend catches up.
Item {
    id: root

    property real value: 0                 // 0..1, externally owned
    property string materialIcon: ""
    property bool interactive: true
    property string disabledHint: ""

    signal moved(real v)

    implicitWidth: Theme.shadeSliderWidth
    implicitHeight: Theme.shadeMediaHeight

    // Local preview while the finger is down.
    property real _drag: 0
    readonly property real _value: dragArea.pressed
        ? root._drag : Math.max(0, Math.min(1, root.value))

    opacity: root.interactive ? 1.0 : 0.5

    Rectangle {
        id: track
        anchors.fill: parent
        radius: width / 2
        color: Theme.shadeWell
        border.width: 1
        border.color: Theme.shadeLine

        Rectangle {
            id: fill
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            height: track.height * root._value
            radius: width / 2
            color: root.interactive ? ShellPrefs.accentColor : Theme.textFaint
        }

        // Sits on the fill edge (clamped inside the track) so it stays visible
        // at 0 % where the fill has no height at all.
        MaterialIcon {
            id: glyph
            anchors.horizontalCenter: parent.horizontalCenter
            readonly property real fillTop: track.height - fill.height
            y: Math.max(Theme.spaceXS,
                        Math.min(track.height - height - Theme.spaceXS,
                                 glyph.fillTop - height / 2))
            name: root.materialIcon
            size: Theme.fontTitle + 4
            color: (glyph.y + glyph.height > glyph.fillTop)
                ? ShellPrefs.accentContent : Theme.text
        }

        Text {
            anchors {
                horizontalCenter: parent.horizontalCenter
                bottom: parent.bottom
                bottomMargin: Theme.spaceS
            }
            visible: !root.interactive && root.disabledHint.length > 0
            text: root.disabledHint
            color: Theme.textFaint
            font.pixelSize: Theme.fontMicro
            font.family: Theme.fontFamily
        }
    }

    MouseArea {
        id: dragArea
        anchors.fill: parent
        enabled: root.interactive
        preventStealing: true

        function valueAt(y) {
            return Math.max(0, Math.min(1, 1 - y / height))
        }

        onPressed: function(mouse) {
            root._drag = valueAt(mouse.y)
            root.moved(root._drag)
        }
        onPositionChanged: function(mouse) {
            if (!pressed)
                return
            root._drag = valueAt(mouse.y)
            root.moved(root._drag)
        }
    }
}
