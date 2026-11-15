import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import A21s.Theme
import A21s.Services
import A21s.Components as Components

// a21s-dialer — One UI / Stitch-styled Phone app for a21s-shell.
//
// Launched by `a21s-dialer` (quickshell -p Dialer.qml) or plain `qml6`.
// Args (after `--`):
//   --simulate-incoming <number>   ring the fake modem after ~1 s
//   --simulate-incoming=<number>   same, single-arg form
ApplicationWindow {
    id: root
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    title: "Phone"
    color: Theme.bg
    font.family: Theme.fontFamily

    // ---------------- state ----------------
    property string dialNumber: ""
    property int currentTab: 0            // 0 = Keypad, 1 = Recents, 2 = Contacts
    property string recentsFilter: "all"  // "all" | "missed"
    property string contactQuery: ""
    property bool muted: false
    property bool speakerOn: false
    property bool dtmfOpen: false
    property string dtmfBuffer: ""
    property int callSeconds: 0
    property string lastCallNumber: ""
    property string callDir: "out"        // "in" | "out"
    property bool callWasActive: false

    readonly property var padKeys: [
        { "d": "1", "l": "",     "icon": "voicemail" },
        { "d": "2", "l": "ABC",  "icon": "" },
        { "d": "3", "l": "DEF",  "icon": "" },
        { "d": "4", "l": "GHI",  "icon": "" },
        { "d": "5", "l": "JKL",  "icon": "" },
        { "d": "6", "l": "MNO",  "icon": "" },
        { "d": "7", "l": "PQRS", "icon": "" },
        { "d": "8", "l": "TUV",  "icon": "" },
        { "d": "9", "l": "WXYZ", "icon": "" },
        { "d": "*", "l": "",     "icon": "" },
        { "d": "0", "l": "+",    "icon": "" },
        { "d": "#", "l": "",     "icon": "" }
    ]

    readonly property var contactsList: [
        { "name": "Mom",           "number": "0865551234", "label": "Mobile",    "color": "#EC4899", "fav": true },
        { "name": "Alex",          "number": "0812345678", "label": "Mobile",    "color": "#3B82F6", "fav": true },
        { "name": "Sarah Chen",    "number": "0894442211", "label": "Work",      "color": "#10B981", "fav": true },
        { "name": "Somchai",       "number": "0819876543", "label": "Mobile",    "color": "#F59E0B", "fav": false },
        { "name": "Grab Driver",   "number": "0927778888", "label": "Trip",      "color": "#22C55E", "fav": false },
        { "name": "Veridian Bank", "number": "021234567",  "label": "Hotline",   "color": "#8B5CF6", "fav": false },
        { "name": "Emergency",     "number": "191",        "label": "Emergency", "color": "#EF4444", "fav": false },
        { "name": "Voicemail",     "number": "*100",       "label": "Carrier",   "color": "#64748B", "fav": false }
    ]

    readonly property var favoriteContacts: {
        var out = [];
        for (var i = 0; i < contactsList.length; ++i) {
            if (contactsList[i].fav)
                out.push(contactsList[i]);
        }
        return out;
    }

    readonly property var filteredContacts: {
        var q = contactQuery.trim().toLowerCase();
        var out = [];
        for (var i = 0; i < contactsList.length; ++i) {
            var c = contactsList[i];
            if (q.length === 0
                    || c.name.toLowerCase().indexOf(q) >= 0
                    || c.number.toLowerCase().indexOf(q) >= 0) {
                out.push(c);
            }
        }
        return out;
    }

    readonly property var activeDialMatch: lookupContact(dialNumber)
    readonly property var activeCallMatch: lookupContact(Telephony.callNumber)

    // ---------------- helpers ----------------
    function digitsOnly(s) {
        var out = "";
        var t = String(s);
        for (var i = 0; i < t.length; ++i) {
            var c = t.charAt(i);
            if ((c >= "0" && c <= "9") || c === "*" || c === "#")
                out += c;
            else if (c === "+" && out.length === 0)
                out += c;
        }
        return out;
    }

    function formatNumber(d) {
        var s = String(d);
        if (s.length === 0)
            return "";
        if (s.charAt(0) === "*" || s.charAt(0) === "#")
            return s;
        if (s.length <= 3)
            return s;
        if (s.length <= 6)
            return s.slice(0, 3) + "-" + s.slice(3);
        if (s.length <= 10)
            return s.slice(0, 3) + "-" + s.slice(3, 6) + "-" + s.slice(6);
        return s.slice(0, 3) + "-" + s.slice(3, 6) + "-"
             + s.slice(6, 10) + "-" + s.slice(10);
    }

    function lookupContact(number) {
        var target = digitsOnly(number);
        if (target.length === 0)
            return null;
        for (var i = 0; i < contactsList.length; ++i) {
            if (digitsOnly(contactsList[i].number) === target)
                return contactsList[i];
        }
        return null;
    }

    function avatarColorFor(name, number) {
        var m = lookupContact(number);
        if (m)
            return m.color;
        var palette = ["#3B82F6", "#10B981", "#F59E0B", "#EC4899", "#8B5CF6", "#06B6D4"];
        var seed = (name && name.length > 0 ? name : String(number));
        var h = 0;
        for (var i = 0; i < seed.length; ++i)
            h = (h * 31 + seed.charCodeAt(i)) & 0xffff;
        return palette[h % palette.length];
    }

    function appendDigit(d) {
        if (root.dialNumber.length < 24)
            root.dialNumber += d;
    }

    function backspace() {
        root.dialNumber = root.dialNumber.slice(0, -1);
    }

    function placeCall(number) {
        var n = root.digitsOnly(number);
        if (n.length === 0) {
            if (recentsModel.count > 0) {
                root.dialNumber = recentsModel.get(0).number;
                showToast("Loaded last number");
            }
            return;
        }
        if (Telephony.dial(n))
            root.lastCallNumber = n;
    }

    function dirIcon(d) {
        if (d === "missed")
            return "call_missed";
        return d === "out" ? "call_made" : "call_received";
    }

    function dirColor(d) {
        if (d === "missed")
            return Theme.danger;
        return d === "out" ? Theme.success : ShellPrefs.accentColor;
    }

    function dirText(d) {
        if (d === "missed")
            return "Missed";
        return d === "out" ? "Outgoing" : "Incoming";
    }

    function missedCount() {
        var n = 0;
        for (var i = 0; i < recentsModel.count; ++i) {
            if (recentsModel.get(i).dir === "missed")
                n += 1;
        }
        return n;
    }

    function fmtElapsed(total) {
        var m = Math.floor(total / 60);
        var s = total % 60;
        return (m < 10 ? "0" : "") + m + ":" + (s < 10 ? "0" : "") + s;
    }

    function callStateText() {
        switch (Telephony.callState) {
        case "dialing":  return "Dialing…";
        case "ringing":  return "Ringing…";
        case "incoming": return "Incoming call";
        case "active":   return root.fmtElapsed(root.callSeconds);
        default:         return "";
        }
    }

    function showToast(msg) {
        toastText.text = msg;
        toast.opacity = 1;
        toastTimer.restart();
    }

    function copyToClipboard(text) {
        clipHelper.text = text;
        clipHelper.selectAll();
        clipHelper.copy();
        clipHelper.deselect();
        showToast("Copied number");
    }

    // Hidden clipboard helper
    TextEdit {
        id: clipHelper
        visible: false
        width: 0
        height: 0
    }

    // Subtle top ambient accent glow (matches HomeScreen / Settings aesthetic)
    Rectangle {
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
        }
        height: 280
        gradient: Gradient {
            GradientStop { position: 0.0; color: Qt.alpha(ShellPrefs.accentColor, 0.08) }
            GradientStop { position: 1.0; color: "transparent" }
        }
    }

    // ---------------- main shell ----------------
    ColumnLayout {
        anchors {
            fill: parent
            margins: Theme.spaceM
            bottomMargin: Theme.navZoneHeight + Theme.spaceS
        }
        spacing: Theme.spaceM

        // Top app header
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceS

            Rectangle {
                width: 52
                height: 52
                radius: 26
                color: Qt.alpha(ShellPrefs.accentColor, 0.16)
                border.width: 1
                border.color: Qt.alpha(ShellPrefs.accentColor, 0.30)

                Components.MaterialIcon {
                    anchors.centerIn: parent
                    name: "call"
                    size: 26
                    color: ShellPrefs.accentColor
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0

                Text {
                    text: "Phone"
                    color: Theme.text
                    font.pixelSize: Theme.fontTitle
                    font.weight: Font.DemiBold
                }

                Text {
                    text: Telephony.demoMode
                          ? "Simulated voice · " + (Telephony.operatorName.length > 0 ? Telephony.operatorName : "Demo")
                          : (Telephony.operatorName.length > 0 ? Telephony.operatorName : "Mobile network")
                    color: Theme.textDim
                    font.pixelSize: Theme.fontStatus - 2
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
            }

            // Carrier / modem pill
            Rectangle {
                implicitWidth: simRow.implicitWidth + Theme.spaceM
                implicitHeight: 40
                radius: Theme.radiusPill
                color: Theme.surfaceHigh
                border.width: 1
                border.color: Qt.alpha(Theme.text, 0.08)

                Row {
                    id: simRow
                    anchors.centerIn: parent
                    spacing: 6

                    Components.MaterialIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: Telephony.modemAvailable ? "signal_cellular_alt" : "sim_card_alert"
                        size: 18
                        color: Telephony.modemAvailable ? Theme.success : Theme.warn
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: Telephony.modemAvailable
                              ? (Telephony.signalStrength >= 0 ? Telephony.signalStrength + "%" : "SIM")
                              : "DEMO"
                        color: Theme.text
                        font.pixelSize: Theme.fontStatus - 2
                        font.weight: Font.DemiBold
                    }
                }
            }

            // One-tap incoming call simulator in demo mode
            Rectangle {
                visible: Telephony.demoMode
                implicitWidth: ringRow.implicitWidth + Theme.spaceM
                implicitHeight: 40
                radius: Theme.radiusPill
                color: Qt.alpha(ShellPrefs.accentColor, 0.16)
                border.width: 1
                border.color: Qt.alpha(ShellPrefs.accentColor, 0.32)
                scale: ringTap.pressed ? 0.94 : 1.0
                Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

                Row {
                    id: ringRow
                    anchors.centerIn: parent
                    spacing: 6

                    Components.MaterialIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "ring_volume"
                        size: 18
                        color: ShellPrefs.accentColor
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "Ring"
                        color: ShellPrefs.accentColor
                        font.pixelSize: Theme.fontStatus - 2
                        font.weight: Font.DemiBold
                    }
                }

                TapHandler {
                    id: ringTap
                    onTapped: Telephony.simulateIncomingCall("0812345678")
                }
            }
        }

        // Swipeable pages synced with bottom pill tabs
        SwipeView {
            id: pager
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            currentIndex: root.currentTab
            onCurrentIndexChanged: root.currentTab = currentIndex

            // ==============================================================
            // PAGE 0: KEYPAD
            // ==============================================================
            Item {
                ColumnLayout {
                    anchors.fill: parent
                    spacing: Theme.spaceM

                    // Number display stage
                    Item {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.minimumHeight: 190

                        ColumnLayout {
                            anchors {
                                left: parent.left
                                right: parent.right
                                verticalCenter: parent.verticalCenter
                            }
                            spacing: Theme.spaceS

                            // Caller ID match pill (or quick speed-dial chips when empty)
                            Item {
                                Layout.fillWidth: true
                                Layout.preferredHeight: 46

                                // Known contact match pill
                                Rectangle {
                                    anchors.centerIn: parent
                                    visible: root.activeDialMatch !== null
                                    implicitWidth: matchRow.implicitWidth + Theme.spaceM + 8
                                    implicitHeight: 40
                                    radius: Theme.radiusPill
                                    color: Theme.card
                                    border.width: 1
                                    border.color: Qt.alpha(Theme.text, 0.09)

                                    Row {
                                        id: matchRow
                                        anchors.centerIn: parent
                                        spacing: 8

                                        Rectangle {
                                            anchors.verticalCenter: parent.verticalCenter
                                            width: 10
                                            height: 10
                                            radius: 5
                                            color: root.activeDialMatch ? root.activeDialMatch.color : ShellPrefs.accentColor
                                        }
                                        Text {
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: root.activeDialMatch
                                                  ? (root.activeDialMatch.name + " · " + root.activeDialMatch.label)
                                                  : ""
                                            color: Theme.text
                                            font.pixelSize: Theme.fontCaption
                                            font.weight: Font.Medium
                                        }
                                    }
                                }

                                // Quick-dial favorite chips when keypad is empty
                                Flickable {
                                    anchors.fill: parent
                                    visible: root.dialNumber.length === 0
                                    contentWidth: quickRow.implicitWidth
                                    clip: true
                                    flickableDirection: Flickable.HorizontalFlick

                                    Row {
                                        id: quickRow
                                        height: parent.height
                                        x: Math.max(0, (parent.width - width) / 2)
                                        spacing: Theme.spaceS

                                        Repeater {
                                            model: root.favoriteContacts
                                            delegate: Rectangle {
                                                required property var modelData
                                                anchors.verticalCenter: parent.verticalCenter
                                                implicitWidth: chipRow.implicitWidth + Theme.spaceM + 4
                                                implicitHeight: 40
                                                radius: Theme.radiusPill
                                                color: Theme.card
                                                border.width: 1
                                                border.color: Qt.alpha(Theme.text, 0.08)
                                                scale: chipTap.pressed ? 0.94 : 1.0
                                                Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                                                Row {
                                                    id: chipRow
                                                    anchors.centerIn: parent
                                                    spacing: 8

                                                    Rectangle {
                                                        anchors.verticalCenter: parent.verticalCenter
                                                        width: 22
                                                        height: 22
                                                        radius: 11
                                                        color: Qt.alpha(modelData.color, 0.22)
                                                        Text {
                                                            anchors.centerIn: parent
                                                            text: modelData.name.charAt(0).toUpperCase()
                                                            color: modelData.color
                                                            font.pixelSize: 13
                                                            font.weight: Font.Bold
                                                        }
                                                    }
                                                    Text {
                                                        anchors.verticalCenter: parent.verticalCenter
                                                        text: modelData.name
                                                        color: Theme.textDim
                                                        font.pixelSize: Theme.fontStatus
                                                        font.weight: Font.Medium
                                                    }
                                                }

                                                TapHandler {
                                                    id: chipTap
                                                    onTapped: root.dialNumber = modelData.number
                                                    onLongPressed: root.placeCall(modelData.number)
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Big formatted number display
                            Text {
                                Layout.fillWidth: true
                                Layout.leftMargin: Theme.spaceS
                                Layout.rightMargin: Theme.spaceS
                                horizontalAlignment: Text.AlignHCenter
                                text: root.dialNumber.length > 0
                                      ? root.formatNumber(root.dialNumber)
                                      : "Enter number"
                                color: root.dialNumber.length > 0 ? Theme.text : Theme.textFaint
                                font.pixelSize: root.dialNumber.length > 0 ? 58 : 42
                                font.weight: root.dialNumber.length > 0 ? Font.Light : Font.Normal
                                minimumPixelSize: 28
                                fontSizeMode: Text.HorizontalFit
                                maximumLineCount: 1
                            }

                            // Subtle action hint row
                            Text {
                                Layout.alignment: Qt.AlignHCenter
                                visible: root.dialNumber.length > 0
                                text: root.dialNumber.length === 1 && root.dialNumber === "0"
                                      ? "Hold 0 for + · Tap number to copy"
                                      : "Tap to copy · Hold Call icon to clear"
                                color: Theme.textFaint
                                font.pixelSize: Theme.fontStatus - 2
                                opacity: 0.85

                                TapHandler {
                                    onTapped: root.copyToClipboard(root.dialNumber)
                                }
                            }
                        }
                    }

                    // Tactile 3x4 keypad grid
                    PadGrid {
                        id: mainPad
                        Layout.alignment: Qt.AlignHCenter
                        Layout.preferredWidth: Math.min(parent.width, 624)
                        keyHeight: 96
                        keys: root.padKeys
                        onKeyPressed: function(digit) { root.appendDigit(digit); }
                        onKeyLongPressed: function(digit) {
                            if (digit === "0") {
                                root.appendDigit("+");
                            } else if (digit === "1") {
                                root.dialNumber = "*100";
                                root.placeCall("*100");
                            }
                        }
                    }

                    // Symmetric bottom action row: Voicemail | Call FAB | Backspace
                    RowLayout {
                        Layout.alignment: Qt.AlignHCenter
                        Layout.preferredWidth: mainPad.width
                        Layout.topMargin: Theme.spaceXS
                        Layout.bottomMargin: Theme.spaceXS
                        spacing: 14

                        // Left: Voicemail shortcut
                        Item {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 84

                            Rectangle {
                                anchors.centerIn: parent
                                width: 80
                                height: 80
                                radius: 40
                                color: vmTap.pressed ? Theme.surfaceHigh : Theme.card
                                border.width: 1
                                border.color: Qt.alpha(Theme.text, 0.08)
                                scale: vmTap.pressed ? 0.92 : 1.0
                                Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

                                Components.MaterialIcon {
                                    anchors.centerIn: parent
                                    name: "voicemail"
                                    size: 30
                                    color: Theme.textDim
                                }

                                TapHandler {
                                    id: vmTap
                                    onTapped: {
                                        root.dialNumber = "*100";
                                        root.showToast("Voicemail *100");
                                    }
                                    onLongPressed: root.placeCall("*100")
                                }
                            }
                        }

                        // Center: Prominent green Call pill
                        Item {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 84

                            Rectangle {
                                anchors.centerIn: parent
                                width: 152
                                height: 80
                                radius: 40
                                color: root.dialNumber.length > 0
                                       ? Theme.success
                                       : Qt.alpha(Theme.success, 0.22)
                                border.width: root.dialNumber.length > 0 ? 0 : 1
                                border.color: Qt.alpha(Theme.success, 0.40)
                                scale: callBtnTap.pressed ? 0.93 : 1.0
                                Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }
                                Behavior on color { ColorAnimation { duration: Theme.animFast } }

                                Components.MaterialIcon {
                                    anchors.centerIn: parent
                                    name: "call"
                                    size: 38
                                    color: root.dialNumber.length > 0 ? "#052E16" : Theme.success
                                }

                                TapHandler {
                                    id: callBtnTap
                                    onTapped: root.placeCall(root.dialNumber)
                                }
                            }
                        }

                        // Right: Backspace (or muted spacer when empty)
                        Item {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 84

                            Rectangle {
                                anchors.centerIn: parent
                                width: 80
                                height: 80
                                radius: 40
                                visible: root.dialNumber.length > 0
                                color: bkTap.pressed ? Qt.alpha(Theme.danger, 0.22) : Theme.card
                                border.width: 1
                                border.color: Qt.alpha(Theme.text, 0.08)
                                scale: bkTap.pressed ? 0.92 : 1.0
                                Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

                                Components.MaterialIcon {
                                    anchors.centerIn: parent
                                    name: "backspace"
                                    size: 28
                                    color: bkTap.pressed ? Theme.danger : Theme.text
                                }

                                TapHandler {
                                    id: bkTap
                                    onTapped: root.backspace()
                                    onLongPressed: root.dialNumber = ""
                                }
                            }
                        }
                    }
                }
            }

            // ==============================================================
            // PAGE 1: RECENTS
            // ==============================================================
            Item {
                ColumnLayout {
                    anchors.fill: parent
                    spacing: Theme.spaceM

                    // Filter chips + Clear action
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spaceS

                        Repeater {
                            model: [
                                { id: "all",    label: "All" },
                                { id: "missed", label: "Missed" }
                            ]
                            delegate: Rectangle {
                                required property var modelData
                                readonly property bool active: root.recentsFilter === modelData.id
                                implicitWidth: fText.implicitWidth + Theme.spaceM * 1.4
                                implicitHeight: 42
                                radius: Theme.radiusPill
                                color: active ? ShellPrefs.accentColor : Theme.card
                                border.width: active ? 0 : 1
                                border.color: Qt.alpha(Theme.text, 0.08)

                                Text {
                                    id: fText
                                    anchors.centerIn: parent
                                    text: modelData.id === "missed" && root.missedCount() > 0
                                          ? modelData.label + " (" + root.missedCount() + ")"
                                          : modelData.label
                                    color: active ? ShellPrefs.onAccentColor : Theme.textDim
                                    font.pixelSize: Theme.fontCaption
                                    font.weight: active ? Font.DemiBold : Font.Medium
                                }

                                TapHandler {
                                    onTapped: root.recentsFilter = modelData.id
                                }
                            }
                        }

                        Item { Layout.fillWidth: true }

                        Rectangle {
                            visible: recentsModel.count > 0
                            implicitWidth: clrRow.implicitWidth + Theme.spaceM
                            implicitHeight: 42
                            radius: Theme.radiusPill
                            color: Theme.surfaceHigh
                            border.width: 1
                            border.color: Qt.alpha(Theme.text, 0.08)

                            Row {
                                id: clrRow
                                anchors.centerIn: parent
                                spacing: 6

                                Components.MaterialIcon {
                                    anchors.verticalCenter: parent.verticalCenter
                                    name: "clear_all"
                                    size: 20
                                    color: Theme.textDim
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: "Clear"
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontCaption
                                }
                            }

                            TapHandler {
                                onTapped: {
                                    recentsModel.clear();
                                    root.showToast("Call history cleared");
                                }
                            }
                        }
                    }

                    // Call log cards list
                    Item {
                        Layout.fillWidth: true
                        Layout.fillHeight: true

                        ListView {
                            id: recentsList
                            anchors.fill: parent
                            clip: true
                            model: recentsModel
                            spacing: 0

                            delegate: Item {
                                id: recentWrap
                                required property int index
                                required property string name
                                required property string number
                                required property string when
                                required property string dir

                                readonly property bool shown: root.recentsFilter === "all" || dir === "missed"
                                readonly property color avColor: dir === "missed"
                                                                 ? Theme.danger
                                                                 : root.avatarColorFor(name, number)

                                width: ListView.view.width
                                height: shown ? 104 + Theme.spaceS : 0
                                visible: shown

                                Rectangle {
                                    anchors {
                                        left: parent.left
                                        right: parent.right
                                        top: parent.top
                                    }
                                    height: 104
                                    radius: Theme.radiusL
                                    color: Theme.card
                                    border.width: 1
                                    border.color: Qt.alpha(Theme.text, 0.07)
                                    scale: rowTap.pressed ? 0.985 : 1.0
                                    Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

                                    RowLayout {
                                        anchors {
                                            fill: parent
                                            leftMargin: Theme.spaceM
                                            rightMargin: Theme.spaceM
                                        }
                                        spacing: Theme.spaceM

                                        // Avatar
                                        Rectangle {
                                            Layout.preferredWidth: 58
                                            Layout.preferredHeight: 58
                                            radius: 29
                                            color: Qt.alpha(recentWrap.avColor, 0.18)
                                            border.width: 1
                                            border.color: Qt.alpha(recentWrap.avColor, 0.35)

                                            Text {
                                                visible: recentWrap.name.length > 0
                                                anchors.centerIn: parent
                                                text: recentWrap.name.charAt(0).toUpperCase()
                                                color: recentWrap.avColor
                                                font.pixelSize: Theme.fontBody
                                                font.weight: Font.Bold
                                            }
                                            Components.MaterialIcon {
                                                visible: recentWrap.name.length === 0
                                                anchors.centerIn: parent
                                                name: "person"
                                                size: 28
                                                color: recentWrap.avColor
                                            }
                                        }

                                        // Name + direction + metadata
                                        ColumnLayout {
                                            Layout.fillWidth: true
                                            spacing: 4

                                            Text {
                                                Layout.fillWidth: true
                                                text: recentWrap.name.length > 0
                                                      ? recentWrap.name
                                                      : root.formatNumber(recentWrap.number)
                                                color: recentWrap.dir === "missed" ? Theme.danger : Theme.text
                                                font.pixelSize: Theme.fontBody
                                                font.weight: Font.DemiBold
                                                elide: Text.ElideRight
                                            }

                                            RowLayout {
                                                Layout.fillWidth: true
                                                spacing: 6

                                                Components.MaterialIcon {
                                                    name: root.dirIcon(recentWrap.dir)
                                                    size: 20
                                                    color: root.dirColor(recentWrap.dir)
                                                }
                                                Text {
                                                    Layout.fillWidth: true
                                                    text: root.dirText(recentWrap.dir)
                                                          + (recentWrap.name.length > 0
                                                             ? " · " + root.formatNumber(recentWrap.number)
                                                             : "")
                                                          + " · " + recentWrap.when
                                                    color: Theme.textDim
                                                    font.pixelSize: Theme.fontStatus
                                                    elide: Text.ElideRight
                                                }
                                            }
                                        }

                                        // Quick call button
                                        Rectangle {
                                            Layout.preferredWidth: 54
                                            Layout.preferredHeight: 54
                                            radius: 27
                                            color: Qt.alpha(Theme.success, callRowTap.pressed ? 0.30 : 0.14)
                                            border.width: 1
                                            border.color: Qt.alpha(Theme.success, 0.28)
                                            scale: callRowTap.pressed ? 0.90 : 1.0
                                            Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                                            Components.MaterialIcon {
                                                anchors.centerIn: parent
                                                name: "call"
                                                size: 26
                                                color: Theme.success
                                            }

                                            TapHandler {
                                                id: callRowTap
                                                onTapped: root.placeCall(recentWrap.number)
                                            }
                                        }
                                    }

                                    TapHandler {
                                        id: rowTap
                                        onTapped: {
                                            root.dialNumber = recentWrap.number;
                                            root.currentTab = 0;
                                        }
                                        onLongPressed: recentsMenu.openFor(
                                            recentWrap.index,
                                            recentWrap.name,
                                            recentWrap.number
                                        )
                                    }
                                }
                            }
                        }

                        // Empty state
                        Column {
                            anchors.centerIn: parent
                            visible: recentsModel.count === 0
                                     || (root.recentsFilter === "missed" && root.missedCount() === 0)
                            spacing: Theme.spaceS

                            Components.MaterialIcon {
                                anchors.horizontalCenter: parent.horizontalCenter
                                name: root.recentsFilter === "missed" ? "call_missed" : "history"
                                size: 64
                                color: Theme.textFaint
                            }
                            Text {
                                anchors.horizontalCenter: parent.horizontalCenter
                                text: root.recentsFilter === "missed"
                                      ? "No missed calls"
                                      : "No recent calls"
                                color: Theme.textFaint
                                font.pixelSize: Theme.fontBody
                            }
                        }
                    }
                }
            }

            // ==============================================================
            // PAGE 2: CONTACTS & FAVORITES
            // ==============================================================
            Item {
                ColumnLayout {
                    anchors.fill: parent
                    spacing: Theme.spaceM

                    // Search pill
                    Rectangle {
                        Layout.fillWidth: true
                        height: 60
                        radius: Theme.radiusPill
                        color: Theme.surfaceHigh
                        border.width: 1
                        border.color: Qt.alpha(Theme.text, 0.09)

                        RowLayout {
                            anchors {
                                fill: parent
                                leftMargin: Theme.spaceM
                                rightMargin: Theme.spaceM
                            }
                            spacing: Theme.spaceS

                            Components.MaterialIcon {
                                name: "search"
                                size: 28
                                color: ShellPrefs.accentColor
                            }

                            TextField {
                                id: searchField
                                Layout.fillWidth: true
                                placeholderText: "Search contacts or numbers"
                                placeholderTextColor: Theme.textFaint
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody - 2
                                background: Item {}
                                text: root.contactQuery
                                onTextChanged: root.contactQuery = text
                            }

                            Components.MaterialIcon {
                                visible: root.contactQuery.length > 0
                                name: "close"
                                size: 26
                                color: Theme.textDim

                                TapHandler {
                                    onTapped: root.contactQuery = ""
                                }
                            }
                        }
                    }

                    // Favorites cards strip (shown when not searching)
                    ColumnLayout {
                        visible: root.contactQuery.trim().length === 0
                        Layout.fillWidth: true
                        spacing: Theme.spaceS

                        Text {
                            text: "FAVORITES"
                            color: Theme.textDim
                            font.pixelSize: Theme.fontStatus - 2
                            font.weight: Font.DemiBold
                            font.letterSpacing: 1.4
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spaceS

                            Repeater {
                                model: root.favoriteContacts
                                delegate: Rectangle {
                                    required property var modelData
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 136
                                    radius: Theme.radiusL
                                    color: Theme.card
                                    border.width: 1
                                    border.color: Qt.alpha(Theme.text, 0.08)
                                    scale: favTap.pressed ? 0.95 : 1.0
                                    Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

                                    Column {
                                        anchors.centerIn: parent
                                        spacing: 8

                                        Rectangle {
                                            anchors.horizontalCenter: parent.horizontalCenter
                                            width: 56
                                            height: 56
                                            radius: 28
                                            color: Qt.alpha(modelData.color, 0.20)
                                            border.width: 1
                                            border.color: Qt.alpha(modelData.color, 0.40)

                                            Text {
                                                anchors.centerIn: parent
                                                text: modelData.name.charAt(0).toUpperCase()
                                                color: modelData.color
                                                font.pixelSize: Theme.fontBody
                                                font.weight: Font.Bold
                                            }
                                        }

                                        Text {
                                            anchors.horizontalCenter: parent.horizontalCenter
                                            text: modelData.name
                                            color: Theme.text
                                            font.pixelSize: Theme.fontCaption
                                            font.weight: Font.DemiBold
                                        }

                                        Text {
                                            anchors.horizontalCenter: parent.horizontalCenter
                                            text: "Tap to call"
                                            color: Theme.textFaint
                                            font.pixelSize: Theme.fontStatus - 4
                                        }
                                    }

                                    TapHandler {
                                        id: favTap
                                        onTapped: root.placeCall(modelData.number)
                                    }
                                }
                            }
                        }
                    }

                    Text {
                        text: root.contactQuery.trim().length > 0 ? "RESULTS" : "ALL CONTACTS"
                        color: Theme.textDim
                        font.pixelSize: Theme.fontStatus - 2
                        font.weight: Font.DemiBold
                        font.letterSpacing: 1.4
                    }

                    // All contacts list
                    ListView {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        spacing: Theme.spaceS
                        model: root.filteredContacts

                        delegate: Rectangle {
                            required property var modelData
                            width: ListView.view.width
                            height: 98
                            radius: Theme.radiusL
                            color: Theme.card
                            border.width: 1
                            border.color: Qt.alpha(Theme.text, 0.07)

                            RowLayout {
                                anchors {
                                    fill: parent
                                    leftMargin: Theme.spaceM
                                    rightMargin: Theme.spaceM
                                }
                                spacing: Theme.spaceM

                                Rectangle {
                                    Layout.preferredWidth: 56
                                    Layout.preferredHeight: 56
                                    radius: 28
                                    color: Qt.alpha(modelData.color, 0.18)
                                    border.width: 1
                                    border.color: Qt.alpha(modelData.color, 0.35)

                                    Text {
                                        anchors.centerIn: parent
                                        text: modelData.name.charAt(0).toUpperCase()
                                        color: modelData.color
                                        font.pixelSize: Theme.fontBody
                                        font.weight: Font.Bold
                                    }
                                }

                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 2

                                    Text {
                                        Layout.fillWidth: true
                                        text: modelData.name
                                        color: Theme.text
                                        font.pixelSize: Theme.fontBody
                                        font.weight: Font.DemiBold
                                        elide: Text.ElideRight
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        text: modelData.label + " · " + root.formatNumber(modelData.number)
                                        color: Theme.textDim
                                        font.pixelSize: Theme.fontStatus
                                        elide: Text.ElideRight
                                    }
                                }

                                // Load into keypad
                                Rectangle {
                                    Layout.preferredWidth: 48
                                    Layout.preferredHeight: 48
                                    radius: 24
                                    color: Theme.surfaceHigh
                                    scale: padPickTap.pressed ? 0.90 : 1.0
                                    Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                                    Components.MaterialIcon {
                                        anchors.centerIn: parent
                                        name: "dialpad"
                                        size: 22
                                        color: Theme.textDim
                                    }

                                    TapHandler {
                                        id: padPickTap
                                        onTapped: {
                                            root.dialNumber = modelData.number;
                                            root.currentTab = 0;
                                        }
                                    }
                                }

                                // Instant call
                                Rectangle {
                                    Layout.preferredWidth: 52
                                    Layout.preferredHeight: 52
                                    radius: 26
                                    color: Qt.alpha(Theme.success, 0.16)
                                    border.width: 1
                                    border.color: Qt.alpha(Theme.success, 0.30)
                                    scale: cCallTap.pressed ? 0.90 : 1.0
                                    Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                                    Components.MaterialIcon {
                                        anchors.centerIn: parent
                                        name: "call"
                                        size: 24
                                        color: Theme.success
                                    }

                                    TapHandler {
                                        id: cCallTap
                                        onTapped: root.placeCall(modelData.number)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Modern floating pill bottom navigation bar
        Rectangle {
            id: tabBar
            Layout.fillWidth: true
            Layout.preferredHeight: 74
            radius: Theme.radiusPill
            color: Qt.alpha(Theme.surfaceHigh, 0.94)
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.10)

            RowLayout {
                anchors {
                    fill: parent
                    margins: 6
                }
                spacing: 6

                Repeater {
                    model: [
                        { label: "Keypad",   icon: "dialpad" },
                        { label: "Recents",  icon: "history" },
                        { label: "Contacts", icon: "group"   }
                    ]
                    delegate: Rectangle {
                        required property int index
                        required property var modelData
                        readonly property bool active: root.currentTab === index

                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        radius: Theme.radiusPill
                        color: active ? ShellPrefs.accentColor : "transparent"
                        Behavior on color { ColorAnimation { duration: Theme.animFast } }

                        Row {
                            anchors.centerIn: parent
                            spacing: 8

                            Components.MaterialIcon {
                                anchors.verticalCenter: parent.verticalCenter
                                name: modelData.icon
                                size: 26
                                color: active ? ShellPrefs.onAccentColor : Theme.textDim
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: modelData.label
                                color: active ? ShellPrefs.onAccentColor : Theme.textDim
                                font.pixelSize: Theme.fontCaption
                                font.weight: active ? Font.DemiBold : Font.Medium
                            }

                            // Missed-call badge dot on Recents
                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                visible: index === 1 && root.missedCount() > 0 && !active
                                width: 10
                                height: 10
                                radius: 5
                                color: Theme.danger
                            }
                        }

                        TapHandler {
                            onTapped: root.currentTab = index
                        }
                    }
                }
            }
        }
    }

    // ---------------- full-screen call overlay ----------------
    Rectangle {
        id: callOverlay
        anchors.fill: parent
        z: 20
        visible: Telephony.callState !== ""
        color: Theme.bg

        readonly property bool isIncoming: Telephony.callState === "incoming"
        readonly property bool isActive: Telephony.callState === "active"
        readonly property color callAccent: isIncoming || isActive
                                          ? Theme.success
                                          : ShellPrefs.accentColor
        readonly property string displayName: root.activeCallMatch
                                              ? root.activeCallMatch.name
                                              : root.formatNumber(Telephony.callNumber)
        readonly property color avatarTone: root.activeCallMatch
                                            ? root.activeCallMatch.color
                                            : ShellPrefs.accentColor

        // Ambient vertical background glow
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0.0; color: Qt.alpha(callOverlay.callAccent, 0.16) }
                GradientStop { position: 0.45; color: Qt.alpha(callOverlay.avatarTone, 0.05) }
                GradientStop { position: 1.0; color: Theme.bg }
            }
        }

        ColumnLayout {
            anchors {
                fill: parent
                margins: Theme.spaceL
                topMargin: Theme.spaceXL
                bottomMargin: Theme.navZoneHeight + Theme.spaceL
            }
            spacing: Theme.spaceM

            // Status pill
            Rectangle {
                Layout.alignment: Qt.AlignHCenter
                implicitWidth: statusPillRow.implicitWidth + Theme.spaceM * 1.4
                implicitHeight: 42
                radius: Theme.radiusPill
                color: Theme.surfaceHigh
                border.width: 1
                border.color: Qt.alpha(callOverlay.callAccent, 0.40)

                Row {
                    id: statusPillRow
                    anchors.centerIn: parent
                    spacing: 8

                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 10
                        height: 10
                        radius: 5
                        color: callOverlay.callAccent
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.callStateText()
                        color: Theme.text
                        font.pixelSize: Theme.fontCaption
                        font.weight: Font.DemiBold
                    }
                }
            }

            Item { Layout.fillHeight: true; Layout.maximumHeight: 48 }

            // Pulsing caller avatar
            Item {
                Layout.alignment: Qt.AlignHCenter
                width: 210
                height: 210

                Rectangle {
                    id: pulseOuter
                    anchors.centerIn: parent
                    width: 204
                    height: 204
                    radius: 102
                    color: Qt.alpha(callOverlay.callAccent, 0.10)
                    visible: !callOverlay.isActive

                    SequentialAnimation on scale {
                        running: callOverlay.visible && !callOverlay.isActive
                        loops: Animation.Infinite
                        NumberAnimation { from: 0.85; to: 1.06; duration: 950; easing.type: Easing.InOutSine }
                        NumberAnimation { from: 1.06; to: 0.85; duration: 950; easing.type: Easing.InOutSine }
                    }
                }

                Rectangle {
                    anchors.centerIn: parent
                    width: 164
                    height: 164
                    radius: 82
                    color: Qt.alpha(callOverlay.avatarTone, 0.22)
                    border.width: 2
                    border.color: Qt.alpha(callOverlay.avatarTone, 0.55)
                }

                Rectangle {
                    anchors.centerIn: parent
                    width: 136
                    height: 136
                    radius: 68
                    color: callOverlay.avatarTone

                    Text {
                        visible: root.activeCallMatch !== null
                        anchors.centerIn: parent
                        text: root.activeCallMatch ? root.activeCallMatch.name.charAt(0).toUpperCase() : ""
                        color: Theme.primaryContent
                        font.pixelSize: 56
                        font.weight: Font.Bold
                    }
                    Components.MaterialIcon {
                        visible: root.activeCallMatch === null
                        anchors.centerIn: parent
                        name: "person"
                        size: 64
                        color: Theme.primaryContent
                    }
                }
            }

            // Caller name + formatted number
            Text {
                Layout.alignment: Qt.AlignHCenter
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                text: callOverlay.displayName
                color: Theme.text
                font.pixelSize: Theme.fontHeadline
                font.weight: Font.DemiBold
                elide: Text.ElideRight
            }

            Text {
                Layout.alignment: Qt.AlignHCenter
                visible: root.activeCallMatch !== null
                text: (root.activeCallMatch ? root.activeCallMatch.label + " · " : "")
                      + root.formatNumber(Telephony.callNumber)
                color: Theme.textDim
                font.pixelSize: Theme.fontBody - 2
            }

            Text {
                Layout.alignment: Qt.AlignHCenter
                visible: Telephony.demoMode
                text: (Telephony.operatorName.length > 0 ? Telephony.operatorName : "Demo Carrier")
                      + " · Simulated HD Voice"
                color: Theme.textFaint
                font.pixelSize: Theme.fontStatus
            }

            Item { Layout.fillHeight: true }

            // Active / dialing call controls
            Rectangle {
                visible: !callOverlay.isIncoming && !root.dtmfOpen
                Layout.alignment: Qt.AlignHCenter
                Layout.fillWidth: true
                Layout.maximumWidth: 540
                implicitHeight: 152
                radius: Theme.radiusL + 8
                color: Qt.alpha(Theme.surfaceHigh, 0.78)
                border.width: 1
                border.color: Qt.alpha(Theme.text, 0.09)

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: Theme.spaceM
                    spacing: Theme.spaceM

                    CallToggle {
                        Layout.fillWidth: true
                        iconName: root.muted ? "mic_off" : "mic"
                        caption: root.muted ? "Unmute" : "Mute"
                        checked: root.muted
                        onTapped: root.muted = !root.muted
                    }
                    CallToggle {
                        Layout.fillWidth: true
                        iconName: "dialpad"
                        caption: "Keypad"
                        checked: root.dtmfOpen
                        onTapped: root.dtmfOpen = !root.dtmfOpen
                    }
                    CallToggle {
                        Layout.fillWidth: true
                        iconName: "volume_up"
                        caption: "Speaker"
                        checked: root.speakerOn
                        onTapped: root.speakerOn = !root.speakerOn
                    }
                }
            }

            // In-call DTMF keypad panel
            Rectangle {
                visible: !callOverlay.isIncoming && root.dtmfOpen
                Layout.alignment: Qt.AlignHCenter
                Layout.fillWidth: true
                Layout.maximumWidth: 580
                implicitHeight: dtmfCol.implicitHeight + Theme.spaceM * 2
                radius: Theme.radiusL + 4
                color: Qt.alpha(Theme.surface, 0.94)
                border.width: 1
                border.color: Qt.alpha(Theme.text, 0.10)

                ColumnLayout {
                    id: dtmfCol
                    anchors {
                        left: parent.left
                        right: parent.right
                        top: parent.top
                        margins: Theme.spaceM
                    }
                    spacing: Theme.spaceS

                    RowLayout {
                        Layout.fillWidth: true

                        Text {
                            Layout.fillWidth: true
                            text: root.dtmfBuffer.length > 0 ? root.dtmfBuffer : "Enter tones"
                            color: root.dtmfBuffer.length > 0 ? Theme.text : Theme.textFaint
                            font.pixelSize: Theme.fontTitle
                            font.weight: Font.Medium
                            elide: Text.ElideLeft
                        }

                        Rectangle {
                            implicitWidth: hideRow.implicitWidth + Theme.spaceM
                            implicitHeight: 40
                            radius: Theme.radiusPill
                            color: Theme.surfaceHigh

                            Row {
                                id: hideRow
                                anchors.centerIn: parent
                                spacing: 6

                                Components.MaterialIcon {
                                    anchors.verticalCenter: parent.verticalCenter
                                    name: "keyboard_hide"
                                    size: 20
                                    color: ShellPrefs.accentColor
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: "Hide"
                                    color: ShellPrefs.accentColor
                                    font.pixelSize: Theme.fontStatus
                                    font.weight: Font.DemiBold
                                }
                            }

                            TapHandler { onTapped: root.dtmfOpen = false }
                        }
                    }

                    PadGrid {
                        Layout.fillWidth: true
                        keyHeight: 74
                        keys: root.padKeys
                        onKeyPressed: function(digit) {
                            root.dtmfBuffer += digit;
                            Telephony.sendDtmf(digit);
                        }
                    }
                }
            }

            // Hangup pill (dialing / ringing / active)
            Rectangle {
                visible: !callOverlay.isIncoming
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: Theme.spaceS
                width: 184
                height: 82
                radius: 41
                color: Theme.danger
                scale: endTap.pressed ? 0.92 : 1.0
                Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

                Components.MaterialIcon {
                    anchors.centerIn: parent
                    name: "call_end"
                    size: 40
                    color: Theme.primaryContent
                }

                TapHandler {
                    id: endTap
                    onTapped: Telephony.hangup()
                }
            }

            // Incoming call dual action buttons (Decline / Answer)
            RowLayout {
                visible: callOverlay.isIncoming
                Layout.alignment: Qt.AlignHCenter
                Layout.bottomMargin: Theme.spaceM
                spacing: 88

                Column {
                    spacing: Theme.spaceS

                    Rectangle {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 96
                        height: 96
                        radius: 48
                        color: Theme.danger
                        scale: decTap.pressed ? 0.90 : 1.0
                        Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                        Components.MaterialIcon {
                            anchors.centerIn: parent
                            name: "call_end"
                            size: 44
                            color: Theme.primaryContent
                        }

                        TapHandler {
                            id: decTap
                            onTapped: Telephony.hangup()
                        }
                    }

                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "Decline"
                        color: Theme.textDim
                        font.pixelSize: Theme.fontCaption
                        font.weight: Font.Medium
                    }
                }

                Column {
                    spacing: Theme.spaceS

                    Rectangle {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 96
                        height: 96
                        radius: 48
                        color: Theme.success
                        scale: ansTap.pressed ? 0.90 : 1.0
                        Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                        transform: Translate {
                            SequentialAnimation on y {
                                running: callOverlay.visible && callOverlay.isIncoming && !ansTap.pressed
                                loops: Animation.Infinite
                                NumberAnimation { from: 0; to: -6; duration: 550; easing.type: Easing.InOutQuad }
                                NumberAnimation { from: -6; to: 0; duration: 550; easing.type: Easing.InOutQuad }
                            }
                        }

                        Components.MaterialIcon {
                            anchors.centerIn: parent
                            name: "call"
                            size: 44
                            color: "#052E16"
                        }

                        TapHandler {
                            id: ansTap
                            onTapped: Telephony.answer()
                        }
                    }

                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "Answer"
                        color: Theme.success
                        font.pixelSize: Theme.fontCaption
                        font.weight: Font.DemiBold
                    }
                }
            }
        }
    }

    // ---------------- recents context popup ----------------
    Popup {
        id: recentsMenu
        property int rowIndex: -1
        property string rowName: ""
        property string rowNumber: ""

        function openFor(idx, n, num) {
            rowIndex = idx;
            rowName = n;
            rowNumber = num;
            open();
        }

        modal: true
        width: Math.min(root.width - Theme.spaceL * 2, 460)
        padding: Theme.spaceM
        x: Math.round((root.width - width) / 2)
        y: Math.round((root.height - height) / 2)
        background: Rectangle {
            color: Theme.surfaceHigh
            radius: Theme.radiusL
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.12)
        }

        contentItem: ColumnLayout {
            spacing: Theme.spaceXS

            Text {
                Layout.fillWidth: true
                Layout.bottomMargin: Theme.spaceXS
                text: recentsMenu.rowName.length > 0
                      ? recentsMenu.rowName + " · " + root.formatNumber(recentsMenu.rowNumber)
                      : root.formatNumber(recentsMenu.rowNumber)
                color: Theme.textDim
                font.pixelSize: Theme.fontCaption
                elide: Text.ElideRight
            }

            Repeater {
                model: [
                    { label: "Call now",      icon: "call",         danger: false, action: 0 },
                    { label: "Copy number",   icon: "content_copy", danger: false, action: 1 },
                    { label: "Delete entry",  icon: "delete",       danger: true,  action: 2 }
                ]
                delegate: Rectangle {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.preferredHeight: 62
                    radius: Theme.radiusM
                    color: actTap.pressed ? Qt.alpha(Theme.text, 0.08) : "transparent"

                    RowLayout {
                        anchors {
                            fill: parent
                            leftMargin: Theme.spaceS
                            rightMargin: Theme.spaceS
                        }
                        spacing: Theme.spaceM

                        Components.MaterialIcon {
                            name: modelData.icon
                            size: 26
                            color: modelData.danger ? Theme.danger : Theme.text
                        }
                        Text {
                            Layout.fillWidth: true
                            text: modelData.label
                            color: modelData.danger ? Theme.danger : Theme.text
                            font.pixelSize: Theme.fontBody - 2
                            font.weight: Font.Medium
                        }
                    }

                    TapHandler {
                        id: actTap
                        onTapped: {
                            var num = recentsMenu.rowNumber;
                            var idx = recentsMenu.rowIndex;
                            recentsMenu.close();
                            if (modelData.action === 0)
                                root.placeCall(num);
                            else if (modelData.action === 1)
                                root.copyToClipboard(num);
                            else if (idx >= 0 && idx < recentsModel.count)
                                recentsModel.remove(idx);
                        }
                    }
                }
            }
        }
    }

    // ---------------- floating toast ----------------
    Rectangle {
        id: toast
        z: 40
        anchors {
            bottom: parent.bottom
            bottomMargin: Theme.navZoneHeight + 104
            horizontalCenter: parent.horizontalCenter
        }
        width: toastText.implicitWidth + Theme.spaceL * 2
        height: 48
        radius: Theme.radiusPill
        color: Theme.surfaceHigh
        border.width: 1
        border.color: Qt.alpha(Theme.text, 0.14)
        opacity: 0
        visible: opacity > 0

        Behavior on opacity { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

        Text {
            id: toastText
            anchors.centerIn: parent
            color: Theme.text
            font.pixelSize: Theme.fontCaption
            font.weight: Font.Medium
        }

        Timer {
            id: toastTimer
            interval: 1600
            onTriggered: toast.opacity = 0
        }
    }

    // ---------------- recents model (demo entries) ----------------
    ListModel {
        id: recentsModel
        ListElement { name: "Alex";          number: "0812345678"; when: "09:41";     dir: "missed" }
        ListElement { name: "Mom";           number: "0865551234"; when: "Yesterday"; dir: "in"     }
        ListElement { name: "Grab Driver";   number: "0927778888"; when: "Yesterday"; dir: "out"    }
        ListElement { name: "Sarah Chen";    number: "0894442211"; when: "Monday";    dir: "in"     }
        ListElement { name: "Veridian Bank"; number: "021234567";  when: "Sunday";    dir: "out"    }
    }

    // ---------------- inline components ----------------
    component CallToggle: Column {
        id: ct
        property string iconName: ""
        property string caption: ""
        property bool checked: false
        signal tapped()

        spacing: 8

        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 72
            height: 72
            radius: 36
            color: ctTap.pressed
                   ? Qt.alpha(ShellPrefs.accentColor, 0.35)
                   : (ct.checked ? ShellPrefs.accentColor : Qt.alpha(Theme.text, 0.08))
            border.width: 1
            border.color: ct.checked ? ShellPrefs.accentColor : Qt.alpha(Theme.text, 0.10)
            scale: ctTap.pressed ? 0.92 : 1.0
            Behavior on scale { NumberAnimation { duration: Theme.animFast } }
            Behavior on color { ColorAnimation { duration: Theme.animFast } }

            Components.MaterialIcon {
                anchors.centerIn: parent
                name: ct.iconName
                size: 30
                color: ct.checked ? ShellPrefs.onAccentColor : Theme.text
            }

            TapHandler {
                id: ctTap
                onTapped: ct.tapped()
            }
        }

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: ct.caption
            color: ct.checked ? ShellPrefs.accentColor : Theme.textDim
            font.pixelSize: Theme.fontStatus - 1
            font.weight: ct.checked ? Font.DemiBold : Font.Normal
        }
    }

    // Reusable 4x3 tactile digit pad
    component PadGrid: GridLayout {
        id: padGrid
        property var keys: []
        property int keyHeight: 94
        signal keyPressed(string digit)
        signal keyLongPressed(string digit)

        columns: 3
        columnSpacing: 14
        rowSpacing: 12

        Repeater {
            model: padGrid.keys
            delegate: Rectangle {
                required property var modelData
                Layout.fillWidth: true
                Layout.preferredHeight: padGrid.keyHeight
                radius: Math.min(32, height / 2)
                color: keyTap.pressed
                       ? Qt.alpha(ShellPrefs.accentColor, 0.22)
                       : Theme.card
                border.width: 1
                border.color: keyTap.pressed
                              ? Qt.alpha(ShellPrefs.accentColor, 0.55)
                              : Qt.alpha(Theme.text, 0.08)
                scale: keyTap.pressed ? 0.93 : 1.0
                Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }
                Behavior on color { ColorAnimation { duration: Theme.animFast } }

                Column {
                    anchors.centerIn: parent
                    spacing: -2

                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: modelData.d
                        color: Theme.text
                        font.pixelSize: 38
                        font.weight: Font.Medium
                    }

                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        visible: modelData.l.length > 0 && (!modelData.icon || modelData.icon.length === 0)
                        text: modelData.l
                        color: Theme.textDim
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                        font.letterSpacing: 1.8
                    }

                    Components.MaterialIcon {
                        anchors.horizontalCenter: parent.horizontalCenter
                        visible: modelData.icon && modelData.icon.length > 0
                        name: modelData.icon || ""
                        size: 16
                        color: Theme.textFaint
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
            var s = Telephony.callState;
            if (s === "dialing" || s === "incoming") {
                root.callDir = s === "incoming" ? "in" : "out";
                root.callWasActive = false;
                root.lastCallNumber = Telephony.callNumber;
            } else if (s === "active") {
                root.callSeconds = 0;
                root.callWasActive = true;
            } else if (s === "") {
                if (root.lastCallNumber.length > 0) {
                    var d = root.callDir === "out" ? "out"
                          : (root.callWasActive ? "in" : "missed");
                    var matched = root.lookupContact(root.lastCallNumber);
                    recentsModel.insert(0, {
                        "name": matched ? matched.name : "",
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

    // --simulate-incoming trigger (1 s delay so UI is up first)
    Timer {
        id: simTrigger
        property string number: "0800123456"
        interval: 1000
        onTriggered: Telephony.simulateIncomingCall(simTrigger.number)
    }

    Component.onCompleted: {
        var args = Qt.application.arguments;
        for (var i = 0; i < args.length; ++i) {
            var a = String(args[i]);
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

    // Edge-back gesture overlay
    Components.AppGestureLayer {
        anchors.fill: parent
        z: 60
        onBackTriggered: {
            if (recentsMenu.visible)
                recentsMenu.close();
            else if (root.dtmfOpen)
                root.dtmfOpen = false;
            else if (root.currentTab !== 0)
                root.currentTab = 0;
            else
                root.close();
        }
    }

    // Hardware keyboard shortcuts
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
            if (recentsMenu.visible)
                recentsMenu.close();
            else if (root.dtmfOpen)
                root.dtmfOpen = false;
            else if (Telephony.callState !== "")
                Telephony.hangup();
        }
    }
}
