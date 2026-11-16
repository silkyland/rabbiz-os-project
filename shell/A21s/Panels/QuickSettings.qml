import QtQuick
import QtQuick.Layouts
import Quickshell.Io
import A21s.Theme
import A21s.Services
import A21s.Widgets

// Quick-settings sheet of the pull-down shade (One UI 7-style).
//
// Grouped top to bottom so the sheet scans in one pass:
//   1. header round buttons          lock | power, settings
//   2. connection block              2 large pill tiles + one grouped share card
//   3. media block                   player card + vertical brightness/volume
//   4. device strip                  telemetry summary + two round actions
//   5. action grid                   dense icon tiles with labels
//
// A plain Item (not a window): Shade.qml embeds it on the controls page. The
// content scrolls only when the sheet is shorter than the content (landscape,
// gesture-revealed half shade); in portrait everything fits without scrolling,
// which keeps the shade's own pull-to-close gesture working.
Item {
    id: root

    implicitWidth: 400
    implicitHeight: column.implicitHeight

    // Introspection for the shade's `ipc shade state` endpoint: whether the
    // sheet overflows (and therefore takes the vertical drag) at this height.
    readonly property real contentHeight: column.implicitHeight
    readonly property bool scrolling: scroller.interactive

    Process {
        id: settingsProc
        environment: Appearance.appEnvironment
        command: ["a21s-settings"]
    }

    function openPowerMenu() {
        ShellState.shadeProgress = 0
        ShellState.powerMenuOpen = true
    }

    function openSettings() {
        ShellState.shadeProgress = 0
        if (!settingsProc.running)
            settingsProc.running = true
    }

    // The shade has to be gone before the capture, or it photographs itself.
    function takeScreenshot() {
        ShellState.shadeProgress = 0
        SystemControl.takeScreenshot(350)
    }

    function openLastScreenshot() {
        if (SystemControl.lastScreenshotPath.length > 0) {
            ShellState.shadeProgress = 0
            SystemControl.openGallery(SystemControl.lastScreenshotPath)
        } else {
            root.takeScreenshot()
        }
    }

    Flickable {
        id: scroller
        anchors.fill: parent
        clip: true
        contentWidth: width
        contentHeight: Math.max(column.implicitHeight, height)
        boundsBehavior: Flickable.StopAtBounds
        interactive: column.implicitHeight > height

        ColumnLayout {
            id: column
            width: scroller.width
            spacing: 0

            // --- 1. header ------------------------------------------------
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: Theme.spaceXS
                spacing: Theme.spaceS

                RoundButton {
                    materialIcon: "lock"
                    onTapped: SystemControl.lockScreen()
                }

                Item { Layout.fillWidth: true }

                RoundButton {
                    materialIcon: "power_settings_new"
                    onTapped: root.openPowerMenu()
                }
                RoundButton {
                    materialIcon: "settings"
                    onTapped: root.openSettings()
                }
            }

            // --- 2. connections -------------------------------------------
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: Theme.spaceL
                Layout.preferredHeight: Theme.shadeTileHeight * 2 + Theme.spaceS
                spacing: Theme.spaceS

                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: Theme.spaceS

                    QSTile {
                        Layout.fillWidth: true
                        materialIcon: Network.wifiEnabled ? "wifi" : "wifi_off"
                        label: qsTr("Internet")
                        secondary: Network.wifiConnected ? Network.activeSsid
                                 : Network.wifiEnabled ? qsTr("On") : qsTr("Off")
                        active: Network.wifiEnabled
                        onTapped: Network.setWifiEnabled(!Network.wifiEnabled)
                        onLongPressed: {
                            ShellState.connPanelTab = 0
                            ShellState.connPanelOpen = true
                        }
                    }

                    QSTile {
                        Layout.fillWidth: true
                        materialIcon: "signal_cellular_alt"
                        label: qsTr("Mobile data")
                        secondary: SystemControl.airplaneMode ? qsTr("Airplane")
                                 : (!SystemControl.mobileData ? qsTr("Off")
                                    : (ShellPrefs.carrierName + " · "
                                       + ShellPrefs.preferredNetworkMode))
                        active: !SystemControl.airplaneMode && SystemControl.mobileData
                        onTapped: SystemControl.toggleMobileData()
                    }
                }

                // One card, two rows: the device-sharing half of the block.
                // implicitWidth matches the pill tile so the RowLayout gives
                // both halves an equal share instead of collapsing the card.
                Rectangle {
                    implicitWidth: 280
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    radius: Theme.radiusCard
                    color: Theme.shadeCard
                    border.width: 1
                    border.color: Theme.shadeLine

                    ColumnLayout {
                        anchors {
                            fill: parent
                            margins: Theme.spaceXS + 6
                        }
                        spacing: Theme.spaceXS

                        QSRowTile {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            materialIcon: Bluetooth.powered ? "bluetooth" : "bluetooth_disabled"
                            label: qsTr("Bluetooth")
                            secondary: Bluetooth.connected ? Bluetooth.connectedName
                                     : Bluetooth.powered ? qsTr("On") : qsTr("Off")
                            active: Bluetooth.powered
                            onTapped: Bluetooth.setPowered(!Bluetooth.powered)
                            onLongPressed: {
                                ShellState.connPanelTab = 1
                                ShellState.connPanelOpen = true
                            }
                        }

                        QSRowTile {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            materialIcon: "devices"
                            label: qsTr("Devices")
                            secondary: {
                                var list = Bluetooth.btDevices || []
                                var paired = 0
                                for (var i = 0; i < list.length; ++i) {
                                    if (list[i] && list[i].paired)
                                        paired++
                                }
                                return paired > 0
                                    ? paired + qsTr(" paired") : qsTr("None paired")
                            }
                            onTapped: {
                                ShellState.connPanelTab = 1
                                ShellState.connPanelOpen = true
                            }
                        }
                    }
                }
            }

            // --- 3. media + vertical sliders -------------------------------
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: Theme.spaceL
                Layout.preferredHeight: Theme.shadeMediaHeight
                spacing: Theme.spaceS

                Rectangle {
                    id: mediaCard
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    radius: Theme.radiusCard
                    color: Theme.shadeCard
                    border.width: 1
                    border.color: Media.isPlaying
                        ? Qt.alpha(ShellPrefs.accentColor, 0.45) : Theme.shadeLine

                    Behavior on border.color { ColorAnimation { duration: Theme.animFast } }

                    ColumnLayout {
                        anchors {
                            fill: parent
                            margins: Theme.spaceM
                        }
                        spacing: 0

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spaceS

                            MaterialIcon {
                                name: "audiotrack"
                                size: Theme.iconS
                                color: Theme.textDim
                            }

                            Item { Layout.fillWidth: true }

                            // Output/route hint (One UI puts media output here).
                            Rectangle {
                                width: Theme.iconM
                                height: width
                                radius: width / 2
                                color: Theme.shadeWell

                                MaterialIcon {
                                    anchors.centerIn: parent
                                    name: "cast"
                                    size: Theme.fontCaption + 6
                                    color: Theme.text
                                }
                            }
                        }

                        Item { Layout.fillHeight: true }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spaceS

                            Rectangle {
                                visible: art.status === Image.Ready
                                Layout.preferredWidth: Theme.iconL
                                Layout.preferredHeight: Theme.iconL
                                radius: Theme.radiusM
                                clip: true
                                color: Theme.shadeWell

                                Image {
                                    id: art
                                    anchors.fill: parent
                                    source: Media.artUrl
                                    fillMode: Image.PreserveAspectCrop
                                    asynchronous: true
                                }
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0

                                Text {
                                    Layout.fillWidth: true
                                    text: Media.title
                                    color: Media.isPlaying ? Theme.text : Theme.textDim
                                    font.pixelSize: Theme.fontCaption + 2
                                    font.family: Theme.fontFamily
                                    font.weight: Font.DemiBold
                                    elide: Text.ElideRight
                                    maximumLineCount: 1
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: Media.artist
                                    color: Theme.textFaint
                                    font.pixelSize: Theme.fontStatus
                                    font.family: Theme.fontFamily
                                    elide: Text.ElideRight
                                    maximumLineCount: 1
                                }

                                TapHandler {
                                    onTapped: {
                                        ShellState.shadeProgress = 0
                                        Media.openMusicApp()
                                    }
                                }
                            }
                        }

                        Item { Layout.fillHeight: true }

                        // Transport controls, centred like One UI's media pill.
                        Item {
                            Layout.fillWidth: true
                            Layout.preferredHeight: Theme.iconL + Theme.spaceXS

                            Row {
                                anchors.centerIn: parent
                                spacing: Theme.spaceL

                                MaterialIcon {
                                    name: "skip_previous"
                                    size: Theme.fontTitle
                                    color: Theme.text
                                    TapHandler { onTapped: Media.previous() }
                                }

                                Rectangle {
                                    width: Theme.iconL + Theme.spaceXS
                                    height: width
                                    radius: Media.isPlaying ? Theme.radiusM + 6 : width / 2
                                    color: Media.isPlaying ? ShellPrefs.accentColor : Theme.shadeCardOn

                                    Behavior on radius {
                                        NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack }
                                    }
                                    Behavior on color { ColorAnimation { duration: Theme.animFast } }

                                    MaterialIcon {
                                        anchors.centerIn: parent
                                        name: Media.isPlaying ? "pause" : "play_arrow"
                                        size: Theme.fontTitle
                                        color: Media.isPlaying ? ShellPrefs.accentContent : Theme.text
                                    }
                                    TapHandler { onTapped: Media.togglePlaying() }
                                }

                                MaterialIcon {
                                    name: "skip_next"
                                    size: Theme.fontTitle
                                    color: Theme.text
                                    TapHandler { onTapped: Media.next() }
                                }
                            }
                        }
                    }
                }

                QSSliderVertical {
                    Layout.fillHeight: true
                    materialIcon: Brightness.available && Brightness.level > 50
                        ? "brightness_high" : "brightness_medium"
                    interactive: Brightness.available
                    disabledHint: qsTr("No backlight")
                    value: Brightness.level / 100
                    onMoved: function(v) { Brightness.setLevel(Math.round(v * 100)) }
                }

                QSSliderVertical {
                    Layout.fillHeight: true
                    materialIcon: (SystemControl.volumeMuted || SystemControl.volumeLevel === 0)
                        ? "volume_off" : (SystemControl.volumeLevel > 55 ? "volume_up" : "volume_down")
                    interactive: true
                    value: SystemControl.volumeMuted ? 0 : (SystemControl.volumeLevel / 100)
                    onMoved: function(v) {
                        SystemControl.setVolume(Math.round(v * 100))
                    }
                }
            }

            // --- 4. device strip ------------------------------------------
            Rectangle {
                Layout.fillWidth: true
                Layout.topMargin: Theme.spaceL
                Layout.preferredHeight: Theme.shadeTileHeight - Theme.spaceXS
                radius: height / 2
                color: Theme.shadeCard
                border.width: 1
                border.color: Theme.shadeLine

                RowLayout {
                    anchors {
                        fill: parent
                        leftMargin: Theme.spaceM
                        rightMargin: Theme.spaceXS + 2
                    }
                    spacing: Theme.spaceS

                    MaterialIcon {
                        name: "devices_other"
                        size: Theme.iconS
                        color: Theme.text
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0

                        Text {
                            text: qsTr("Device centre")
                            color: Theme.text
                            font.pixelSize: Theme.fontCaption
                            font.family: Theme.fontFamily
                            font.weight: Font.DemiBold
                        }
                        Text {
                            Layout.fillWidth: true
                            text: "CPU " + SystemControl.cpuPercent + "%"
                                  + (SystemControl.diskFreeText !== "--"
                                     ? " · " + SystemControl.diskFreeText + " " + qsTr("free") : "")
                            color: Theme.textDim
                            font.pixelSize: Theme.fontMicro
                            font.family: Theme.fontFamily
                            elide: Text.ElideRight
                        }
                    }

                    RoundButton {
                        materialIcon: SystemControl.lastScreenshotPath.length > 0
                            ? "photo_library" : "photo_camera"
                        size: Theme.iconM + Theme.spaceXS * 2
                        onTapped: root.openLastScreenshot()
                    }
                    RoundButton {
                        materialIcon: "more_vert"
                        size: Theme.iconM + Theme.spaceXS * 2
                        onTapped: root.openSettings()
                    }
                }
            }

            // --- 5. action grid -------------------------------------------
            GridLayout {
                Layout.fillWidth: true
                Layout.topMargin: Theme.spaceM
                columns: 4
                columnSpacing: Theme.spaceXS + 2
                rowSpacing: Theme.spaceL

                QSIconTile {
                    Layout.fillWidth: true
                    materialIcon: "photo_camera"
                    label: qsTr("Screenshot")
                    onTapped: root.takeScreenshot()
                }
                QSIconTile {
                    Layout.fillWidth: true
                    materialIcon: "flashlight_on"
                    label: qsTr("Flashlight")
                    active: SystemControl.flashlightOn
                    onTapped: SystemControl.setFlashlight(!SystemControl.flashlightOn)
                }
                QSIconTile {
                    Layout.fillWidth: true
                    materialIcon: "do_not_disturb_on"
                    label: qsTr("Do not disturb")
                    active: SystemControl.dndEnabled
                    onTapped: SystemControl.setDndEnabled(!SystemControl.dndEnabled)
                }
                QSIconTile {
                    Layout.fillWidth: true
                    materialIcon: "screen_rotation"
                    label: qsTr("Rotate")
                    active: SystemControl.screenLandscape
                    onTapped: SystemControl.toggleLandscape()
                }

                QSIconTile {
                    Layout.fillWidth: true
                    materialIcon: "flight"
                    label: qsTr("Airplane")
                    active: SystemControl.airplaneMode
                    onTapped: SystemControl.setAirplaneMode(!SystemControl.airplaneMode)
                }
                QSIconTile {
                    Layout.fillWidth: true
                    materialIcon: "keyboard"
                    label: qsTr("Keyboard")
                    onTapped: SystemControl.toggleOsk()
                }
                QSIconTile {
                    Layout.fillWidth: true
                    materialIcon: "language"
                    label: qsTr("Language")
                    onTapped: SystemControl.nextOskLanguage()
                }
                QSIconTile {
                    Layout.fillWidth: true
                    materialIcon: "speed"
                    label: qsTr("Perf mode")
                    active: SystemControl.cpuGovernor === "performance"
                    onTapped: SystemControl.toggleGovernor()
                }

                QSIconTile {
                    Layout.fillWidth: true
                    materialIcon: "bedtime"
                    label: qsTr("Sleep")
                    onTapped: SystemControl.sleep()
                }
                QSIconTile {
                    Layout.fillWidth: true
                    materialIcon: "lock"
                    label: qsTr("Lock")
                    onTapped: SystemControl.lockScreen()
                }
            }

        }
    }

    // Round glass button used by the header, the device strip and future sheets.
    component RoundButton: Rectangle {
        id: button

        property string materialIcon: ""
        property color tint: Theme.text
        property int size: Theme.shadeControlButton
        signal tapped()

        width: size
        height: size
        radius: width / 2
        color: Theme.shadeCard

        border.width: 1
        border.color: Theme.shadeLine
        scale: buttonArea.pressed ? 0.94 : 1.0

        Behavior on scale {
            NumberAnimation { duration: Theme.animFast; easing.type: Easing.OutCubic }
        }

        MaterialIcon {
            anchors.centerIn: parent
            name: button.materialIcon
            size: Math.round(button.size * 0.46)
            color: button.tint
        }

        MouseArea {
            id: buttonArea
            anchors.fill: parent
            onClicked: button.tapped()
        }
    }
}
