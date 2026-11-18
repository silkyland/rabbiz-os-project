import QtQuick
import Rabbiz.Theme
import Rabbiz.Services

// Custom quick-settings slider (named QSSlider to avoid clashing with
// QtQuick.Controls Slider). Left icon (tappable -> iconTapped), pill
// track filled with Theme.primary, drag anywhere on the track to set.
// While dragging, the fill follows the finger WITHOUT touching `value`
// (so externally-bound values keep their binding); `moved` is emitted
// continuously and the backend re-binds `value` when it catches up.
Item {
    id: root

    property real value: 0              // 0..1, externally owned
    property string glyph: ""           // text/emoji icon on the left
    property string materialIcon: ""    // MaterialIconsRound name
    property bool interactive: true     // false -> dimmed, input ignored
    property string disabledHint: ""    // shown in track when !interactive

    signal moved(real v)
    signal iconTapped()

    // Width is driven by the parent layout (Layout.fillWidth); implicit
    // height is the pill-track row height.
    implicitHeight: Theme.iconM

    opacity: interactive ? 1.0 : 0.5

    // Fill actually rendered: drag preview when pressed, bound value else.
    readonly property real _fill: trackArea.pressed ? trackArea._v
                                                    : Math.max(0, Math.min(1, value))

    Item {
        id: icon
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        width: Theme.iconM
        height: Theme.iconM

        MaterialIcon {
            anchors.centerIn: parent
            visible: root.materialIcon.length > 0
            name: root.materialIcon
            size: Theme.iconS - 4
            color: Theme.textDim
        }

        Text {
            anchors.centerIn: parent
            visible: root.materialIcon.length === 0
            text: root.glyph
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontBody
            color: Theme.textDim
        }

        MouseArea {
            anchors.fill: parent
            enabled: root.interactive
            onClicked: root.iconTapped()
        }
    }

    Item {
        id: track
        anchors {
            left: icon.right
            right: parent.right
            top: parent.top
            bottom: parent.bottom
        }

        Rectangle {
            anchors.fill: parent
            radius: Theme.radiusPill
            color: Theme.surfaceHigh
        }

        Rectangle {
            anchors {
                left: parent.left
                top: parent.top
                bottom: parent.bottom
            }
            width: track.width * root._fill
            radius: Theme.radiusPill
            color: root.interactive ? ShellPrefs.accentColor : Theme.textFaint
        }

        Text {
            anchors {
                right: parent.right
                rightMargin: Theme.spaceM
                verticalCenter: parent.verticalCenter
            }
            text: root.interactive ? Math.round(root._fill * 100) + "%"
                                   : root.disabledHint
            font.family: Theme.fontFamily
            color: root.interactive && root._fill < 0.85 ? Theme.textDim
                                                         : Theme.text
            font.pixelSize: Theme.fontCaption
        }

        MouseArea {
            id: trackArea
            anchors.fill: parent
            enabled: root.interactive
            preventStealing: true

            property real _v: root.value

            function _clamp(v) {
                return Math.max(0, Math.min(1, v))
            }

            onPressed: function(mouse) {
                _v = _clamp(mouse.x / width)
                root.moved(_v)
            }
            onPositionChanged: function(mouse) {
                if (!pressed)
                    return
                _v = _clamp(mouse.x / width)
                root.moved(_v)
            }
        }
    }
}
