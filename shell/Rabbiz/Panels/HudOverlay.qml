import Quickshell
import Quickshell.Wayland
import QtQuick
import QtQuick.Layouts
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets

// Compact non-blocking HUD overlay at bottom of screen for:
//   1. Hardware / slider Volume OSD with live interactive QSSlider
//   2. Screenshot preview toast with 1-tap launch into Gallery
PanelWindow {
    id: hud

    anchors { bottom: true; left: true; right: true }
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "a21s-hud"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    color: "transparent"

    implicitHeight: 240
    visible: SystemControl.volumeOsdVisible || SystemControl.screenshotToastVisible

    ColumnLayout {
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            leftMargin: Theme.spaceM
            rightMargin: Theme.spaceM
            bottomMargin: Theme.navZoneHeight + Theme.spaceS
        }
        spacing: Theme.spaceS

        // ---- Screenshot Preview Toast ----
        Rectangle {
            visible: SystemControl.screenshotToastVisible
            Layout.fillWidth: true
            implicitHeight: 112
            radius: Theme.radiusL
            color: Qt.alpha(Theme.surface, 0.96)
            border.width: 1
            border.color: Qt.alpha("#A855F7", 0.65)

            RowLayout {
                anchors {
                    fill: parent
                    margins: Theme.spaceS
                }
                spacing: Theme.spaceM

                Rectangle {
                    Layout.preferredWidth: 56
                    Layout.preferredHeight: 88
                    radius: Theme.radiusS
                    color: Theme.card
                    border.width: 1
                    border.color: Theme.separator
                    clip: true

                    Image {
                        anchors.fill: parent
                        source: SystemControl.lastScreenshotPath.length > 0
                            ? ("file://" + SystemControl.lastScreenshotPath)
                            : ""
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2

                    Text {
                        text: "บันทึกในคลังภาพแล้ว"
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontCaption
                        font.bold: true
                        color: Theme.text
                    }
                    Text {
                        Layout.fillWidth: true
                        text: SystemControl.lastScreenshotName || "Screenshot"
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontStatus - 1
                        color: Theme.textDim
                        elide: Text.ElideMiddle
                    }
                }

                Rectangle {
                    implicitWidth: openGalRow.implicitWidth + 24
                    height: 52
                    radius: Theme.radiusPill
                    color: Theme.primary

                    RowLayout {
                        id: openGalRow
                        anchors.centerIn: parent
                        spacing: 6
                        MaterialIcon {
                            name: "photo_library"
                            size: 22
                            color: Theme.primaryContent
                        }
                        Text {
                            text: "ดูภาพ"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontStatus
                            font.bold: true
                            color: Theme.primaryContent
                        }
                    }

                    MouseArea {
                        anchors.fill: parent
                        onClicked: SystemControl.openGallery(SystemControl.lastScreenshotPath)
                    }
                }
            }
        }

        // ---- Interactive Volume OSD ----
        Rectangle {
            visible: SystemControl.volumeOsdVisible
            Layout.fillWidth: true
            implicitHeight: volCol.implicitHeight + Theme.spaceM * 1.4
            radius: Theme.radiusL
            color: Qt.alpha(Theme.surface, 0.96)
            border.width: 1
            border.color: Qt.alpha(ShellPrefs.accentColor, 0.45)

            ColumnLayout {
                id: volCol
                anchors {
                    left: parent.left
                    right: parent.right
                    verticalCenter: parent.verticalCenter
                    leftMargin: Theme.spaceM
                    rightMargin: Theme.spaceM
                }
                spacing: Theme.spaceS

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceS

                    MaterialIcon {
                        name: (SystemControl.volumeMuted || SystemControl.volumeLevel === 0)
                            ? "volume_off"
                            : (SystemControl.volumeLevel < 45 ? "volume_down" : "volume_up")
                        size: 28
                        color: SystemControl.volumeMuted ? Theme.danger : ShellPrefs.accentColor
                    }

                    Text {
                        text: SystemControl.volumeMuted ? "ระดับเสียง (Muted)" : "ระดับเสียง (Volume)"
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontCaption
                        font.bold: true
                        color: Theme.text
                        Layout.fillWidth: true
                    }

                    // Step -
                    Rectangle {
                        width: 44; height: 38; radius: 19
                        color: Theme.surfaceHigh
                        Text {
                            anchors.centerIn: parent
                            text: "-"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontBody
                            font.bold: true
                            color: Theme.text
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: SystemControl.adjustVolume(-5)
                        }
                    }

                    // Step +
                    Rectangle {
                        width: 44; height: 38; radius: 19
                        color: Theme.surfaceHigh
                        Text {
                            anchors.centerIn: parent
                            text: "+"
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontBody
                            font.bold: true
                            color: Theme.text
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: SystemControl.adjustVolume(5)
                        }
                    }

                    Text {
                        text: SystemControl.volumeLevel + "%"
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontCaption
                        font.bold: true
                        color: SystemControl.volumeMuted ? Theme.danger : ShellPrefs.accentColor
                        horizontalAlignment: Text.AlignRight
                        Layout.minimumWidth: 52
                    }
                }

                QSSlider {
                    Layout.fillWidth: true
                    materialIcon: (SystemControl.volumeMuted || SystemControl.volumeLevel === 0)
                        ? "volume_off" : "volume_up"
                    interactive: true
                    value: SystemControl.volumeMuted ? 0 : (SystemControl.volumeLevel / 100)
                    onMoved: function(v) {
                        SystemControl.setVolume(Math.round(v * 100))
                    }
                    onIconTapped: SystemControl.toggleMute()
                }
            }
        }
    }
}
