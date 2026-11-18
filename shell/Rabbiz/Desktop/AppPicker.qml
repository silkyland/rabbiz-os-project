import QtQuick
import QtQuick.Layouts
import Quickshell
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets
import "AppCatalog.js" as AppCatalog

// Bottom sheet that adds an installed app to ~/Desktop or to the dock.
// Opens from the "+" tile on either surface and from the icon context menu.
Item {
    id: picker

    property bool open: false
    property string surface: "desktop"        // "desktop" | "dock"
    property string surfaceLabel: "~/Desktop"
    property var existingIds: []              // ids already on that surface
    property bool full: false                 // surface is at capacity
    property Item backdropSource: null

    signal picked(string entryId)
    signal closed()

    property var apps: []

    function rebuild() {
        apps = AppCatalog.launcherEntries(DesktopEntries.applications ? DesktopEntries.applications.values : [],
                                          search.text)
    }

    function begin() {
        search.text = ""
        rebuild()
        open = true
        search.forceActiveFocus()
    }

    function close() {
        open = false
        search.focus = false
        closed()
    }

    function alreadyAdded(id) {
        return DesktopLayout.contains(surface, String(id))
    }

    Component.onCompleted: rebuild()

    Connections {
        target: DesktopEntries
        function onApplicationsChanged() { picker.rebuild() }
    }

    visible: opacity > 0.01
    opacity: open ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: Theme.animMed; easing.type: Theme.animEase } }

    Rectangle {
        anchors.fill: parent
        color: Theme.scrim
        opacity: 0.42 * picker.opacity
        TapHandler {
            onTapped: picker.close()
        }
    }

    Item {
        id: sheet
        width: parent.width
        height: parent.height * 0.72
        y: picker.open ? parent.height - height : parent.height
        Behavior on y { NumberAnimation { duration: Theme.animMed; easing.type: Theme.animEase } }

        BackdropBlur {
            anchors.fill: parent
            sourceItem: picker.backdropSource
            sourceRect: Qt.rect(0, sheet.y, width, height)
            blurEnabled: picker.open
            radius: Theme.radiusL
            color: gpu ? Theme.glassBlurHigh : Theme.glassHigh
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.10)
        }

        Rectangle {
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            height: Theme.radiusL
            color: Theme.glassHigh
        }

        Item {
            id: head
            width: parent.width
            height: 44

            Rectangle {
                anchors.centerIn: parent
                width: 56
                height: 6
                radius: Theme.radiusPill
                color: Qt.alpha(Theme.textDim, 0.45)
            }
        }

        RowLayout {
            id: titleRow
            anchors {
                top: head.bottom
                left: parent.left
                right: parent.right
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
            }
            spacing: Theme.spaceS

            MaterialIcon {
                name: picker.surface === "dock" ? "dock" : "add"
                size: Theme.fontTitle - 6
                color: ShellPrefs.accentColor
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                Text {
                    Layout.fillWidth: true
                    text: "Add to " + picker.surfaceLabel
                    color: Theme.text
                    font.pixelSize: Theme.fontTitle - 6
                    font.family: Theme.fontFamily
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }
                Text {
                    Layout.fillWidth: true
                    text: picker.full
                        ? "Full — remove an icon first"
                        : "Choose an installed app"
                    color: picker.full ? Theme.warn : Theme.textFaint
                    font.pixelSize: Theme.fontStatus - 3
                    font.family: Theme.fontFamily
                }
            }

            Rectangle {
                width: Theme.iconS
                height: Theme.iconS
                radius: Theme.radiusS
                color: Qt.alpha(Theme.text, 0.08)
                MaterialIcon {
                    anchors.centerIn: parent
                    name: "close"
                    size: Theme.fontCaption
                    color: Theme.textDim
                }
                TapHandler {
                    onTapped: picker.close()
                }
            }
        }

        Rectangle {
            id: searchBar
            anchors {
                top: titleRow.bottom
                topMargin: Theme.spaceS
                left: parent.left
                right: parent.right
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
            }
            height: Theme.iconM - 6
            radius: Theme.radiusPill
            color: Theme.surfaceHigh
            border.width: 1.5
            border.color: search.activeFocus
                ? ShellPrefs.accentColor
                : Qt.alpha(Theme.text, 0.09)

            Behavior on border.color { ColorAnimation { duration: Theme.animFast } }

            RowLayout {
                anchors {
                    fill: parent
                    leftMargin: Theme.spaceM
                    rightMargin: Theme.spaceM
                }
                spacing: Theme.spaceS

                MaterialIcon {
                    name: "search"
                    size: Theme.fontCaption
                    color: search.activeFocus ? ShellPrefs.accentColor : Theme.textDim
                }

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    TextInput {
                        id: search
                        anchors.fill: parent
                        color: Theme.text
                        font.pixelSize: Theme.fontCaption - 2
                        font.family: Theme.fontFamily
                        verticalAlignment: TextInput.AlignVCenter
                        clip: true
                        onTextChanged: picker.rebuild()
                        Keys.onEscapePressed: picker.close()
                    }

                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: search.text.length === 0
                        text: "Search apps…"
                        color: Theme.textFaint
                        font.pixelSize: Theme.fontCaption - 2
                        font.family: Theme.fontFamily
                    }
                }
            }
        }

        GridView {
            id: grid
            anchors {
                top: searchBar.bottom
                topMargin: Theme.spaceM
                left: parent.left
                right: parent.right
                bottom: parent.bottom
                leftMargin: Theme.spaceS
                rightMargin: Theme.spaceS
                bottomMargin: Theme.navZoneHeight
            }
            clip: true
            cellWidth: Math.floor(width / 4)
            cellHeight: Theme.iconApp + Theme.fontCaption * 2 + Theme.spaceM + 8
            model: picker.apps
            boundsBehavior: Flickable.StopAtBounds

            delegate: Item {
                required property var modelData
                width: grid.cellWidth
                height: grid.cellHeight

                readonly property bool added: picker.alreadyAdded(modelData.id)

                AppIcon {
                    anchors.centerIn: parent
                    width: grid.cellWidth
                    height: grid.cellHeight - Theme.spaceS
                    entry: modelData
                    interactive: false
                    tileSize: Theme.iconApp - 8
                    labelVisible: true
                }

                Rectangle {
                    anchors {
                        top: parent.top
                        right: parent.right
                        topMargin: Theme.spaceXS
                        rightMargin: Theme.spaceM
                    }
                    width: 26
                    height: 26
                    radius: 13
                    color: added
                        ? Qt.alpha(Theme.success, 0.9)
                        : ShellPrefs.accentColor
                    border.width: 2
                    border.color: Theme.bg

                    MaterialIcon {
                        anchors.centerIn: parent
                        name: added ? "check" : "add"
                        size: Theme.fontStatus - 2
                        color: added ? Theme.bg : ShellPrefs.onAccentColor
                    }
                }

                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radiusM
                    color: Qt.alpha(Theme.text, 0.06)
                    opacity: addRow.pressed ? 1 : 0
                    Behavior on opacity { NumberAnimation { duration: Theme.animFast } }
                }

                TapHandler {
                    id: addRow
                    enabled: !picker.full && !added
                    onTapped: {
                        picker.picked(String(modelData.id))
                        picker.close()
                    }
                }
            }
        }

        Text {
            anchors.centerIn: grid
            visible: grid.count === 0
            text: "No matching apps"
            color: Theme.textFaint
            font.pixelSize: Theme.fontBody
            font.family: Theme.fontFamily
        }
    }
}
