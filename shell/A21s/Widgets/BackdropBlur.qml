import QtQuick
import QtQuick.Effects
import A21s.Theme

// Source must exclude this item (a sibling scene or a frozen screen capture).
// Never feed an output capture back into a live effect on that same output.
Item {
    id: root
    property Item sourceItem: null
    property rect sourceRect: Qt.rect(0, 0, width, height)
    property alias radius: tint.radius
    property alias color: tint.color
    property alias border: tint.border
    property bool blurEnabled: true
    readonly property bool gpu: GraphicsInfo.api === GraphicsInfo.OpenGL
    readonly property bool active: gpu && blurEnabled && sourceItem !== null && visible

    Loader {
        anchors.fill: parent
        active: root.active
        sourceComponent: Component {
            Item {
                ShaderEffectSource {
                    id: sample
                    visible: false
                    sourceItem: root.sourceItem
                    sourceRect: root.sourceRect
                    textureSize: Qt.size(Math.max(1, Math.ceil(root.width / 2)),
                                         Math.max(1, Math.ceil(root.height / 2)))
                    live: true
                    recursive: false
                }
                Rectangle {
                    id: mask
                    anchors.fill: parent
                    radius: root.radius
                    color: Theme.text
                    visible: false
                    layer.enabled: true
                }
                MultiEffect {
                    anchors.fill: parent
                    source: sample
                    autoPaddingEnabled: false
                    blurEnabled: true
                    blurMax: Theme.backdropBlurMax
                    blur: 1.0
                    maskEnabled: root.radius > 0
                    maskSource: mask
                }
            }
        }
    }
    Rectangle {
        id: tint
        anchors.fill: parent
        color: Theme.glass
        border.width: 1
        border.color: Theme.glassBorder
    }
}
