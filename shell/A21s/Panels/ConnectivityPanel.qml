import Quickshell
import Quickshell.Wayland
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import A21s.Theme
import A21s.Services
import A21s.Widgets

// Android-style full-screen connectivity sheet: Wi-Fi + Bluetooth tabs.
// Opened via ShellState.connPanelOpen / connPanelTab (long-press the QS
// tiles). While open it gates Network.wifiPanelActive / Bluetooth.btPanelActive
// so services only rescan/rediscover on demand. Closes on X, swipe-down,
// Escape, system overlays, or NavBar.goHome() (ShellState.goHome).
PanelWindow {
    id: panel

    readonly property bool open: ShellState.connPanelOpen
    readonly property int tab: ShellState.connPanelTab
    property real enterProgress: open ? 1.0 : 0.0
    property real dragOffset: 0

    function close() { ShellState.connPanelOpen = false }
    function openAt(t) {
        ShellState.connPanelTab = t
        ShellState.connPanelOpen = true
    }
    function setTab(t) { ShellState.connPanelTab = t }

    Behavior on enterProgress {
        NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutQuint }
    }

    anchors { top: true; bottom: true; left: true; right: true }
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "a21s-conn"
    WlrLayershell.keyboardFocus: panel.open ? WlrKeyboardFocus.Exclusive
                                          : WlrKeyboardFocus.None
    color: "transparent"
    visible: panel.open || panel.enterProgress > 0.01

    onOpenChanged: {
        if (open) {
            ShellState.shadeProgress = 0
            Network.refresh()
            Bluetooth.refresh()
            sheetContent.forceActiveFocus()
        } else {
            wifiSheetOpen = false
            btSheetOpen = false
        }
    }

    onVisibleChanged: {
        if (!visible)
            panel.dragOffset = 0
    }

    onTabChanged: {
        if (!open)
            return
        if (tab === 0)
            Network.refresh()
        else
            Bluetooth.refresh()
    }

    // Panel-gated service polling (see Network/Bluetooth singletons).
    Binding { target: Network;   property: "wifiPanelActive"; value: panel.open && panel.tab === 0 }
    Binding { target: Bluetooth; property: "btPanelActive";   value: panel.open && panel.tab === 1 }

    Connections {
        target: ShellState
        function onGoHome() { panel.close() }
        function onLockScreenOpenChanged() {
            if (ShellState.lockScreenOpen)
                panel.close()
        }
        function onPowerMenuOpenChanged() {
            if (ShellState.powerMenuOpen)
                panel.close()
        }
        function onSleepingChanged() {
            if (ShellState.sleeping)
                panel.close()
        }
    }

    // ---- sheets state ---------------------------------------------------
    property bool wifiSheetOpen: false
    property string wifiSheetMode: "pw"      // "pw" | "info"
    property string wifiSheetSsid: ""
    property string wifiSheetSec: ""
    property bool wifiSheetInUse: false
    property bool wifiSheetSaved: false

    property bool btSheetOpen: false
    property string btSheetMac: ""
    property string btSheetName: ""
    property bool btSheetPaired: false
    property bool btSheetConnected: false

    property string toastText: ""
    function showToast(t) {
        toastText = t
        toastTimer.restart()
    }

    Timer {
        id: toastTimer
        interval: 3500
        onTriggered: panel.toastText = ""
    }

    Connections {
        target: Network
        function onWifiLastErrorChanged() {
            if (Network.wifiLastError.length > 0)
                panel.showToast(Network.wifiLastError)
        }
        function onWifiConnectedChanged() {
            if (Network.wifiConnected)
                panel.wifiSheetOpen = false
        }
    }
    Connections {
        target: Bluetooth
        function onBtLastErrorChanged() {
            if (Bluetooth.btLastError.length > 0)
                panel.showToast(Bluetooth.btLastError)
        }
    }

    function openWifiPw(ssid, sec) {
        wifiSheetSsid = ssid
        wifiSheetSec = sec
        wifiSheetMode = "pw"
        wifiSheetOpen = true
        pwField.text = ""
        pwField.forceActiveFocus()
    }

    function openWifiInfo(n) {
        wifiSheetSsid = n.ssid
        wifiSheetSec = n.security
        wifiSheetInUse = n.inUse
        wifiSheetSaved = n.inUse || Network.savedSsids.indexOf(n.ssid) !== -1
        wifiSheetMode = "info"
        wifiSheetOpen = true
    }

    function tapNetwork(n) {
        if (n.inUse) {
            openWifiInfo(n)
        } else if (n.security.length === 0
                   || Network.savedSsids.indexOf(n.ssid) !== -1) {
            Network.wifiConnect(n.ssid, "")
        } else {
            openWifiPw(n.ssid, n.security)
        }
    }

    function openBtSheet(d) {
        btSheetMac = d.mac
        btSheetName = d.name
        btSheetPaired = d.paired
        btSheetConnected = d.connected
        btSheetOpen = true
    }

    function tapBtDevice(d) {
        if (d.connected)
            openBtSheet(d)
        else
            Bluetooth.btConnect(d.mac)
    }

    // Map bluez Icon field / device name to a MaterialIconsRound name.
    function btIconFor(d) {
        var s = ((d.icon || "") + " " + (d.name || "")).toLowerCase()
        if (s.indexOf("headset") !== -1 || s.indexOf("headphone") !== -1
                || s.indexOf("buds") !== -1 || s.indexOf("earbud") !== -1)
            return "headphones"
        if (s.indexOf("keyboard") !== -1)
            return "keyboard"
        if (s.indexOf("mouse") !== -1)
            return "mouse"
        if (s.indexOf("gaming") !== -1 || s.indexOf("xbox") !== -1
                || s.indexOf("controller") !== -1 || s.indexOf("gamepad") !== -1
                || s.indexOf("joy-con") !== -1 || s.indexOf("playstation") !== -1)
            return "sports_esports"
        if (s.indexOf("speaker") !== -1 || s.indexOf("audio") !== -1
                || s.indexOf("sound") !== -1)
            return "speaker"
        if (s.indexOf("watch") !== -1)
            return "watch"
        if (s.indexOf("tablet") !== -1 || s.indexOf("ipad") !== -1)
            return "tablet_android"
        if (s.indexOf("phone") !== -1 || s.indexOf("mobile") !== -1
                || s.indexOf("iphone") !== -1 || s.indexOf("galaxy") !== -1)
            return "phone_android"
        if (s.indexOf("computer") !== -1 || s.indexOf("laptop") !== -1
                || s.indexOf("macbook") !== -1 || s.indexOf("pc") !== -1)
            return "laptop"
        if (s.indexOf("printer") !== -1 || s.indexOf("print") !== -1)
            return "print"
        if (s.indexOf("camera") !== -1 || s.indexOf("cam") !== -1)
            return "photo_camera"
        if (s.indexOf("tv") !== -1 || s.indexOf("television") !== -1)
            return "tv"
        return "bluetooth"
    }

    readonly property var btPaired: Bluetooth.btDevices.filter(
        function(d) { return d.paired })
    readonly property var btAvailable: Bluetooth.btDevices.filter(
        function(d) { return !d.paired })

    // ---- inline mini controls ------------------------------------------
    component ConnSwitch: Rectangle {
        id: sw
        property bool checked: false
        signal toggled(bool on)
        implicitWidth: Theme.iconL + Theme.spaceS
        implicitHeight: Theme.iconS
        radius: Theme.radiusPill
        color: sw.checked ? ShellPrefs.accentColor : Theme.tileOff
        Behavior on color { ColorAnimation { duration: Theme.animFast } }
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            x: sw.checked ? parent.width - width - Theme.spaceXS : Theme.spaceXS
            width: parent.height - 2 * Theme.spaceXS
            height: width
            radius: Theme.radiusPill
            color: Theme.primaryContent
            Behavior on x { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }
        }
        MouseArea {
            anchors.fill: parent
            onClicked: { sw.checked = !sw.checked; sw.toggled(sw.checked) }
        }
    }

    component ConnButton: Rectangle {
        id: btn
        property string text: ""
        property string icon: ""
        property bool accent: true
        property bool danger: false
        signal clicked()
        implicitWidth: btnContent.implicitWidth + Theme.spaceL * 2
        implicitHeight: Theme.iconM
        radius: Theme.radiusPill
        color: btn.danger ? Qt.alpha(Theme.danger, 0.16)
             : btn.accent ? ShellPrefs.accentColor : Theme.tileOff
        opacity: btnArea.pressed ? 0.7 : 1.0
        RowLayout {
            id: btnContent
            anchors.centerIn: parent
            spacing: Theme.spaceXS
            MaterialIcon {
                visible: btn.icon.length > 0
                name: btn.icon
                size: 22
                color: btn.danger ? Theme.danger
                     : btn.accent ? Theme.primaryContent : Theme.tileOffText
            }
            Text {
                text: btn.text
                color: btn.danger ? Theme.danger
                     : btn.accent ? Theme.primaryContent : Theme.tileOffText
                font.pixelSize: Theme.fontStatus + 1
                font.family: Theme.fontFamily
                font.weight: Font.DemiBold
            }
        }
        MouseArea {
            id: btnArea
            anchors.fill: parent
            onClicked: btn.clicked()
        }
    }

    component SectionLabel: Text {
        color: Theme.textDim
        font.pixelSize: Theme.fontStatus
        font.family: Theme.fontFamily
        font.weight: Font.DemiBold
        font.letterSpacing: 1.2
    }

    // ---- the sheet ------------------------------------------------------
    Item {
        id: sheet
        anchors.fill: parent
        y: (1.0 - panel.enterProgress) * sheet.height + panel.dragOffset

        NumberAnimation {
            id: dragBack
            target: panel
            property: "dragOffset"
            to: 0
            duration: Theme.animMed
            easing.type: Theme.animEase
        }

        Rectangle {
            anchors.fill: parent
            color: Theme.bg
        }

        ColumnLayout {
            id: sheetContent
            anchors.fill: parent
            spacing: 0
            focus: true
            Keys.onEscapePressed: panel.close()

            // Grab handle + swipe-down-to-close zone.
            Item {
                Layout.fillWidth: true
                implicitHeight: Theme.statusBarHeight + Theme.shadeHandleZone

                Rectangle {
                    anchors {
                        horizontalCenter: parent.horizontalCenter
                        bottom: parent.bottom
                        bottomMargin: Theme.spaceS
                    }
                    width: Theme.navPillWidth
                    height: Theme.navPillHeight
                    radius: Theme.radiusPill
                    color: Theme.textFaint
                }

                MouseArea {
                    anchors.fill: parent
                    property real pressY: 0
                    property real pressOffset: 0
                    onPressed: function(mouse) {
                        dragBack.stop()
                        pressY = mouse.y
                        pressOffset = panel.dragOffset
                    }
                    onPositionChanged: function(mouse) {
                        if (pressed)
                            panel.dragOffset = Math.max(0,
                                pressOffset + mouse.y - pressY)
                    }
                    onReleased: function(mouse) {
                        if (panel.dragOffset > 160)
                            panel.close()
                        else
                            dragBack.restart()
                    }
                    onCanceled: dragBack.restart()
                }
            }

            // Title row.
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.spaceM
                Layout.rightMargin: Theme.spaceM
                spacing: Theme.spaceS

                Text {
                    Layout.fillWidth: true
                    text: "Connected devices"
                    color: Theme.text
                    font.pixelSize: Theme.fontTitle
                    font.family: Theme.fontFamily
                    font.weight: Font.DemiBold
                }

                Rectangle {
                    Layout.preferredWidth: Theme.iconM
                    Layout.preferredHeight: Theme.iconM
                    radius: Theme.radiusPill
                    color: Theme.surfaceHigh
                    MaterialIcon {
                        anchors.centerIn: parent
                        name: "close"
                        size: 24
                        color: Theme.text
                    }
                    TapHandler { onTapped: panel.close() }
                }
            }

            // Segmented Wi-Fi / Bluetooth tab switch.
            Rectangle {
                Layout.fillWidth: true
                Layout.topMargin: Theme.spaceM
                Layout.leftMargin: Theme.spaceM
                Layout.rightMargin: Theme.spaceM
                implicitHeight: Theme.iconM + Theme.spaceXS
                radius: Theme.radiusPill
                color: Theme.surfaceHigh

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: Theme.spaceXS
                    spacing: Theme.spaceXS

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        radius: Theme.radiusPill
                        color: panel.tab === 0 ? ShellPrefs.accentColor : "transparent"
                        Behavior on color { ColorAnimation { duration: Theme.animFast } }
                        RowLayout {
                            anchors.centerIn: parent
                            spacing: Theme.spaceXS
                            MaterialIcon {
                                name: "wifi"
                                size: 24
                                color: panel.tab === 0 ? ShellPrefs.onAccentColor : Theme.textDim
                            }
                            Text {
                                text: "Wi-Fi"
                                color: panel.tab === 0 ? ShellPrefs.onAccentColor : Theme.textDim
                                font.pixelSize: Theme.fontCaption
                                font.family: Theme.fontFamily
                                font.weight: Font.DemiBold
                            }
                        }
                        TapHandler { onTapped: panel.setTab(0) }
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        radius: Theme.radiusPill
                        color: panel.tab === 1 ? ShellPrefs.accentColor : "transparent"
                        Behavior on color { ColorAnimation { duration: Theme.animFast } }
                        RowLayout {
                            anchors.centerIn: parent
                            spacing: Theme.spaceXS
                            MaterialIcon {
                                name: "bluetooth"
                                size: 24
                                color: panel.tab === 1 ? ShellPrefs.onAccentColor : Theme.textDim
                            }
                            Text {
                                text: "Bluetooth"
                                color: panel.tab === 1 ? ShellPrefs.onAccentColor : Theme.textDim
                                font.pixelSize: Theme.fontCaption
                                font.family: Theme.fontFamily
                                font.weight: Font.DemiBold
                            }
                        }
                        TapHandler { onTapped: panel.setTab(1) }
                    }
                }
            }

            // ---- Wi-Fi page ---------------------------------------------
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.topMargin: Theme.spaceL
                Layout.leftMargin: Theme.spaceM
                Layout.rightMargin: Theme.spaceM
                Layout.bottomMargin: Theme.spaceM
                spacing: Theme.spaceM
                visible: panel.tab === 0

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceS
                    Text {
                        Layout.fillWidth: true
                        text: "Wi-Fi"
                        color: Theme.text
                        font.pixelSize: Theme.fontBody
                        font.family: Theme.fontFamily
                        font.weight: Font.DemiBold
                    }
                    ConnSwitch {
                        checked: Network.wifiEnabled
                        onToggled: function(on) { Network.setWifiEnabled(on) }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    visible: Network.wifiEnabled
                    spacing: Theme.spaceS
                    SectionLabel {
                        Layout.fillWidth: true
                        text: "Available networks"
                    }
                    MaterialIcon {
                        id: wifiScanIcon
                        name: "refresh"
                        size: 24
                        color: Network.wifiScanning ? ShellPrefs.accentColor : Theme.textDim
                        NumberAnimation {
                            target: wifiScanIcon
                            property: "rotation"
                            running: Network.wifiScanning
                            loops: Animation.Infinite
                            from: 0; to: 360; duration: 900
                        }
                    }
                    Text {
                        text: "Rescan"
                        color: ShellPrefs.accentColor
                        font.pixelSize: Theme.fontStatus
                        font.family: Theme.fontFamily
                        TapHandler { onTapped: Network.scan() }
                    }
                }

                Flickable {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    contentWidth: width
                    contentHeight: wifiCol.implicitHeight

                    Column {
                        id: wifiCol
                        width: parent.width
                        spacing: Theme.spaceXS

                        Text {
                            width: wifiCol.width
                            visible: !Network.wifiEnabled
                            text: "Wi-Fi is off. Turn it on to see networks."
                            color: Theme.textDim
                            font.pixelSize: Theme.fontCaption
                            font.family: Theme.fontFamily
                            horizontalAlignment: Text.AlignHCenter
                            topPadding: Theme.spaceXL
                            wrapMode: Text.Wrap
                        }

                        Text {
                            width: wifiCol.width
                            visible: Network.wifiEnabled && Network.wifiNetworks.length === 0
                            text: Network.wifiScanning ? "Searching for networks…"
                                                       : "No networks found"
                            color: Theme.textDim
                            font.pixelSize: Theme.fontCaption
                            font.family: Theme.fontFamily
                            horizontalAlignment: Text.AlignHCenter
                            topPadding: Theme.spaceXL
                        }

                        Repeater {
                            model: Network.wifiNetworks
                            delegate: Rectangle {
                                id: wifiRow
                                required property var modelData
                                width: wifiCol.width
                                implicitHeight: wifiRowCol.implicitHeight + Theme.spaceM
                                radius: Theme.radiusM
                                color: modelData.inUse
                                    ? Qt.alpha(ShellPrefs.accentColor, 0.14)
                                    : Theme.card
                                border.width: modelData.inUse ? 1 : 0
                                border.color: Qt.alpha(ShellPrefs.accentColor, 0.45)

                                readonly property bool connecting:
                                    Network.wifiConnecting
                                    && modelData.ssid === Network.wifiConnectingSsid

                                RowLayout {
                                    anchors {
                                        left: parent.left
                                        right: parent.right
                                        verticalCenter: parent.verticalCenter
                                        leftMargin: Theme.spaceM
                                        rightMargin: Theme.spaceM
                                    }
                                    spacing: Theme.spaceM

                                    MaterialIcon {
                                        name: Network.wifiEnabled ? "wifi" : "wifi_off"
                                        size: 28
                                        color: modelData.inUse ? ShellPrefs.accentColor : Theme.textDim
                                    }

                                    ColumnLayout {
                                        id: wifiRowCol
                                        Layout.fillWidth: true
                                        spacing: 2
                                        Text {
                                            Layout.fillWidth: true
                                            text: wifiRow.modelData.ssid
                                            color: wifiRow.modelData.inUse
                                                ? ShellPrefs.accentColor : Theme.text
                                            font.pixelSize: Theme.fontCaption + 1
                                            font.family: Theme.fontFamily
                                            font.weight: wifiRow.modelData.inUse
                                                ? Font.DemiBold : Font.Normal
                                            elide: Text.ElideRight
                                        }
                                        Text {
                                            Layout.fillWidth: true
                                            text: wifiRow.modelData.inUse ? "Connected"
                                                : wifiRow.connecting ? "Connecting…"
                                                : (Network.savedSsids.indexOf(wifiRow.modelData.ssid) !== -1
                                                    ? "Saved"
                                                    : (wifiRow.modelData.security.length > 0
                                                        ? wifiRow.modelData.security : "Open"))
                                            color: Theme.textDim
                                            font.pixelSize: Theme.fontStatus - 2
                                            font.family: Theme.fontFamily
                                            elide: Text.ElideRight
                                        }
                                    }

                                    MaterialIcon {
                                        visible: wifiRow.modelData.security.length > 0
                                        name: "lock"
                                        size: 18
                                        color: Theme.textFaint
                                    }
                                    Text {
                                        text: wifiRow.modelData.signal >= 0
                                            ? wifiRow.modelData.signal + "%" : ""
                                        color: Theme.textDim
                                        font.pixelSize: Theme.fontStatus - 1
                                        font.family: Theme.fontFamily
                                    }
                                    MaterialIcon {
                                        visible: wifiRow.modelData.inUse
                                        name: "check_circle"
                                        size: 22
                                        color: ShellPrefs.accentColor
                                    }
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    property bool lp: false
                                    onPressed: lp = false
                                    onClicked: {
                                        if (!lp)
                                            panel.tapNetwork(wifiRow.modelData)
                                    }
                                    onPressAndHold: {
                                        lp = true
                                        panel.openWifiInfo(wifiRow.modelData)
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ---- Bluetooth page -----------------------------------------
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.topMargin: Theme.spaceL
                Layout.leftMargin: Theme.spaceM
                Layout.rightMargin: Theme.spaceM
                Layout.bottomMargin: Theme.spaceM
                spacing: Theme.spaceM
                visible: panel.tab === 1

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceS
                    Text {
                        Layout.fillWidth: true
                        text: "Bluetooth"
                        color: Theme.text
                        font.pixelSize: Theme.fontBody
                        font.family: Theme.fontFamily
                        font.weight: Font.DemiBold
                    }
                    ConnSwitch {
                        checked: Bluetooth.powered
                        onToggled: function(on) { Bluetooth.setPowered(on) }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    visible: Bluetooth.powered
                    spacing: Theme.spaceS
                    SectionLabel {
                        Layout.fillWidth: true
                        text: Bluetooth.btScanning ? "Scanning for devices…" : "Devices"
                    }
                    MaterialIcon {
                        id: btScanIcon
                        name: "refresh"
                        size: 24
                        color: Bluetooth.btScanning ? ShellPrefs.accentColor : Theme.textDim
                        NumberAnimation {
                            target: btScanIcon
                            property: "rotation"
                            running: Bluetooth.btScanning
                            loops: Animation.Infinite
                            from: 0; to: 360; duration: 900
                        }
                    }
                    Text {
                        text: "Scan"
                        color: ShellPrefs.accentColor
                        font.pixelSize: Theme.fontStatus
                        font.family: Theme.fontFamily
                        TapHandler { onTapped: Bluetooth.btScan() }
                    }
                }

                Flickable {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    contentWidth: width
                    contentHeight: btCol.implicitHeight

                    Column {
                        id: btCol
                        width: parent.width
                        spacing: Theme.spaceXS

                        Text {
                            width: btCol.width
                            visible: !Bluetooth.powered
                            text: "Bluetooth is off. Turn it on to see devices."
                            color: Theme.textDim
                            font.pixelSize: Theme.fontCaption
                            font.family: Theme.fontFamily
                            horizontalAlignment: Text.AlignHCenter
                            topPadding: Theme.spaceXL
                            wrapMode: Text.Wrap
                        }

                        Text {
                            width: btCol.width
                            visible: Bluetooth.powered && Bluetooth.btDevices.length === 0
                            text: Bluetooth.btScanning ? "Searching for devices…"
                                                       : "No devices found"
                            color: Theme.textDim
                            font.pixelSize: Theme.fontCaption
                            font.family: Theme.fontFamily
                            horizontalAlignment: Text.AlignHCenter
                            topPadding: Theme.spaceXL
                        }

                        SectionLabel {
                            visible: panel.btPaired.length > 0
                            text: "Paired devices"
                            bottomPadding: Theme.spaceXS
                        }

                        Repeater {
                            model: panel.btPaired
                            delegate: btRow
                        }

                        SectionLabel {
                            visible: Bluetooth.powered && panel.btAvailable.length > 0
                            text: "Available devices"
                            topPadding: Theme.spaceS
                            bottomPadding: Theme.spaceXS
                        }

                        Repeater {
                            model: panel.btAvailable
                            delegate: btRow
                        }
                    }
                }
            }
        }

        // Shared BT device row delegate (paired + available lists).
        Component {
            id: btRow
            Rectangle {
                id: btRowItem
                required property var modelData
                width: btCol.width
                implicitHeight: btRowCol.implicitHeight + Theme.spaceM
                radius: Theme.radiusM
                color: modelData.connected
                    ? Qt.alpha(ShellPrefs.accentColor, 0.14) : Theme.card
                border.width: modelData.connected ? 1 : 0
                border.color: Qt.alpha(ShellPrefs.accentColor, 0.45)

                readonly property bool busy:
                    Bluetooth.btBusy && modelData.mac === Bluetooth.btBusyMac

                RowLayout {
                    anchors {
                        left: parent.left
                        right: parent.right
                        verticalCenter: parent.verticalCenter
                        leftMargin: Theme.spaceM
                        rightMargin: Theme.spaceM
                    }
                    spacing: Theme.spaceM

                    Rectangle {
                        Layout.preferredWidth: Theme.iconM + 4
                        Layout.preferredHeight: Theme.iconM + 4
                        radius: (Theme.iconM + 4) / 2
                        color: Theme.surfaceHigh
                        MaterialIcon {
                            anchors.centerIn: parent
                            name: panel.btIconFor(btRowItem.modelData)
                            size: 30
                            color: btRowItem.modelData.connected
                                ? ShellPrefs.accentColor : Theme.text
                        }
                    }

                    ColumnLayout {
                        id: btRowCol
                        Layout.fillWidth: true
                        spacing: 2
                        Text {
                            Layout.fillWidth: true
                            text: btRowItem.modelData.name
                            color: btRowItem.modelData.connected
                                ? ShellPrefs.accentColor : Theme.text
                            font.pixelSize: Theme.fontCaption + 1
                            font.family: Theme.fontFamily
                            font.weight: btRowItem.modelData.connected
                                ? Font.DemiBold : Font.Normal
                            elide: Text.ElideRight
                        }
                        Text {
                            Layout.fillWidth: true
                            text: btRowItem.busy ? "Working…"
                                : btRowItem.modelData.connected
                                ? ("Connected"
                                   + (btRowItem.modelData.battery >= 0
                                      ? " · " + btRowItem.modelData.battery + "%" : ""))
                                : btRowItem.modelData.paired ? "Paired"
                                : (btRowItem.modelData.rssi > -999
                                   ? "Signal " + btRowItem.modelData.rssi + " dBm"
                                   : "Not paired")
                            color: Theme.textDim
                            font.pixelSize: Theme.fontStatus - 2
                            font.family: Theme.fontFamily
                            elide: Text.ElideRight
                        }
                    }

                    MaterialIcon {
                        visible: btRowItem.modelData.connected
                        name: "check_circle"
                        size: 22
                        color: ShellPrefs.accentColor
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    property bool lp: false
                    onPressed: lp = false
                    onClicked: {
                        if (!lp)
                            panel.tapBtDevice(btRowItem.modelData)
                    }
                    onPressAndHold: {
                        lp = true
                        panel.openBtSheet(btRowItem.modelData)
                    }
                }
            }
        }

        // ---- Wi-Fi sheet (password entry / network actions) -------------
        Item {
            id: wifiSheetWrap
            anchors.fill: parent
            visible: panel.wifiSheetOpen || wifiSheet.rise < 0.99

            Rectangle {
                anchors.fill: parent
                color: Theme.scrim
                opacity: 1.0 - wifiSheet.rise
                MouseArea {
                    anchors.fill: parent
                    onClicked: panel.wifiSheetOpen = false
                }
            }

            Rectangle {
                id: wifiSheet
                anchors {
                    left: parent.left
                    right: parent.right
                    bottom: parent.bottom
                    bottomMargin: -Theme.radiusL
                }
                property real rise: panel.wifiSheetOpen ? 0.0 : 1.0
                Behavior on rise { NumberAnimation { duration: Theme.animMed; easing.type: Theme.animEase } }
                transform: Translate { y: wifiSheet.rise * wifiSheet.height }
                implicitHeight: wifiSheetCol.implicitHeight + Theme.spaceL * 2 + Theme.radiusL
                radius: Theme.radiusL
                color: Theme.surfaceHigh

                MouseArea { anchors.fill: parent }   // swallow taps

                ColumnLayout {
                    id: wifiSheetCol
                    anchors {
                        left: parent.left
                        right: parent.right
                        top: parent.top
                        leftMargin: Theme.spaceL
                        rightMargin: Theme.spaceL
                        topMargin: Theme.spaceL
                    }
                    spacing: Theme.spaceM

                    Text {
                        Layout.fillWidth: true
                        text: panel.wifiSheetMode === "pw"
                            ? "Connect to " + panel.wifiSheetSsid
                            : panel.wifiSheetSsid
                        color: Theme.text
                        font.pixelSize: Theme.fontTitle - 4
                        font.family: Theme.fontFamily
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                        maximumLineCount: 2
                        wrapMode: Text.Wrap
                    }

                    Text {
                        Layout.fillWidth: true
                        visible: panel.wifiSheetMode === "info"
                        text: panel.wifiSheetInUse ? "Connected"
                            : panel.wifiSheetSaved ? "Saved" : "Available"
                        color: Theme.textDim
                        font.pixelSize: Theme.fontStatus
                        font.family: Theme.fontFamily
                    }

                    TextField {
                        id: pwField
                        Layout.fillWidth: true
                        visible: panel.wifiSheetMode === "pw"
                        echoMode: TextInput.PasswordEchoOnEdit
                        inputMethodHints: Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                                          | Qt.ImhSensitiveData
                        placeholderText: "Password"
                        placeholderTextColor: Theme.textFaint
                        color: Theme.text
                        font.pixelSize: Theme.fontBody
                        font.family: Theme.fontFamily
                        background: Rectangle {
                            color: Theme.surface
                            radius: Theme.radiusS
                            border.width: pwField.activeFocus ? 1 : 0
                            border.color: ShellPrefs.accentColor
                        }
                        onAccepted: connectBtn.clicked()
                    }

                    // Password-mode actions
                    RowLayout {
                        Layout.fillWidth: true
                        visible: panel.wifiSheetMode === "pw"
                        spacing: Theme.spaceM
                        Item { Layout.fillWidth: true }
                        ConnButton {
                            text: "Cancel"
                            accent: false
                            onClicked: panel.wifiSheetOpen = false
                        }
                        ConnButton {
                            id: connectBtn
                            text: Network.wifiConnecting
                                  && Network.wifiConnectingSsid === panel.wifiSheetSsid
                                ? "Connecting…" : "Connect"
                            onClicked: {
                                Network.wifiConnect(panel.wifiSheetSsid, pwField.text)
                                panel.wifiSheetOpen = false
                            }
                        }
                    }

                    // Info-mode actions
                    ColumnLayout {
                        Layout.fillWidth: true
                        visible: panel.wifiSheetMode === "info"
                        spacing: Theme.spaceS

                        ConnButton {
                            Layout.fillWidth: true
                            visible: !panel.wifiSheetInUse
                            text: "Connect"
                            icon: "wifi"
                            onClicked: {
                                Network.wifiConnect(panel.wifiSheetSsid, "")
                                panel.wifiSheetOpen = false
                            }
                        }
                        ConnButton {
                            Layout.fillWidth: true
                            visible: panel.wifiSheetInUse
                            text: "Disconnect"
                            icon: "wifi_off"
                            accent: false
                            onClicked: {
                                Network.disconnect()
                                panel.wifiSheetOpen = false
                            }
                        }
                        ConnButton {
                            Layout.fillWidth: true
                            visible: panel.wifiSheetSaved
                            text: "Forget"
                            icon: "delete"
                            danger: true
                            onClicked: {
                                Network.wifiForget(panel.wifiSheetSsid)
                                panel.wifiSheetOpen = false
                            }
                        }
                        ConnButton {
                            Layout.fillWidth: true
                            text: "Cancel"
                            accent: false
                            onClicked: panel.wifiSheetOpen = false
                        }
                    }
                }
            }
        }

        // ---- Bluetooth device action sheet ------------------------------
        Item {
            id: btSheetWrap
            anchors.fill: parent
            visible: panel.btSheetOpen || btSheet.rise < 0.99

            Rectangle {
                anchors.fill: parent
                color: Theme.scrim
                opacity: 1.0 - btSheet.rise
                MouseArea {
                    anchors.fill: parent
                    onClicked: panel.btSheetOpen = false
                }
            }

            Rectangle {
                id: btSheet
                anchors {
                    left: parent.left
                    right: parent.right
                    bottom: parent.bottom
                    bottomMargin: -Theme.radiusL
                }
                property real rise: panel.btSheetOpen ? 0.0 : 1.0
                Behavior on rise { NumberAnimation { duration: Theme.animMed; easing.type: Theme.animEase } }
                transform: Translate { y: btSheet.rise * btSheet.height }
                implicitHeight: btSheetCol.implicitHeight + Theme.spaceL * 2 + Theme.radiusL
                radius: Theme.radiusL
                color: Theme.surfaceHigh

                MouseArea { anchors.fill: parent }   // swallow taps

                ColumnLayout {
                    id: btSheetCol
                    anchors {
                        left: parent.left
                        right: parent.right
                        top: parent.top
                        leftMargin: Theme.spaceL
                        rightMargin: Theme.spaceL
                        topMargin: Theme.spaceL
                    }
                    spacing: Theme.spaceM

                    Text {
                        Layout.fillWidth: true
                        text: panel.btSheetName
                        color: Theme.text
                        font.pixelSize: Theme.fontTitle - 4
                        font.family: Theme.fontFamily
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                        maximumLineCount: 2
                        wrapMode: Text.Wrap
                    }
                    Text {
                        Layout.fillWidth: true
                        text: panel.btSheetMac
                          + (panel.btSheetConnected ? " · Connected"
                             : panel.btSheetPaired ? " · Paired" : "")
                        color: Theme.textDim
                        font.pixelSize: Theme.fontStatus
                        font.family: Theme.fontFamily
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spaceS

                        ConnButton {
                            Layout.fillWidth: true
                            text: panel.btSheetConnected ? "Disconnect" : "Connect"
                            icon: "bluetooth"
                            onClicked: {
                                if (panel.btSheetConnected)
                                    Bluetooth.btDisconnect(panel.btSheetMac)
                                else
                                    Bluetooth.btConnect(panel.btSheetMac)
                                panel.btSheetOpen = false
                            }
                        }
                        ConnButton {
                            Layout.fillWidth: true
                            visible: panel.btSheetPaired
                            text: "Forget this device"
                            icon: "delete"
                            danger: true
                            onClicked: {
                                Bluetooth.btRemove(panel.btSheetMac)
                                panel.btSheetOpen = false
                            }
                        }
                        ConnButton {
                            Layout.fillWidth: true
                            text: "Cancel"
                            accent: false
                            onClicked: panel.btSheetOpen = false
                        }
                    }
                }
            }
        }

        // ---- error / status toast ---------------------------------------
        Rectangle {
            anchors {
                horizontalCenter: parent.horizontalCenter
                bottom: parent.bottom
                bottomMargin: Theme.navZoneHeight + Theme.spaceL
            }
            visible: panel.toastText.length > 0
            implicitWidth: Math.min(toastLbl.implicitWidth + Theme.spaceL * 2,
                                    parent.width - Theme.spaceM * 2)
            implicitHeight: toastLbl.implicitHeight + Theme.spaceM
            radius: Theme.radiusPill
            color: Qt.alpha(Theme.surfaceHigh, 0.97)
            border.width: 1
            border.color: Qt.alpha(Theme.danger, 0.5)
            opacity: panel.toastText.length > 0 ? 1.0 : 0.0
            Behavior on opacity { NumberAnimation { duration: Theme.animFast } }

            Text {
                id: toastLbl
                anchors.centerIn: parent
                width: Math.min(implicitWidth, parent.width - Theme.spaceL * 2)
                text: panel.toastText
                color: Theme.text
                font.pixelSize: Theme.fontStatus
                font.family: Theme.fontFamily
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
                maximumLineCount: 2
                wrapMode: Text.Wrap
            }
        }
    }
}
