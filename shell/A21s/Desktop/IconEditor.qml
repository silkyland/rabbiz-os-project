import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services
import A21s.Widgets

// Bottom sheet for renaming a ~/Desktop / dock icon and choosing its glyph.
// Empty label / empty glyph mean "use the app's own", so Reset just clears both.
Item {
    id: editor

    property bool open: false
    // { id, surface, surfaceLabel, name (effective), appName (the app's own name),
    //   defaultIcon (entry glyph), overrideIcon (current), iconUrl }
    property var target: null
    property Item backdropSource: null
    property string draftLabel: ""
    property string draftIcon: ""

    signal applied(string label, string icon)
    signal resetRequested()
    signal closed()

    // A small curated set — enough to give any app a distinct face without a font browser.
    readonly property var glyphs: [
        "apps", "folder", "description", "calendar_month", "schedule", "calculate",
        "terminal", "edit_note", "photo_library", "music_note", "movie", "headphones",
        "call", "sms", "mail", "chat", "settings", "widgets",
        "shopping_cart", "savings", "map", "flight", "directions_bike", "restaurant",
        "local_cafe", "fitness_center", "school", "work", "science", "medical_services",
        "security", "key", "lock", "person", "group", "star",
        "favorite", "bolt", "cloud", "download", "tv", "sports_esports"
    ]

    function begin(t) {
        target = t
        draftLabel = t && t.name ? String(t.name) : ""
        draftIcon = t && t.overrideIcon ? String(t.overrideIcon) : ""
        open = true
        nameInput.text = draftLabel
        nameInput.forceActiveFocus()
        nameInput.selectAll()
    }

    function close() {
        open = false
        nameInput.focus = false
        closed()
    }

    function commit() {
        var label = nameInput.text.trim()
        applied(label, draftIcon)
        close()
    }

    visible: opacity > 0.01
    opacity: open ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: Theme.animMed; easing.type: Theme.animEase } }

    Rectangle {
        anchors.fill: parent
        color: Theme.scrim
        opacity: 0.42 * editor.opacity
        TapHandler {
            onTapped: editor.close()
        }
    }

    Item {
        id: sheet
        width: parent.width
        height: Math.min(parent.height - Theme.statusBarHeight - Theme.spaceL, 560)
        y: editor.open ? parent.height - height : parent.height

        Behavior on y { NumberAnimation { duration: Theme.animMed; easing.type: Theme.animEase } }

        BackdropBlur {
            anchors.fill: parent
            sourceItem: editor.backdropSource
            sourceRect: Qt.rect(0, sheet.y, width, height)
            blurEnabled: editor.open
            radius: Theme.radiusL
            color: gpu ? Theme.glassBlurHigh : Theme.glassHigh
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.10)
        }

        // Square off the bottom corners against the screen edge.
        Rectangle {
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            height: Theme.radiusL
            color: Theme.glassHigh
        }

        // Swipe down anywhere on the sheet's head to dismiss.
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

        ColumnLayout {
            anchors {
                left: parent.left
                right: parent.right
                top: head.bottom
                bottom: footer.top
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
                bottomMargin: Theme.spaceS
            }
            spacing: Theme.spaceS

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceS

                Text {
                    Layout.fillWidth: true
                    text: "Icon"
                    color: Theme.text
                    font.pixelSize: Theme.fontTitle - 2
                    font.family: Theme.fontFamily
                    font.weight: Font.DemiBold
                }
                Text {
                    text: editor.target && editor.target.surfaceLabel
                        ? String(editor.target.surfaceLabel) : ""
                    color: Theme.textFaint
                    font.pixelSize: Theme.fontStatus - 2
                    font.family: Theme.fontFamily
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
                        onTapped: editor.close()
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceS

                // Live preview of the tile.
                Rectangle {
                    Layout.alignment: Qt.AlignVCenter
                    width: Theme.iconM
                    height: Theme.iconM
                    radius: Theme.radiusM
                    color: Theme.card
                    border.width: 1
                    border.color: Qt.alpha(Theme.text, 0.10)

                    Image {
                        id: previewArt
                        anchors.centerIn: parent
                        width: parent.width * 0.6
                        height: width
                        source: editor.target && editor.target.iconUrl ? String(editor.target.iconUrl) : ""
                        fillMode: Image.PreserveAspectFit
                        asynchronous: true
                        visible: status === Image.Ready && editor.draftIcon.length === 0
                    }
                    MaterialIcon {
                        anchors.centerIn: parent
                        visible: previewArt.status !== Image.Ready
                        name: editor.draftIcon.length > 0
                            ? editor.draftIcon
                            : (editor.target && editor.target.defaultIcon
                                ? String(editor.target.defaultIcon) : "apps")
                        size: Theme.iconS - 6
                        color: Theme.text
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignVCenter
                    height: Theme.iconM - 6
                    radius: Theme.radiusM
                    color: Theme.surface
                    border.width: 1.5
                    border.color: nameInput.activeFocus
                        ? ShellPrefs.accentColor
                        : Qt.alpha(Theme.text, 0.10)

                    Behavior on border.color { ColorAnimation { duration: Theme.animFast } }

                    TextInput {
                        id: nameInput
                        anchors {
                            fill: parent
                            leftMargin: Theme.spaceS
                            rightMargin: Theme.spaceS
                        }
                        color: Theme.text
                        font.pixelSize: Theme.fontBody - 2
                        font.family: Theme.fontFamily
                        verticalAlignment: TextInput.AlignVCenter
                        selectionColor: Qt.alpha(ShellPrefs.accentColor, 0.4)
                        selectedTextColor: Theme.text
                        clip: true
                        onAccepted: editor.commit()
                        Keys.onEscapePressed: editor.close()
                    }

                    Text {
                        anchors {
                            left: parent.left
                            leftMargin: Theme.spaceS
                            verticalCenter: parent.verticalCenter
                        }
                        visible: nameInput.text.length === 0
                        text: editor.target && editor.target.appName
                            ? String(editor.target.appName) : "Name"
                        color: Theme.textFaint
                        font.pixelSize: Theme.fontBody - 2
                        font.family: Theme.fontFamily
                    }
                }
            }

            Text {
                Layout.fillWidth: true
                Layout.topMargin: Theme.spaceXS
                text: "PICK AN ICON"
                color: Theme.textDim
                font.pixelSize: Theme.fontStatus - 2
                font.family: Theme.fontFamily
                font.weight: Font.DemiBold
                font.letterSpacing: 1.4
            }

            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true

                Flow {
                    width: parent.width
                    spacing: Theme.spaceS

                    // "Auto": keep whatever the desktop entry ships with.
                    Rectangle {
                        width: Theme.iconM - 2
                        height: width
                        radius: Theme.radiusM
                        color: editor.draftIcon.length === 0
                            ? Qt.alpha(ShellPrefs.accentColor, 0.22)
                            : Qt.alpha(Theme.text, 0.05)
                        border.width: 1
                        border.color: editor.draftIcon.length === 0
                            ? ShellPrefs.accentColor
                            : Qt.alpha(Theme.text, 0.10)

                        MaterialIcon {
                            anchors.centerIn: parent
                            name: "autorenew"
                            size: Theme.fontCaption
                            color: editor.draftIcon.length === 0
                                ? ShellPrefs.accentColor : Theme.textDim
                        }
                        TapHandler {
                            onTapped: editor.draftIcon = ""
                        }
                    }

                    Repeater {
                        model: editor.glyphs
                        delegate: Rectangle {
                            required property string modelData
                            readonly property bool selected: editor.draftIcon === modelData

                            width: Theme.iconM - 2
                            height: width
                            radius: Theme.radiusM
                            color: selected
                                ? Qt.alpha(ShellPrefs.accentColor, 0.22)
                                : Qt.alpha(Theme.text, 0.05)
                            border.width: 1
                            border.color: selected
                                ? ShellPrefs.accentColor
                                : Qt.alpha(Theme.text, 0.10)

                            Behavior on color { ColorAnimation { duration: Theme.animFast } }

                            MaterialIcon {
                                anchors.centerIn: parent
                                name: modelData
                                size: Theme.fontCaption
                                color: selected ? ShellPrefs.accentColor : Theme.textDim
                            }
                            TapHandler {
                                onTapped: editor.draftIcon = modelData
                            }
                        }
                    }
                }
            }
        }

        RowLayout {
            id: footer
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
                bottomMargin: Theme.spaceM + Theme.navZoneHeight * 0.25
            }
            spacing: Theme.spaceS

            Rectangle {
                implicitWidth: resetTxt.implicitWidth + Theme.spaceL
                implicitHeight: Theme.iconS + 8
                radius: Theme.radiusPill
                color: Qt.alpha(Theme.danger, 0.14)
                border.width: 1
                border.color: Qt.alpha(Theme.danger, 0.30)

                Text {
                    id: resetTxt
                    anchors.centerIn: parent
                    text: "Reset"
                    color: Theme.danger
                    font.pixelSize: Theme.fontCaption - 2
                    font.family: Theme.fontFamily
                    font.weight: Font.DemiBold
                }
                TapHandler {
                    onTapped: {
                        editor.resetRequested()
                        editor.close()
                    }
                }
            }

            Item { Layout.fillWidth: true }

            Rectangle {
                implicitWidth: saveTxt.implicitWidth + Theme.spaceL + 8
                implicitHeight: Theme.iconS + 8
                radius: Theme.radiusPill
                color: ShellPrefs.accentColor
                scale: saveTap.pressed ? 0.95 : 1.0
                Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                Text {
                    id: saveTxt
                    anchors.centerIn: parent
                    text: "Save"
                    color: ShellPrefs.onAccentColor
                    font.pixelSize: Theme.fontCaption - 2
                    font.family: Theme.fontFamily
                    font.weight: Font.DemiBold
                }
                TapHandler {
                    id: saveTap
                    onTapped: editor.commit()
                }
            }
        }
    }
}
