import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell.Io
import A21s.Theme
import A21s.Services
import A21s.Components as Components

// Standalone Settings app — launched by the `a21s-settings` wrapper
// (QML2_IMPORT_PATH=/usr/share/a21s-shell exec qml6 Settings.qml).
// labwc maximizes this window automatically (Android-style fullscreen).
ApplicationWindow {
    id: win
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    title: "Settings"
    color: Theme.bg
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontBody

    // ------------------------------------------------------------------
    // Shared mini-controls (inline components, Theme-styled, no Controls
    // styling dependency).
    // ------------------------------------------------------------------
    component PillSwitch: Rectangle {
        id: sw
        property bool checked: false
        signal toggled(bool on)
        implicitWidth: Theme.iconL + Theme.spaceS
        implicitHeight: Theme.iconS
        radius: Theme.radiusPill
        color: sw.checked ? Theme.primary : Theme.tileOff
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

    component AButton: Rectangle {
        id: btn
        property string text: ""
        property bool accent: true
        signal clicked()
        implicitWidth: btnLabel.implicitWidth + Theme.spaceL * 2
        implicitHeight: Theme.iconM
        radius: Theme.radiusPill
        color: btn.accent ? Theme.primary : Theme.tileOff
        opacity: btnArea.pressed ? 0.7 : 1.0
        Text {
            id: btnLabel
            anchors.centerIn: parent
            text: btn.text
            color: btn.accent ? Theme.primaryContent : Theme.tileOffText
            font.pixelSize: Theme.fontBody
        }
        MouseArea {
            id: btnArea
            anchors.fill: parent
            onClicked: btn.clicked()
        }
    }

    component SectionTitle: Text {
        color: Theme.textDim
        font.pixelSize: Theme.fontCaption
    }

    // ------------------------------------------------------------------
    // Header
    // ------------------------------------------------------------------
    header: Rectangle {
        height: Theme.iconL + Theme.spaceS
        color: Theme.surface
        RowLayout {
            anchors {
                fill: parent
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
            }
            spacing: Theme.spaceM
            Text {
                visible: stack.depth > 1
                text: "‹"
                color: Theme.primary
                font.pixelSize: Theme.fontHeadline
                Layout.preferredWidth: Theme.iconS
                MouseArea { anchors.fill: parent; onClicked: stack.pop() }
            }
            Text {
                Layout.fillWidth: true
                text: stack.currentItem && stack.currentItem.pageTitle !== undefined
                      ? stack.currentItem.pageTitle : "Settings"
                color: Theme.text
                font.pixelSize: Theme.fontTitle
                elide: Text.ElideRight
            }
        }
    }

    StackView {
        id: stack
        anchors.fill: parent
        initialItem: mainPage
    }

    function openPage(key) {
        if (key === "wifi")        stack.push(wifiPage)
        else if (key === "bt")     stack.push(btPage)
        else if (key === "vol")    stack.push(volPage)
        else if (key === "bright") stack.push(brightPage)
        else if (key === "about")  stack.push(aboutPage)
    }

    // ------------------------------------------------------------------
    // Main page
    // ------------------------------------------------------------------
    Component {
        id: mainPage
        Item {
            property string pageTitle: "Settings"
            ListView {
                anchors {
                    fill: parent
                    margins: Theme.spaceM
                }
                spacing: Theme.spaceS
                model: [
                    { "name": "Wi-Fi",      "key": "wifi" },
                    { "name": "Bluetooth",  "key": "bt" },
                    { "name": "Volume",     "key": "vol" },
                    { "name": "Brightness", "key": "bright" },
                    { "name": "About",      "key": "about" }
                ]
                delegate: Rectangle {
                    required property var modelData
                    width: ListView.view.width
                    implicitHeight: Theme.iconL + Theme.spaceM
                    radius: Theme.radiusM
                    color: Theme.card
                    RowLayout {
                        anchors {
                            fill: parent
                            leftMargin: Theme.spaceM
                            rightMargin: Theme.spaceM
                        }
                        Text {
                            Layout.fillWidth: true
                            text: modelData.name
                            color: Theme.text
                            font.pixelSize: Theme.fontBody
                        }
                        Text {
                            text: "›"
                            color: Theme.textFaint
                            font.pixelSize: Theme.fontTitle
                        }
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: win.openPage(modelData.key)
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Wi-Fi page: radio switch + AP list + connect dialog
    // ------------------------------------------------------------------
    Component {
        id: wifiPage
        Item {
            property string pageTitle: "Wi-Fi"
            ColumnLayout {
                anchors {
                    fill: parent
                    margins: Theme.spaceM
                }
                spacing: Theme.spaceM

                RowLayout {
                    Layout.fillWidth: true
                    Text {
                        Layout.fillWidth: true
                        text: "Wi-Fi"
                        color: Theme.text
                        font.pixelSize: Theme.fontBody
                    }
                    PillSwitch {
                        checked: Network.wifiEnabled
                        onToggled: function(on) { Network.setWifiEnabled(on) }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    SectionTitle {
                        Layout.fillWidth: true
                        text: "Networks"
                    }
                    Text {
                        text: "Rescan"
                        color: Theme.primary
                        font.pixelSize: Theme.fontCaption
                        MouseArea {
                            anchors.fill: parent
                            onClicked: Network.scan()
                        }
                    }
                }

                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: Theme.spaceXS
                    model: Network.networks
                    delegate: Rectangle {
                        required property var modelData
                        width: ListView.view.width
                        implicitHeight: netCol.implicitHeight + Theme.spaceM
                        radius: Theme.radiusS
                        color: Theme.card
                        RowLayout {
                            anchors {
                                fill: parent
                                leftMargin: Theme.spaceM
                                rightMargin: Theme.spaceM
                            }
                            ColumnLayout {
                                id: netCol
                                Layout.fillWidth: true
                                spacing: Theme.spaceXS
                                Text {
                                    Layout.fillWidth: true
                                    text: modelData.ssid
                                    color: modelData.inUse ? Theme.primary : Theme.text
                                    font.pixelSize: Theme.fontBody
                                    elide: Text.ElideRight
                                }
                                Text {
                                    text: modelData.inUse ? "Connected"
                                        : modelData.security.length > 0 ? modelData.security
                                        : "Open"
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontStatus
                                }
                            }
                            Text {
                                text: (modelData.security.length > 0 ? "🔒 " : "")
                                      + modelData.signal + "%"
                                color: Theme.textDim
                                font.pixelSize: Theme.fontCaption
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: wifiDialog.open(
                                modelData.inUse ? "disconnect" : "connect",
                                modelData.ssid)
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Bluetooth page
    // ------------------------------------------------------------------
    Component {
        id: btPage
        Item {
            property string pageTitle: "Bluetooth"
            ColumnLayout {
                anchors {
                    fill: parent
                    margins: Theme.spaceM
                }
                spacing: Theme.spaceM

                RowLayout {
                    Layout.fillWidth: true
                    Text {
                        Layout.fillWidth: true
                        text: "Bluetooth"
                        color: Theme.text
                        font.pixelSize: Theme.fontBody
                    }
                    PillSwitch {
                        checked: Bluetooth.powered
                        onToggled: function(on) { Bluetooth.setPowered(on) }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    SectionTitle {
                        Layout.fillWidth: true
                        text: "Devices"
                    }
                    Text {
                        text: "Refresh"
                        color: Theme.primary
                        font.pixelSize: Theme.fontCaption
                        MouseArea {
                            anchors.fill: parent
                            onClicked: Bluetooth.refresh()
                        }
                    }
                }

                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: Theme.spaceXS
                    model: Bluetooth.devices
                    delegate: Rectangle {
                        required property var modelData
                        width: ListView.view.width
                        implicitHeight: btCol.implicitHeight + Theme.spaceM
                        radius: Theme.radiusS
                        color: Theme.card
                        ColumnLayout {
                            id: btCol
                            anchors {
                                left: parent.left
                                right: parent.right
                                verticalCenter: parent.verticalCenter
                                leftMargin: Theme.spaceM
                                rightMargin: Theme.spaceM
                            }
                            spacing: Theme.spaceXS
                            Text {
                                Layout.fillWidth: true
                                text: modelData.name
                                color: modelData.connected ? Theme.primary : Theme.text
                                font.pixelSize: Theme.fontBody
                                elide: Text.ElideRight
                            }
                            Text {
                                text: modelData.mac
                                      + (modelData.connected ? " · connected"
                                      : modelData.paired ? " · paired" : "")
                                color: Theme.textDim
                                font.pixelSize: Theme.fontStatus
                            }
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Volume page — wpctl (pipewire) via Process
    // ------------------------------------------------------------------
    Component {
        id: volPage
        Item {
            id: volPageRoot
            property string pageTitle: "Volume"
            property real volValue: 0
            property bool volMuted: false
            property bool volAvail: false
            property real _pendingVol: -1

            function readVol() {
                if (!volRead.running)
                    volRead.running = true
            }

            Process {
                id: volRead
                command: ["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"]
                stdout: StdioCollector {
                    onStreamFinished: {
                        var m = /Volume:\s*([0-9.]+)(.*)/.exec(this.text)
                        if (m !== null) {
                            volPageRoot.volValue = Math.min(1, parseFloat(m[1]))
                            volPageRoot.volMuted = /MUTED/.test(m[2])
                            volPageRoot.volAvail = true
                        } else {
                            volPageRoot.volAvail = false
                        }
                    }
                }
            }

            Process { id: volSet }

            // Debounce writes while the slider is dragged.
            Timer {
                id: volDebounce
                interval: 120
                onTriggered: {
                    if (!volSet.running && volPageRoot._pendingVol >= 0) {
                        volSet.command = ["wpctl", "set-volume",
                            "@DEFAULT_AUDIO_SINK@",
                            Math.round(volPageRoot._pendingVol * 100) + "%"]
                        volSet.running = true
                        volPost.restart()
                    }
                }
            }

            Timer {
                id: volPost
                interval: 400
                onTriggered: volPageRoot.readVol()
            }

            Timer {
                interval: 3000
                running: true
                repeat: true
                onTriggered: volPageRoot.readVol()
            }

            Component.onCompleted: readVol()

            ColumnLayout {
                anchors {
                    fill: parent
                    margins: Theme.spaceM
                }
                spacing: Theme.spaceM

                SectionTitle { text: "Media volume" }

                Components.QSSlider {
                    Layout.fillWidth: true
                    glyph: volPageRoot.volMuted ? "🔇" : "🔊"
                    interactive: volPageRoot.volAvail
                    disabledHint: "no audio sink"
                    value: volPageRoot.volValue
                    onMoved: function(v) {
                        volPageRoot._pendingVol = v
                        volDebounce.restart()
                    }
                    onIconTapped: {
                        if (!volSet.running) {
                            volSet.command = ["wpctl", "set-mute",
                                "@DEFAULT_AUDIO_SINK@", "toggle"]
                            volSet.running = true
                            volPost.restart()
                        }
                    }
                }

                Text {
                    text: volPageRoot.volAvail
                          ? (volPageRoot.volMuted ? "Muted" : "")
                          : "No audio sink found (wpctl/PipeWire unavailable)."
                    color: Theme.textDim
                    font.pixelSize: Theme.fontCaption
                }

                Item { Layout.fillHeight: true }
            }
        }
    }

    // ------------------------------------------------------------------
    // Brightness page
    // ------------------------------------------------------------------
    Component {
        id: brightPage
        Item {
            property string pageTitle: "Brightness"
            ColumnLayout {
                anchors {
                    fill: parent
                    margins: Theme.spaceM
                }
                spacing: Theme.spaceM

                SectionTitle { text: "Display brightness" }

                Components.QSSlider {
                    Layout.fillWidth: true
                    glyph: "🔆"
                    interactive: Brightness.available
                    disabledHint: "no backlight device"
                    value: Brightness.level / 100
                    onMoved: function(v) { Brightness.setLevel(Math.round(v * 100)) }
                }

                Text {
                    visible: !Brightness.available
                    text: "No backlight device found yet — display driver pending (M5)."
                    color: Theme.textDim
                    font.pixelSize: Theme.fontCaption
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }

                Item { Layout.fillHeight: true }
            }
        }
    }

    // ------------------------------------------------------------------
    // About page — uname/os-release/uptime via Process
    // ------------------------------------------------------------------
    Component {
        id: aboutPage
        Item {
            id: aboutPageRoot
            property string pageTitle: "About"
            property string unameText: "…"
            property string osText: ""
            property string upText: ""

            Process {
                id: aboutProc
                command: ["sh", "-c",
                    "uname -a; echo '==='; "
                  + "grep PRETTY_NAME /etc/os-release 2>/dev/null; echo '==='; "
                  + "uptime -p 2>/dev/null || uptime"]
                stdout: StdioCollector {
                    onStreamFinished: {
                        var secs = this.text.split("===")
                        if (secs.length > 0)
                            aboutPageRoot.unameText = secs[0].trim()
                        if (secs.length > 1)
                            aboutPageRoot.osText = secs[1].trim()
                                .replace(/^PRETTY_NAME=/, "").replace(/"/g, "")
                        if (secs.length > 2)
                            aboutPageRoot.upText = secs[2].trim()
                    }
                }
            }

            Component.onCompleted: aboutProc.running = true

            ColumnLayout {
                anchors {
                    fill: parent
                    margins: Theme.spaceM
                }
                spacing: Theme.spaceM

                Text {
                    text: "a21s-shell"
                    color: Theme.text
                    font.pixelSize: Theme.fontHeadline
                }
                Text {
                    text: "Android-like shell for Galaxy A21s (SM-A217F)\n"
                        + "Quickshell + labwc on mainline Linux"
                    color: Theme.textDim
                    font.pixelSize: Theme.fontBody
                }

                SectionTitle { text: "System" }
                Text {
                    Layout.fillWidth: true
                    text: aboutPageRoot.unameText
                    color: Theme.text
                    font.pixelSize: Theme.fontCaption
                    wrapMode: Text.Wrap
                }
                Text {
                    visible: text.length > 0
                    text: aboutPageRoot.osText
                    color: Theme.textDim
                    font.pixelSize: Theme.fontCaption
                }
                Text {
                    visible: text.length > 0
                    text: "Uptime: " + aboutPageRoot.upText
                    color: Theme.textDim
                    font.pixelSize: Theme.fontCaption
                }

                Item { Layout.fillHeight: true }
            }
        }
    }

    // ------------------------------------------------------------------
    // Wi-Fi connect / disconnect dialog (window-level overlay)
    // ------------------------------------------------------------------
    Rectangle {
        id: wifiDialog
        anchors.fill: parent
        visible: false
        color: Theme.scrim

        property string mode: "connect"   // "connect" | "disconnect"
        property string ssid: ""

        function open(m, s) {
            mode = m
            ssid = s
            pwField.text = ""
            visible = true
            if (m === "connect")
                pwField.forceActiveFocus()
        }

        MouseArea {
            anchors.fill: parent
            onClicked: wifiDialog.visible = false
        }

        Rectangle {
            anchors.centerIn: parent
            width: parent.width - Theme.spaceXL * 2
            implicitHeight: dlgCol.implicitHeight + Theme.spaceL * 2
            radius: Theme.radiusL
            color: Theme.surfaceHigh

            MouseArea { anchors.fill: parent }   // swallow clicks

            ColumnLayout {
                id: dlgCol
                anchors {
                    fill: parent
                    margins: Theme.spaceL
                }
                spacing: Theme.spaceM

                Text {
                    Layout.fillWidth: true
                    text: wifiDialog.mode === "connect"
                          ? "Connect to " + wifiDialog.ssid
                          : "Disconnect from " + wifiDialog.ssid + "?"
                    color: Theme.text
                    font.pixelSize: Theme.fontTitle
                    wrapMode: Text.Wrap
                }

                TextField {
                    id: pwField
                    Layout.fillWidth: true
                    visible: wifiDialog.mode === "connect"
                    echoMode: TextInput.Password
                    placeholderText: "Password (blank for open networks)"
                    placeholderTextColor: Theme.textFaint
                    color: Theme.text
                    font.pixelSize: Theme.fontBody
                    background: Rectangle {
                        color: Theme.surface
                        radius: Theme.radiusS
                    }
                    onAccepted: connectBtn.clicked()
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceM
                    Item { Layout.fillWidth: true }
                    AButton {
                        text: "Cancel"
                        accent: false
                        onClicked: wifiDialog.visible = false
                    }
                    AButton {
                        id: connectBtn
                        text: wifiDialog.mode === "connect" ? "Connect" : "Disconnect"
                        onClicked: {
                            if (wifiDialog.mode === "connect")
                                Network.connectTo(wifiDialog.ssid, pwField.text)
                            else
                                Network.disconnect()
                            wifiDialog.visible = false
                        }
                    }
                }
            }
        }
    }
}
