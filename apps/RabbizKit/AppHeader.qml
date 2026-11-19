import QtQuick
import QtQuick.Layouts
import Rabbiz.Theme

// Shared page header: big title, optional subtitle, optional trailing actions (an Item).
Item {
    id: root
    property string title: ""
    property string subtitle: ""
    default property alias actions: actionsRow.data

    implicitHeight: Math.max(68, titleCol.implicitHeight + Theme.spaceXS)

    Column {
        id: titleCol
        anchors {
            left: parent.left
            right: actionsRow.left
            rightMargin: Theme.spaceS
            verticalCenter: parent.verticalCenter
        }
        spacing: 2
        Text {
            width: parent.width
            text: root.title
            color: Theme.text
            font.pixelSize: Theme.fontTitle
            font.family: Theme.fontFamily
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            visible: root.subtitle.length > 0
            text: root.subtitle
            color: Theme.textDim
            font.pixelSize: Theme.fontCaption
            font.family: Theme.fontFamily
            elide: Text.ElideRight
        }
    }
    Row {
        id: actionsRow
        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
        spacing: Theme.spaceS
    }
}
