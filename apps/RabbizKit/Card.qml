import QtQuick
import Rabbiz.Theme

// Soft surface for grouped blocks. Content is placed inside `inner` (width available as
// `inner.width`); the card sizes to its content. Do NOT anchor content with anchors.fill —
// that makes childrenRect depend on the card height (binding loop). For a card that must
// fill the page, set its height explicitly and place a layout with anchors.fill inside a
// plain Rectangle instead.
Rectangle {
    id: root
    default property alias contentData: inner.data
    property int padding: Theme.spaceM

    implicitWidth: 200
    implicitHeight: inner.childrenRect.height + padding * 2
    radius: Theme.radiusL
    color: Theme.card
    border.width: 1
    border.color: Qt.alpha(Theme.text, 0.08)
    Behavior on color { ColorAnimation { duration: Theme.animMed; easing.type: Theme.animEase } }

    Item {
        id: inner
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
            margins: root.padding
        }
        height: childrenRect.height
    }
}
