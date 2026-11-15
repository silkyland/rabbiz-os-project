import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import A21s.Theme
import A21s.Services

// a21s-dialer — Google-Phone-style dialer for a21s-shell.
//
// Standalone app window (real xdg toplevel, maximized by labwc):
//   QML2_IMPORT_PATH=/usr/share/a21s-shell exec qml6 \
//       /usr/share/a21s-shell/apps/dialer/Dialer.qml
//
// Args (after `--`):
//   --simulate-incoming <number>   ring the fake modem after ~1 s
//   --simulate-incoming=<number>   same, single-arg form
//
// Pure QtQuick — Quickshell is intentionally not imported so this file also
// works under plain `qml6`.
// NOTE on imports: `A21s.Theme`/`A21s.Services` URIs require the deployed
// tree to expose A21s/Theme + A21s/Services qmldirs; relative directory
// imports below resolve the SAME singletons in every layout.

ApplicationWindow {
    id: root
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    title: "Phone"
    color: Theme.bg

    // ---------------- state ----------------
    property string dialNumber: ""
    property int currentTab: 1            // 0 = Recents, 1 = Keypad
    property bool muted: false
    property bool speakerOn: false
    property bool dtmfOpen: false
    property string dtmfBuffer: ""
    property int callSeconds: 0
    property string lastCallNumber: ""
    property string callDir: "out"        // "in" | "out"
    property bool callWasActive: false

    readonly property var padKeys: [
        { "d": "1", "l": ""     }, { "d": "2", "l": "ABC"   }, { "d": "3", "l": "DEF"   },
        { "d": "4", "l": "GHI"  }, { "d": "5", "l": "JKL"   }, { "d": "6", "l": "MNO"   },
        { "d": "7", "l": "PQRS" }, { "d": "8", "l": "TUV"   }, { "d": "9", "l": "WXYZ" },
        { "d": "*", "l": ""     }, { "d": "0", "l": "+"     }, { "d": "#", "l": ""     }
    ]

    // ---------------- helpers ----------------
    function digitsOnly(s) {
        let out = "";
        const t = String(s);
        for (let i = 0; i < t.length; ++i) {
            const c = t.charAt(i);
            if ((c >= "0" && c <= "9") || c === "*" || c === "#")
                out += c;
            else if (c === "+" && out.length === 0)
                out += c;
        }
        return out;
    }

    // Loose grouping: ###-###-#### (longer numbers get a trailing group).
    function formatNumber(d) {
        const s = String(d);
        if (s.length <= 3)
            return s;
        if (s.length <= 6)
            return s.slice(0, 3) + "-" + s.slice(3);
        if (s.length <= 10)
            return s.slice(0, 3) + "-" + s.slice(3, 6) + "-" + s.slice(6);
        return s.slice(0, 3) + "-" + s.slice(3, 6) + "-" +
               s.slice(6, 10) + "-" + s.slice(10);
    }

    function appendDigit(d) { root.dialNumber += d; }
    function backspace() { root.dialNumber = root.dialNumber.slice(0, -1); }

    function placeCall(number) {
        const n = root.digitsOnly(number);
        if (n.length === 0)
            return;
        if (Telephony.dial(n))
            root.lastCallNumber = n;
    }

    function dirGlyph(d) {
        return d === "out" ? "↗" : "↙";
    }
    function dirColor(d) {
        if (d === "missed")
            return Theme.danger;
        return d === "out" ? Theme.success : Theme.primary;
    }
    function dirText(d) {
        if (d === "missed")
            return "Missed";
        return d === "out" ? "Outgoing" : "Incoming";
    }

    function fmtElapsed(total) {
        const m = Math.floor(total / 60);
        const s = total % 60;
        return (m < 10 ? "0" : "") + m + ":" + (s < 10 ? "0" : "") + s;
    }

    function callStateText() {
        switch (Telephony.callState) {
        case "dialing": return "Dialing…";
        case "ringing": return "Ringing…";
        case "active":  return root.fmtElapsed(root.callSeconds);
        default:        return "";
        }
    }

    // Keep the editable number field in sync with dialNumber without a
    // text binding (user input would destroy it). Formatting is live:
    // ###-###-####.
    onDialNumberChanged: {
        const f = root.formatNumber(root.dialNumber);
        if (numberField && numberField.text !== f)
            numberField.text = f;
    }

    // ---------------- header ----------------
    header: Rectangle {
        implicitHeight: Theme.iconL + Theme.spaceS * 2
        color: Theme.surface
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Theme.spaceM
            anchors.rightMargin: Theme.spaceM
            spacing: Theme.spaceS

            Text {
                text: "Phone"
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontTitle
                font.bold: true
            }
            Rectangle {
                visible: Telephony.demoMode
                radius: Theme.radiusPill
                color: Theme.tileOn
                implicitWidth: demoText.implicitWidth + Theme.spaceM
                implicitHeight: demoText.implicitHeight + Theme.spaceXS * 2
                Text {
                    id: demoText
                    anchors.centerIn: parent
                    text: "DEMO"
                    color: Theme.accent
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontCaption
                    font.bold: true
                }
            }
            Item { Layout.fillWidth: true }
            Text {
                text: (Telephony.operatorName.length > 0
                      ? Telephony.operatorName : "No service")
                      + (Telephony.signalStrength >= 0
                         ? "  ·  " + Telephony.signalStrength + "%" : "")
                color: Theme.textDim
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontCaption
            }
        }
    }

    // ---------------- content ----------------
    Item {
        anchors.fill: parent

        StackLayout {
            id: stack
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.bottom: tabBar.top
            currentIndex: root.currentTab

            // ===== page 0: recents =====
            Item {
                ListView {
                    id: recentsList
                    anchors.fill: parent
                    clip: true
                    topMargin: Theme.spaceS
                    model: recentsModel
                    delegate: recentsDelegate
                }
                Text {
                    anchors.centerIn: parent
                    visible: recentsModel.count === 0
                    text: "No recent calls"
                    color: Theme.textFaint
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                }
            }

            // ===== page 1: keypad =====
            Item {
                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Theme.spaceM
                    anchors.bottomMargin: Theme.spaceS
                    spacing: Theme.spaceM

                    // number display + backspace
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spaceS
                        Item {
                            Layout.preferredWidth: backBtn.width
                            Layout.preferredHeight: 2
                        }
                        TextField {
                            id: numberField
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignHCenter
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontHeadline
                            color: Theme.text
                            placeholderText: "Enter number"
                            placeholderTextColor: Theme.textFaint
                            inputMethodHints: Qt.ImhDialableCharactersOnly | Qt.ImhNoPredictiveText
                            selectByMouse: true
                            background: Item {}
                            onTextChanged: {
                                const d = root.digitsOnly(text);
                                if (d !== root.dialNumber)
                                    root.dialNumber = d;
                            }
                        }
                        Text {
                            id: backBtn
                            text: "⌫"
                            color: tapBk.pressed ? Theme.text : Theme.textDim
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontTitle
                            visible: root.dialNumber.length > 0
                            TapHandler {
                                id: tapBk
                                onTapped: root.backspace()
                                onLongPressed: root.dialNumber = ""
                            }
                        }
                    }

                    PadGrid {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        keys: root.padKeys
                        onKeyPressed: function(digit) { root.appendDigit(digit); }
                        onKeyLongPressed: function(digit) {
                            if (digit === "0")
                                root.appendDigit("+");
                        }
                    }

                    // green call FAB
                    Item {
                        Layout.fillWidth: true
                        Layout.preferredHeight: Theme.iconApp + Theme.spaceS
                        Rectangle {
                            id: callFab
                            anchors.centerIn: parent
                            width: Theme.iconApp + Theme.spaceM
                            height: Theme.iconApp + Theme.spaceM
                            radius: width / 2
                            color: root.dialNumber.length > 0 ? Theme.success : Theme.tileOff
                            opacity: callTap.pressed ? 0.75 : 1.0
                            Text {
                                anchors.centerIn: parent
                                text: "☎"
                                color: root.dialNumber.length > 0 ? Theme.primaryContent : Theme.tileOffText
                                font.pixelSize: Theme.fontTitle
                            }
                            TapHandler {
                                id: callTap
                                enabled: root.dialNumber.length > 0
                                onTapped: root.placeCall(root.dialNumber)
                            }
                        }
                    }
                }
            }
        }

        // ===== bottom tab bar =====
        Rectangle {
            id: tabBar
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: Theme.iconL + Theme.spaceM * 2
            color: Theme.surface

            Rectangle {
                anchors.top: parent.top
                width: parent.width
                height: Math.max(1, Theme.navPillHeight / 4)
                color: Theme.separator
            }

            RowLayout {
                anchors.fill: parent
                spacing: 0
                Repeater {
                    model: [
                        { "label": "Recents", "glyph": "◷" },
                        { "label": "Keypad",  "glyph": "⌨" }
                    ]
                    delegate: Item {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        Layout.fillHeight: true

                        Rectangle {
                            anchors.top: parent.top
                            anchors.horizontalCenter: parent.horizontalCenter
                            width: Theme.navPillWidth / 2
                            height: Theme.navPillHeight / 2
                            radius: Theme.radiusPill
                            color: Theme.primary
                            visible: root.currentTab === index
                        }
                        Column {
                            anchors.centerIn: parent
                            spacing: Theme.spaceXS
                            Text {
                                anchors.horizontalCenter: parent.horizontalCenter
                                text: modelData.glyph
                                color: root.currentTab === index ? Theme.primary : Theme.tileOffText
                                font.pixelSize: Theme.fontTitle
                            }
                            Text {
                                anchors.horizontalCenter: parent.horizontalCenter
                                text: modelData.label
                                color: root.currentTab === index ? Theme.primary : Theme.tileOffText
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontCaption
                            }
                        }
                        TapHandler { onTapped: root.currentTab = index }
                    }
                }
            }
        }

        // ===== in-call overlay (dialing / ringing / active) =====
        Rectangle {
            id: callOverlay
            anchors.fill: parent
            z: 10
            visible: Telephony.callState === "dialing"
                     || Telephony.callState === "ringing"
                     || Telephony.callState === "active"
            color: Theme.bg

            ColumnLayout {
                anchors.fill: parent
                anchors.topMargin: Theme.spaceXL * 2
                anchors.bottomMargin: Theme.spaceXL
                spacing: Theme.spaceM

                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: root.formatNumber(Telephony.callNumber)
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontHeadline
                }
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: root.callStateText()
                    color: Telephony.callState === "active" ? Theme.accent : Theme.textDim
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                }
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    visible: Telephony.demoMode
                    text: "simulated call"
                    color: Theme.textFaint
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontCaption
                }

                Item { Layout.fillHeight: true }

                RowLayout {
                    Layout.alignment: Qt.AlignHCenter
                    spacing: Theme.spaceXL
                    CircleButton {
                        glyph: "∅"
                        caption: "Mute"
                        checked: root.muted
                        onTapped: root.muted = !root.muted
                    }
                    CircleButton {
                        glyph: "◉"
                        caption: "Speaker"
                        checked: root.speakerOn
                        onTapped: root.speakerOn = !root.speakerOn
                    }
                    CircleButton {
                        glyph: "▦"
                        caption: "Keypad"
                        checked: root.dtmfOpen
                        onTapped: root.dtmfOpen = !root.dtmfOpen
                    }
                }

                Item { Layout.fillHeight: true }

                CircleButton {
                    Layout.alignment: Qt.AlignHCenter
                    glyph: "✕"
                    caption: "End"
                    bg: Theme.danger
                    fg: Theme.primaryContent
                    onTapped: Telephony.hangup()
                }
            }

            // ===== in-call DTMF pad =====
            Rectangle {
                anchors.fill: parent
                visible: root.dtmfOpen
                color: Theme.bg
                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Theme.spaceM
                    spacing: Theme.spaceS

                    Text {
                        Layout.alignment: Qt.AlignHCenter
                        text: root.dtmfBuffer.length > 0 ? root.dtmfBuffer : "Keypad"
                        color: root.dtmfBuffer.length > 0 ? Theme.text : Theme.textFaint
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontTitle
                    }
                    PadGrid {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        keys: root.padKeys
                        onKeyPressed: function(digit) {
                            root.dtmfBuffer += digit;
                            Telephony.sendDtmf(digit);
                        }
                    }
                    Text {
                        Layout.alignment: Qt.AlignHCenter
                        text: "Hide"
                        color: Theme.primary
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontBody
                        padding: Theme.spaceM
                        TapHandler { onTapped: root.dtmfOpen = false }
                    }
                }
            }
        }

        // ===== incoming-call overlay (slide up = answer, down = decline) =====
        Rectangle {
            id: incomingOverlay
            anchors.fill: parent
            z: 20
            visible: Telephony.callState === "incoming"
            color: Theme.bg

            ColumnLayout {
                anchors.fill: parent
                anchors.topMargin: Theme.spaceXL * 2
                anchors.bottomMargin: Theme.spaceXL
                spacing: Theme.spaceM

                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: "Incoming call"
                    color: Theme.textDim
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                }
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: root.formatNumber(Telephony.callNumber)
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontHeadline
                }
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: Telephony.operatorName.length > 0 ? Telephony.operatorName : "No service"
                    color: Theme.textDim
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontCaption
                }

                Item { Layout.fillHeight: true }

                Item {
                    id: answerPad
                    Layout.fillWidth: true
                    Layout.preferredHeight: Theme.iconApp * 4
                    readonly property int restY: Math.round((height - Theme.iconApp) / 2)
                    onVisibleChanged: {
                        if (visible)
                            handle.y = answerPad.restY;
                    }

                    Rectangle {
                        anchors.top: parent.top
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Theme.navPillHeight / 2
                        height: Theme.spaceL
                        radius: Theme.radiusPill
                        color: Theme.success
                        opacity: 0.9
                    }
                    Text {
                        anchors.top: parent.top
                        anchors.topMargin: Theme.spaceL + Theme.spaceS
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "↑ answer"
                        color: Theme.success
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontCaption
                    }
                    Text {
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: Theme.spaceL + Theme.spaceS
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "↓ decline"
                        color: Theme.danger
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontCaption
                    }
                    Rectangle {
                        anchors.bottom: parent.bottom
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Theme.navPillHeight / 2
                        height: Theme.spaceL
                        radius: Theme.radiusPill
                        color: Theme.danger
                        opacity: 0.9
                    }

                    // vertical guide track
                    Rectangle {
                        anchors.horizontalCenter: parent.horizontalCenter
                        y: Theme.spaceL * 2 + Theme.spaceS
                        width: Theme.navPillHeight / 4
                        height: parent.height - (Theme.spaceL * 2 + Theme.spaceS) * 2
                        radius: Theme.radiusPill
                        color: Theme.separator
                    }

                    // draggable answer handle
                    Rectangle {
                        id: handle
                        width: Theme.iconApp
                        height: Theme.iconApp
                        radius: width / 2
                        x: Math.round((answerPad.width - width) / 2)
                        y: answerPad.restY
                        color: dragArea.pressed ? Theme.accent : Theme.success
                        Text {
                            anchors.centerIn: parent
                            text: "☎"
                            color: Theme.primaryContent
                            font.pixelSize: Theme.fontTitle
                        }
                        MouseArea {
                            id: dragArea
                            anchors.fill: parent
                            drag.target: handle
                            drag.axis: Drag.YAxis
                            drag.minimumY: 0
                            drag.maximumY: answerPad.height - handle.height
                            onReleased: {
                                const up = answerPad.restY - Theme.iconApp;
                                const down = answerPad.restY + Theme.iconApp;
                                if (handle.y <= up)
                                    Telephony.answer();
                                else if (handle.y >= down)
                                    Telephony.hangup();
                                snapBack.restart();
                            }
                        }
                        NumberAnimation on y {
                            id: snapBack
                            to: answerPad.restY
                            duration: Theme.animMed
                            easing.type: Theme.animEase
                        }
                    }
                }
            }
        }
    }

    // ---------------- recents model (demo entries) ----------------
    ListModel {
        id: recentsModel
        ListElement { name: "Somchai";     number: "0812345678"; when: "09:41";     dir: "missed" }
        ListElement { name: "Mom";         number: "0865551234"; when: "Yesterday"; dir: "in"     }
        ListElement { name: "Grab Driver"; number: "0927778888"; when: "Yesterday"; dir: "out"    }
        ListElement { name: "";            number: "1700";       when: "Monday";    dir: "in"     }
        ListElement { name: "Bank Service"; number: "021234567"; when: "Sunday";    dir: "out"    }
    }

    Component {
        id: recentsDelegate
        Item {
            required property string name
            required property string number
            required property string when
            required property string dir
            width: ListView.view.width
            height: Theme.iconM + Theme.spaceM * 2

            Rectangle {   // row separator
                anchors.bottom: parent.bottom
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.leftMargin: Theme.spaceM + Theme.iconM + Theme.spaceM
                height: Math.max(1, Theme.navPillHeight / 4)
                color: Theme.separator
            }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Theme.spaceM
                anchors.rightMargin: Theme.spaceM
                spacing: Theme.spaceM

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    RowLayout {
                        anchors.fill: parent
                        spacing: Theme.spaceM
                        Rectangle {   // avatar tile
                            Layout.preferredWidth: Theme.iconM
                            Layout.preferredHeight: Theme.iconM
                            radius: Theme.radiusPill
                            color: Qt.rgba(root.dirColor(dir).r, root.dirColor(dir).g,
                                          root.dirColor(dir).b, 0.18)
                            Text {
                                anchors.centerIn: parent
                                text: name.length > 0 ? name.charAt(0).toUpperCase() : "☎"
                                color: root.dirColor(dir)
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody
                                font.bold: true
                            }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spaceXS
                            Text {
                                Layout.fillWidth: true
                                text: name.length > 0 ? name : root.formatNumber(number)
                                color: dir === "missed" ? Theme.danger : Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody
                                elide: Text.ElideRight
                            }
                            RowLayout {
                                spacing: Theme.spaceXS
                                Text {
                                    text: root.dirGlyph(dir)
                                    color: root.dirColor(dir)
                                    font.pixelSize: Theme.fontCaption
                                    font.bold: true
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: root.dirText(dir) + " · "
                                          + root.formatNumber(number) + " · " + when
                                    color: Theme.textDim
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontCaption
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }
                    TapHandler {
                        onTapped: {
                            root.dialNumber = number;
                            root.currentTab = 1;
                        }
                    }
                }

                Rectangle {   // call-back button
                    Layout.preferredWidth: Theme.iconM
                    Layout.preferredHeight: Theme.iconM
                    radius: Theme.radiusPill
                    color: callBackTap.pressed ? Theme.tileOn : Theme.surfaceHigh
                    Text {
                        anchors.centerIn: parent
                        text: "☎"
                        color: Theme.success
                        font.pixelSize: Theme.fontBody
                    }
                    TapHandler {
                        id: callBackTap
                        onTapped: root.placeCall(number)
                    }
                }
            }
        }
    }

    // ---------------- inline components ----------------
    // Round icon button with optional caption (in-call controls).
    component CircleButton: Column {
        id: cb
        property string glyph: ""
        property string caption: ""
        property color bg: Theme.surfaceHigh
        property color fg: Theme.text
        property bool checked: false
        property int diameter: Theme.iconApp
        signal tapped()

        spacing: Theme.spaceXS
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: cb.diameter
            height: cb.diameter
            radius: cb.diameter / 2
            color: keyTap.pressed ? Theme.tileOn
                 : (cb.checked ? Theme.tileOn : cb.bg)
            Text {
                anchors.centerIn: parent
                text: cb.glyph
                color: cb.checked ? Theme.tileOnText : cb.fg
                font.pixelSize: Theme.fontTitle
            }
            TapHandler { id: keyTap; onTapped: cb.tapped() }
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: cb.caption.length > 0
            text: cb.caption
            color: cb.checked ? Theme.tileOnText : Theme.textDim
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontCaption
        }
    }

    // Reusable 4x3 digit pad (keypad page + in-call DTMF overlay).
    component PadGrid: GridLayout {
        id: padGrid
        property var keys: []
        signal keyPressed(string digit)
        signal keyLongPressed(string digit)

        columns: 3
        columnSpacing: Theme.spaceM
        rowSpacing: Theme.spaceS

        readonly property int rows: Math.max(1, Math.ceil(keys.length / columns))
        // Key size fits BOTH the grid width and the height the parent
        // ColumnLayout actually allocated — otherwise rows overlap when the
        // window is shorter than the design size (e.g. 1280x720 preview).
        readonly property int keyD: Math.max(Theme.iconM, Math.min(
            Theme.iconApp + Theme.iconL,
            Math.floor((width - columnSpacing * (columns - 1)) / columns),
            Math.floor((height - rowSpacing * (rows - 1)) / rows)))

        Repeater {
            model: padGrid.keys
            delegate: Rectangle {
                required property var modelData
                Layout.preferredWidth: padGrid.keyD
                Layout.preferredHeight: padGrid.keyD
                Layout.alignment: Qt.AlignHCenter
                radius: width / 2
                color: keyTap.pressed ? Theme.tileOn : Theme.surfaceHigh
                Column {
                    anchors.centerIn: parent
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: modelData.d
                        color: Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontTitle
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        visible: modelData.l.length > 0
                        text: modelData.l
                        color: Theme.textFaint
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontCaption
                    }
                }
                TapHandler {
                    id: keyTap
                    onTapped: padGrid.keyPressed(modelData.d)
                    onLongPressed: padGrid.keyLongPressed(modelData.d)
                }
            }
        }
    }

    // ---------------- telephony wiring ----------------
    Connections {
        target: Telephony
        function onCallStateChanged() {
            const s = Telephony.callState;
            if (s === "dialing" || s === "incoming") {
                root.callDir = s === "incoming" ? "in" : "out";
                root.callWasActive = false;
                root.lastCallNumber = Telephony.callNumber;
            } else if (s === "active") {
                root.callSeconds = 0;
                root.callWasActive = true;
            } else if (s === "") {
                if (root.lastCallNumber.length > 0) {
                    const d = root.callDir === "out" ? "out"
                            : (root.callWasActive ? "in" : "missed");
                    recentsModel.insert(0, {
                        "name": "",
                        "number": root.lastCallNumber,
                        "when": Qt.formatTime(new Date(), "hh:mm"),
                        "dir": d
                    });
                    root.lastCallNumber = "";
                }
                root.muted = false;
                root.speakerOn = false;
                root.dtmfOpen = false;
                root.dtmfBuffer = "";
            }
        }
    }

    Timer {
        interval: 1000
        repeat: true
        running: Telephony.callState === "active"
        onTriggered: root.callSeconds += 1
    }

    // --simulate-incoming trigger (1 s delay so the UI is up first).
    Timer {
        id: simTrigger
        property string number: "0800123456"
        interval: 1000
        onTriggered: Telephony.simulateIncomingCall(simTrigger.number)
    }

    Component.onCompleted: {
        const args = Qt.application.arguments;
        for (let i = 0; i < args.length; ++i) {
            const a = String(args[i]);
            if (a === "--simulate-incoming") {
                if (i + 1 < args.length)
                    simTrigger.number = String(args[i + 1]);
                simTrigger.start();
                break;
            }
            if (a.indexOf("--simulate-incoming=") === 0) {
                simTrigger.number = a.substring("--simulate-incoming=".length);
                simTrigger.start();
                break;
            }
        }
    }

    // Desktop keyboard support (device users use the pad).
    Shortcut {
        id: digitKeys
        sequences: ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "*", "#"]
        onActivated: root.appendDigit(digitKeys.sequence)
    }
    Shortcut {
        sequence: "Backspace"
        onActivated: root.backspace()
    }
    Shortcut {
        sequences: ["Return", "Enter"]
        onActivated: root.placeCall(root.dialNumber)
    }
    Shortcut {
        sequence: "Escape"
        onActivated: {
            if (root.dtmfOpen)
                root.dtmfOpen = false;
            else if (Telephony.callState !== "")
                Telephony.hangup();
        }
    }
}
