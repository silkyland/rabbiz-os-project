import QtQuick
import A21s.Theme

// Horizontal battery gauge: outline + terminal nub + fill proportional to charge.
// level is 0..1. When `present` is false it shows an empty dashed-look outline
// (no fuel gauge wired yet) instead of a misleading 0%.
// Usage: BatteryMeter { present: true; level: 0.62; charging: false }
Item {
    id: meter

    property bool present: false
    property real level: 0
    property bool charging: false
    property int bodyWidth: 30
    property int bodyHeight: 15

    readonly property real clamped: Math.max(0, Math.min(1, meter.level))
    readonly property bool low: meter.present && !meter.charging && meter.clamped <= 0.15
    readonly property color tint: !meter.present ? Theme.textFaint
        : meter.charging ? Theme.success
        : meter.low ? Theme.danger
        : Theme.text

    implicitWidth: meter.bodyWidth + 3
    implicitHeight: meter.bodyHeight

    Rectangle {
        id: body
        width: meter.bodyWidth
        height: meter.bodyHeight
        radius: 4
        color: "transparent"
        border.width: 1.5
        border.color: Qt.alpha(meter.tint, 0.85)

        Rectangle {
            visible: meter.present
            anchors {
                left: parent.left; top: parent.top; bottom: parent.bottom
                margins: 2.5
            }
            width: Math.max(meter.clamped > 0 ? 2 : 0,
                            (body.width - 5) * meter.clamped)
            radius: 2
            color: meter.tint
            Behavior on width { NumberAnimation { duration: Theme.animMed } }
        }

        MaterialIcon {
            visible: meter.present && meter.charging
            anchors.centerIn: parent
            name: "bolt"
            size: meter.bodyHeight - 1
            color: Theme.bg
        }
    }

    Rectangle {
        anchors {
            left: body.right
            verticalCenter: body.verticalCenter
        }
        width: 3
        height: Math.round(meter.bodyHeight * 0.45)
        radius: 1.5
        color: Qt.alpha(meter.tint, 0.85)
    }
}
