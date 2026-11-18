import QtQuick
import QtQuick.Layouts
import Rabbiz.Theme

// Shared page header: big title, optional subtitle, optional trailing actions (an Item).
Item {
    id: root
    property string title: ""
    property string subtitle: ""
    default property alias actions: actionsRow.data

    implicitHeight: 66

    Column {
        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
        spacing: 2
        Text {
            text: root.title
            color: Theme.text
            font.pixelSize: Theme.fontTitle
            font.family: Theme.fontFamily
            font.weight: Font.DemiBold
        }
        Text {
            visible: root.subtitle.length > 0
            text: root.subtitle
            color: Theme.textDim
            font.pixelSize: Theme.fontCaption
            font.family: Theme.fontFamily
        }
    }
    Row {
        id: actionsRow
        anchors { right: parent.right; verticalCenter: parent.verticalCenter }
        spacing: Theme.spaceS
    }
}
