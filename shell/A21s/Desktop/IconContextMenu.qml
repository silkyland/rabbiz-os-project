import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services
import A21s.Widgets

// Long-press context menu for a ~/Desktop or dock icon.
// Floating glass card anchored to the icon; the caller supplies the action list
// (DesktopView owns the rules, so the same menu serves both surfaces).
Item {
    id: menu

    property bool open: false
    // [{ id, label, icon, danger, enabled }] — caller-built, order is display order.
    property var actions: []
    // { id, name, materialIcon, iconUrl, tint, running } — header content.
    property var target: null
    // Scene rect of the icon tile, used to place the card above/below it.
    property rect anchorRect: Qt.rect(0, 0, 0, 0)

    signal actionTriggered(string action)
    signal dismissed()

    readonly property int pad: Theme.spaceS
    readonly property int cardWidth: Math.min(320, width - 2 * Theme.spaceM)

    readonly property real placedX: Math.max(Theme.spaceM,
        Math.min(anchorRect.x + anchorRect.width / 2 - cardWidth / 2, width - cardWidth - Theme.spaceM))
    readonly property bool placedAbove: anchorRect.y - card.height - 12 > Theme.statusBarHeight
    readonly property real placedY: {
        if (placedAbove)
            return anchorRect.y - card.height - 12
        var below = anchorRect.y + anchorRect.height + 12
        return Math.min(below, height - card.height - Theme.navZoneHeight - Theme.spaceM)
    }

    visible: opacity > 0.01
    opacity: open ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

    // Tap-anywhere-else to dismiss. A tap (not a swipe) so a pan across the home
    // screen does not eat the touch that opens the drawer.
    Rectangle {
        anchors.fill: parent
        color: Theme.scrim
        opacity: 0.22 * menu.opacity

        TapHandler {
            onTapped: menu.dismissed()
        }
    }

    Rectangle {
        id: card
        x: menu.placedX
        y: menu.placedY
        width: menu.cardWidth
        height: body.implicitHeight + menu.pad * 2
        radius: Theme.radiusM + 4
        color: Qt.alpha(Theme.surfaceHigh, 0.98)
        border.width: 1
        border.color: Qt.alpha(Theme.text, 0.12)

        scale: menu.open ? 1.0 : 0.92
        transformOrigin: menu.placedAbove ? Item.Bottom : Item.Top
        Behavior on scale {
            NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack }
        }
        Behavior on y { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

        Column {
            id: body
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
                leftMargin: menu.pad
                rightMargin: menu.pad
                topMargin: menu.pad
            }
            spacing: 0

            // ---- header: what was long-pressed, and where it lives ----
            Item {
                width: parent.width
                height: headerRow.implicitHeight + menu.pad

                RowLayout {
                    id: headerRow
                    anchors {
                        left: parent.left
                        right: parent.right
                        top: parent.top
                        leftMargin: Theme.spaceXS
                        rightMargin: Theme.spaceXS
                    }
                    spacing: menu.pad

                    Rectangle {
                        Layout.alignment: Qt.AlignVCenter
                        width: Theme.iconS
                        height: Theme.iconS
                        radius: Theme.radiusS
                        color: Qt.alpha(Theme.text, 0.06)
                        border.width: 1
                        border.color: Qt.alpha(Theme.text, 0.08)

                        Image {
                            id: art
                            anchors.centerIn: parent
                            width: parent.width * 0.78
                            height: width
                            source: menu.target && menu.target.iconUrl ? String(menu.target.iconUrl) : ""
                            fillMode: Image.PreserveAspectFit
                            asynchronous: true
                            visible: status === Image.Ready
                        }
                        MaterialIcon {
                            anchors.centerIn: parent
                            visible: art.status !== Image.Ready
                            name: menu.target && menu.target.materialIcon ? String(menu.target.materialIcon) : "apps"
                            size: Theme.fontCaption
                            color: Theme.text
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0

                        Text {
                            Layout.fillWidth: true
                            text: menu.target && menu.target.name ? String(menu.target.name) : ""
                            color: Theme.text
                            font.pixelSize: Theme.fontBody - 2
                            font.family: Theme.fontFamily
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                        }
                        Text {
                            Layout.fillWidth: true
                            text: menu.target && menu.target.surfaceLabel
                                ? String(menu.target.surfaceLabel) : ""
                            color: Theme.textFaint
                            font.pixelSize: Theme.fontStatus - 3
                            font.family: Theme.fontFamily
                            elide: Text.ElideRight
                        }
                    }
                }
            }

            Rectangle {
                width: parent.width
                height: 1
                color: Qt.alpha(Theme.text, 0.08)
            }

            // ---- actions ----
            Repeater {
                model: menu.actions

                delegate: Item {
                    id: actionRow
                    required property var modelData
                    width: body.width
                    height: 46

                    readonly property bool enabledRow: modelData.enabled === undefined
                        ? true : Boolean(modelData.enabled)
                    readonly property bool dangerRow: Boolean(modelData.danger)

                    Rectangle {
                        anchors {
                            fill: parent
                            leftMargin: -menu.pad + Theme.spaceXS
                            rightMargin: -menu.pad + Theme.spaceXS
                        }
                        radius: Theme.radiusS
                        color: {
                            if (!actionRow.enabledRow)
                                return "transparent"
                            if (actionTap.pressed)
                                return Qt.alpha(actionRow.dangerRow ? Theme.danger : ShellPrefs.accentColor, 0.20)
                            return "transparent"
                        }
                    }

                    Row {
                        anchors {
                            left: parent.left
                            right: parent.right
                            verticalCenter: parent.verticalCenter
                            leftMargin: Theme.spaceXS
                            rightMargin: Theme.spaceXS
                        }
                        spacing: menu.pad

                        MaterialIcon {
                            name: actionRow.modelData.icon ? String(actionRow.modelData.icon) : "chevron_right"
                            size: Theme.fontCaption
                            color: actionRow.enabledRow
                                ? (actionRow.dangerRow ? Theme.danger : Theme.text)
                                : Theme.textFaint
                            opacity: actionRow.enabledRow ? 1.0 : 0.5
                        }

                        Text {
                            width: parent.width - Theme.fontCaption - menu.pad
                            text: actionRow.modelData.label ? String(actionRow.modelData.label) : ""
                            color: actionRow.enabledRow
                                ? (actionRow.dangerRow ? Theme.danger : Theme.text)
                                : Theme.textFaint
                            font.pixelSize: Theme.fontCaption - 2
                            font.family: Theme.fontFamily
                            font.weight: actionRow.dangerRow ? Font.DemiBold : Font.Medium
                            elide: Text.ElideRight
                            verticalAlignment: Text.AlignVCenter
                        }
                    }

                    TapHandler {
                        id: actionTap
                        enabled: actionRow.enabledRow
                        onTapped: menu.actionTriggered(String(actionRow.modelData.id))
                    }
                }
            }

            // ---- hint: the drag affordance is not visible anywhere else ----
            Item {
                width: parent.width
                height: 30

                Row {
                    anchors {
                        left: parent.left
                        verticalCenter: parent.verticalCenter
                        leftMargin: Theme.spaceXS
                    }
                    spacing: Theme.spaceXS

                    MaterialIcon {
                        name: "drag_indicator"
                        size: Theme.fontStatus - 4
                        color: Theme.textFaint
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "Hold & drag to move"
                        color: Theme.textFaint
                        font.pixelSize: Theme.fontStatus - 4
                        font.family: Theme.fontFamily
                    }
                }
            }
        }
    }
}
