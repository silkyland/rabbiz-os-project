import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import A21s.Theme
import A21s.Services

// a21s-sms — Google-Messages-style SMS app for the A21s shell.
// Launched by the `a21s-sms` wrapper under `quickshell -p` (labwc
// maximizes the window, Android-style). All telephony goes through the
// shared A21s.Services Telephony singleton (demo mode until the cpif
// modem lands — M6).
ApplicationWindow {
    id: root

    visible: true
    width: 720
    height: 1600
    title: "Messages"
    color: Theme.bg

    palette {
        window: Theme.bg
        windowText: Theme.text
        base: Theme.surface
        text: Theme.text
        button: Theme.card
        buttonText: Theme.text
        highlight: Theme.primary
        highlightedText: Theme.primaryContent
        placeholderText: Theme.textFaint
        toolTipBase: Theme.surfaceHigh
        toolTipText: Theme.text
    }

    // ---- State -----------------------------------------------------------

    // Number of the conversation currently on screen, "" when the thread
    // list is showing. Incoming SMS only bumps the unread badge when the
    // thread is not being viewed.
    property string activeThread: ""

    // phone number -> ListModel of {body, incoming, ts, demo}. Created lazily
    // so per-thread history survives navigation.
    property var convStores: ({})

    ListModel {
        id: threadsModel
        // roles: number, name, unread, lastBody, lastTs
    }

    Component {
        id: convModelFactory
        ListModel {}
    }

    // ---- Store helpers ----------------------------------------------------

    function threadIndex(number) {
        for (var i = 0; i < threadsModel.count; ++i) {
            if (threadsModel.get(i).number === number)
                return i;
        }
        return -1;
    }

    function ensureThread(number, name) {
        var i = threadIndex(number);
        if (i >= 0)
            return i;
        threadsModel.insert(0, {
            "number": number,
            "name": name,
            "unread": 0,
            "lastBody": "",
            "lastTs": new Date(0)
        });
        return 0;
    }

    function convModelFor(number) {
        if (convStores[number] === undefined)
            convStores[number] = convModelFactory.createObject(root);
        return convStores[number];
    }

    function appendMessage(number, body, incoming, ts, demo) {
        var i = ensureThread(number, "");
        convModelFor(number).append({
            "body": body,
            "incoming": incoming,
            "ts": ts,
            "demo": demo
        });
        var row = threadsModel.get(i);
        threadsModel.setProperty(i, "lastBody", body);
        threadsModel.setProperty(i, "lastTs", ts);
        if (incoming && number !== activeThread)
            threadsModel.setProperty(i, "unread", row.unread + 1);
        // Most recent conversation on top.
        if (i > 0) {
            var top = threadsModel.get(i);
            threadsModel.remove(i);
            threadsModel.insert(0, top);
        }
    }

    // Incoming path — also reached by the demo reply simulation, which
    // re-emits Telephony.smsReceived so everything flows through the one
    // signal handler.
    function receiveSms(number, body) {
        var m = convModelFor(number);
        if (m.count > 0) {
            var last = m.get(m.count - 1);
            // Demo backends may echo our own send back as an "incoming"
            // message. Outgoing entries are marked demo:true so we can
            // drop the duplicate.
            if (!last.incoming && last.demo && last.body === body)
                return;
        }
        appendMessage(number, body, true, new Date(), false);
    }

    function sendMessage(number, body) {
        // The Telephony backend owns delivery. In demoMode it appends the
        // outgoing copy itself and, ~1.2s later, re-emits smsReceived with an
        // "Echo: ..." body — which arrives here through the same signal path
        // a real modem message would, so no extra simulation is needed.
        Telephony.sendSms(number, body);
        appendMessage(number, body, false, new Date(), Telephony.demoMode);
    }

    function deleteMessage(number, index) {
        var m = convModelFor(number);
        m.remove(index);
        var i = threadIndex(number);
        if (i < 0)
            return;
        if (m.count > 0) {
            var last = m.get(m.count - 1);
            threadsModel.setProperty(i, "lastBody", last.body);
            threadsModel.setProperty(i, "lastTs", last.ts);
        } else {
            threadsModel.setProperty(i, "lastBody", "");
            threadsModel.setProperty(i, "lastTs", new Date(0));
        }
    }

    function deleteThread(number) {
        var i = threadIndex(number);
        if (i >= 0)
            threadsModel.remove(i);
        var m = convStores[number];
        if (m !== undefined) {
            m.destroy();
            delete convStores[number];
        }
        if (activeThread === number)
            stack.pop();
    }

    function openThread(number) {
        var i = ensureThread(number, "");
        threadsModel.setProperty(i, "unread", 0);
        var name = threadsModel.get(i).name;
        activeThread = number;
        stack.push(convComponent, {
            "threadNumber": number,
            "threadName": name
        });
    }

    function fmtThreadTime(d) {
        var now = new Date();
        if (d.toDateString() === now.toDateString())
            return Qt.formatTime(d, "hh:mm");
        return Qt.formatDate(d, "dd MMM");
    }

    function showToast(text) {
        toastText.text = text;
        toast.opacity = 1;
        toastTimer.restart();
    }

    function copyToClipboard(text) {
        clipHelper.text = text;
        clipHelper.selectAll();
        clipHelper.copy();
        clipHelper.deselect();
        showToast("Copied to clipboard");
    }

    // ---- Demo seed data ----------------------------------------------------

    function seedDemo() {
        var now = Date.now();
        var MIN = 60000;
        // Seeded oldest-first so appendMessage()'s reorder-to-top leaves
        // the freshest conversation (Alex) at the head of the list.
        ensureThread("226563", "Veridian Bank");
        appendMessage("226563",
                      "Alert: a purchase of $42.10 at CITY MARKET was approved on card ••4512.",
                      true, new Date(now - 190 * MIN), true);
        appendMessage("226563",
                      "Your current balance is $1,204.55.",
                      true, new Date(now - 140 * MIN), true);
        threadsModel.setProperty(threadIndex("226563"), "unread", 2);

        appendMessage("+1 555-0102",
                      "Hi sweetie, are we still on for Sunday dinner?",
                      true, new Date(now - 90 * MIN), true);
        var mom = ensureThread("+1 555-0102", "");
        threadsModel.setProperty(mom, "name", "Mom");
        appendMessage("+1 555-0102", "Yes! I'll bring dessert.",
                      false, new Date(now - 85 * MIN), true);
        appendMessage("+1 555-0102", "Can't wait — Dad is making his famous stew.",
                      true, new Date(now - 32 * MIN), true);
        threadsModel.setProperty(threadIndex("+1 555-0102"), "unread", 1);

        appendMessage("+1 555-0147", "you still on for tonight?",
                      true, new Date(now - 25 * MIN), true);
        var alex = ensureThread("+1 555-0147", "");
        threadsModel.setProperty(alex, "name", "Alex");
        appendMessage("+1 555-0147", "maybe, work ran late. give me 30",
                      false, new Date(now - 12 * MIN), true);
        appendMessage("+1 555-0147", "ok but we start at 7, don't be late",
                      true, new Date(now - 5 * MIN), true);
    }

    Component.onCompleted: {
        // Pull anything the shared Telephony backend already holds.
        var t = Telephony.threads;
        for (var k = 0; k < t.length; ++k) {
            var e = t[k];
            var when = e.timestamp ? new Date(e.timestamp) : new Date();
            appendMessage(e.number, e.body, e.incoming, when, e.demo);
        }
        if (threadsModel.count === 0)
            seedDemo();
    }

    Connections {
        target: Telephony

        function onSmsReceived(number, body) {
            root.receiveSms(number, body);
        }
    }

    // ---- Shared UI bits ----------------------------------------------------

    // Hidden clipboard workhorse — pure-QML copy to clipboard.
    TextEdit {
        id: clipHelper
        visible: false
        width: 0
        height: 0
    }

    // Message context popup (copy / delete).
    Popup {
        id: msgMenu

        property string msgNumber: ""
        property int msgIndex: -1
        property string msgBody: ""

        function openFor(number, index, body) {
            msgNumber = number;
            msgIndex = index;
            msgBody = body;
            open();
        }

        modal: true
        width: Theme.iconApp * 3
        padding: Theme.spaceXS
        x: Math.round((root.width - width) / 2)
        y: Math.round((root.height - height) / 2)
        background: Rectangle {
            color: Theme.surfaceHigh
            radius: Theme.radiusM
            border.color: Theme.separator
        }

        contentItem: ColumnLayout {
            spacing: 0

            Repeater {
                model: ["Copy", "Delete"]

                delegate: Item {
                    id: menuItem

                    required property string modelData
                    required property int index

                    Layout.fillWidth: true
                    Layout.preferredHeight: Theme.iconM + Theme.spaceS

                    Text {
                        anchors {
                            verticalCenter: parent.verticalCenter
                            left: parent.left
                            leftMargin: Theme.spaceM
                        }
                        text: menuItem.modelData
                        color: menuItem.index === 1 ? Theme.danger : Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontBody
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: {
                            msgMenu.close();
                            if (menuItem.index === 0)
                                root.copyToClipboard(msgMenu.msgBody);
                            else
                                root.deleteMessage(msgMenu.msgNumber, msgMenu.msgIndex);
                        }
                    }
                }
            }
        }
    }

    // Thread context popup (delete conversation).
    Popup {
        id: threadMenu

        property string number: ""

        function openFor(n) {
            number = n;
            open();
        }

        modal: true
        width: Theme.iconApp * 4
        padding: Theme.spaceXS
        x: Math.round((root.width - width) / 2)
        y: Math.round((root.height - height) / 2)
        background: Rectangle {
            color: Theme.surfaceHigh
            radius: Theme.radiusM
            border.color: Theme.separator
        }

        contentItem: Item {
            implicitHeight: Theme.iconM + Theme.spaceS

            Text {
                anchors {
                    verticalCenter: parent.verticalCenter
                    left: parent.left
                    leftMargin: Theme.spaceM
                }
                text: "Delete conversation"
                color: Theme.danger
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
            }
            MouseArea {
                anchors.fill: parent
                onClicked: {
                    threadMenu.close();
                    root.deleteThread(threadMenu.number);
                }
            }
        }
    }

    // Toast.
    Rectangle {
        id: toast

        anchors {
            bottom: parent.bottom
            bottomMargin: Theme.navZoneHeight + Theme.spaceXL
            horizontalCenter: parent.horizontalCenter
        }
        width: toastText.implicitWidth + Theme.spaceL * 2
        height: Theme.iconM + Theme.spaceXS
        radius: Theme.radiusPill
        color: Theme.surfaceHigh
        border.color: Theme.separator
        opacity: 0
        visible: opacity > 0

        Behavior on opacity {
            NumberAnimation {
                duration: Theme.animFast
                easing.type: Theme.animEase
            }
        }

        Text {
            id: toastText
            anchors.centerIn: parent
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontCaption
        }
        Timer {
            id: toastTimer
            interval: 1500
            onTriggered: toast.opacity = 0
        }
    }

    // "DEMO" badge shown in app bars while Telephony is in demoMode.
    Component {
        id: demoChip

        Rectangle {
            visible: Telephony.demoMode
            implicitWidth: demoLabel.implicitWidth + Theme.spaceM
            implicitHeight: Theme.fontCaption + Theme.spaceS
            radius: Theme.radiusPill
            color: Theme.warn

            Text {
                id: demoLabel
                anchors.centerIn: parent
                text: "DEMO"
                color: Theme.bg
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontCaption
                font.bold: true
            }
        }
    }

    // ---- New chat dialog --------------------------------------------------

    Dialog {
        id: newChatDialog

        modal: true
        x: Math.round((root.width - width) / 2)
        y: Math.round((root.height - height) / 2)
        width: root.width - Theme.spaceXL * 2
        padding: Theme.spaceL
        background: Rectangle {
            color: Theme.surface
            radius: Theme.radiusL
            border.color: Theme.separator
        }
        onOpened: numField.forceActiveFocus()
        onClosed: {
            numField.text = "";
            firstMsg.text = "";
        }

        contentItem: ColumnLayout {
            spacing: Theme.spaceM

            Text {
                text: "New chat"
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontTitle
                Layout.bottomMargin: Theme.spaceS
            }
            TextField {
                id: numField
                Layout.fillWidth: true
                placeholderText: "Phone number"
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                color: Theme.text
                inputMethodHints: Qt.ImhDialableCharactersOnly
                background: Rectangle {
                    color: Theme.card
                    radius: Theme.radiusM
                }
            }
            TextArea {
                id: firstMsg
                Layout.fillWidth: true
                Layout.preferredHeight: Theme.fontBody * 4
                placeholderText: "Text message"
                wrapMode: Text.Wrap
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontBody
                color: Theme.text
                background: Rectangle {
                    color: Theme.card
                    radius: Theme.radiusM
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: Theme.spaceS
                spacing: Theme.spaceM

                Item { Layout.fillWidth: true }
                Button {
                    flat: true
                    text: "Cancel"
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                    onClicked: newChatDialog.close()
                }
                Button {
                    id: sendChatBtn

                    text: "Send"
                    enabled: numField.text.trim().length > 0
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontBody
                    background: Rectangle {
                        color: sendChatBtn.enabled ? Theme.primary : Theme.tileOff
                        radius: Theme.radiusPill
                    }
                    contentItem: Text {
                        text: sendChatBtn.text
                        color: sendChatBtn.enabled ? Theme.primaryContent : Theme.tileOffText
                        font: sendChatBtn.font
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        leftPadding: Theme.spaceM
                        rightPadding: Theme.spaceM
                    }
                    onClicked: {
                        var n = numField.text.trim();
                        var b = firstMsg.text.trim();
                        newChatDialog.close();
                        if (b.length > 0)
                            root.sendMessage(n, b);
                        root.openThread(n);
                    }
                }
            }
        }
    }

    // ---- Views -------------------------------------------------------------

    StackView {
        id: stack

        anchors.fill: parent
        initialItem: threadListComponent
        onDepthChanged: {
            if (depth === 1)
                root.activeThread = "";
        }
    }

    // Thread list.
    Component {
        id: threadListComponent

        Item {
            ColumnLayout {
                anchors.fill: parent
                spacing: 0

                // App bar.
                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Theme.iconL + Theme.spaceL
                    color: Theme.bg

                    RowLayout {
                        anchors {
                            fill: parent
                            leftMargin: Theme.spaceL
                            rightMargin: Theme.spaceM
                        }
                        spacing: Theme.spaceM

                        Text {
                            text: "Messages"
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontTitle
                            font.bold: true
                        }
                        Item { Layout.fillWidth: true }
                        Loader {
                            sourceComponent: demoChip
                            Layout.alignment: Qt.AlignVCenter
                        }
                    }
                    Rectangle {
                        anchors {
                            left: parent.left
                            right: parent.right
                            bottom: parent.bottom
                        }
                        height: 1
                        color: Theme.separator
                    }
                }

                ListView {
                    id: threadView

                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: threadsModel
                    spacing: 0

                    delegate: Item {
                        id: threadRow

                        required property int index
                        required property string number
                        required property string name
                        required property int unread
                        required property string lastBody
                        required property date lastTs

                        width: ListView.view.width
                        height: Theme.iconL + Theme.spaceM * 2

                        Rectangle {
                            anchors.fill: parent
                            color: Theme.surfaceHigh
                            opacity: rowMouse.pressed ? 0.6 : 0
                        }

                        RowLayout {
                            anchors {
                                fill: parent
                                leftMargin: Theme.spaceL
                                rightMargin: Theme.spaceL
                            }
                            spacing: Theme.spaceM

                            // Avatar.
                            Rectangle {
                                Layout.preferredWidth: Theme.iconL
                                Layout.preferredHeight: Theme.iconL
                                Layout.alignment: Qt.AlignVCenter
                                radius: Theme.radiusPill
                                color: threadRow.unread > 0 ? Theme.primary : Theme.tileOn

                                Text {
                                    anchors.centerIn: parent
                                    text: (threadRow.name.length > 0
                                           ? threadRow.name
                                           : threadRow.number).charAt(0).toUpperCase()
                                    color: threadRow.unread > 0 ? Theme.primaryContent : Theme.tileOnText
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontTitle
                                    font.bold: true
                                }
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignVCenter
                                spacing: Theme.spaceXS / 2

                                Text {
                                    Layout.fillWidth: true
                                    text: threadRow.name.length > 0 ? threadRow.name : threadRow.number
                                    color: Theme.text
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontBody
                                    font.bold: threadRow.unread > 0
                                    elide: Text.ElideRight
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: threadRow.lastBody
                                    color: threadRow.unread > 0 ? Theme.text : Theme.textDim
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontCaption
                                    elide: Text.ElideRight
                                    maximumLineCount: 1
                                }
                            }

                            ColumnLayout {
                                Layout.alignment: Qt.AlignTop | Qt.AlignRight
                                Layout.topMargin: Theme.spaceS
                                spacing: Theme.spaceXS

                                Text {
                                    Layout.alignment: Qt.AlignRight
                                    text: threadRow.lastBody.length > 0 ? root.fmtThreadTime(threadRow.lastTs) : ""
                                    color: threadRow.unread > 0 ? Theme.primary : Theme.textFaint
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontCaption
                                    font.bold: threadRow.unread > 0
                                }
                                Rectangle {
                                    Layout.alignment: Qt.AlignRight
                                    visible: threadRow.unread > 0
                                    implicitWidth: Math.max(countText.implicitWidth + Theme.spaceS,
                                                            Theme.fontCaption + Theme.spaceXS)
                                    implicitHeight: Theme.fontCaption + Theme.spaceXS
                                    radius: Theme.radiusPill
                                    color: Theme.primary

                                    Text {
                                        id: countText
                                        anchors.centerIn: parent
                                        text: threadRow.unread
                                        color: Theme.primaryContent
                                        font.family: Theme.fontFamily
                                        font.pixelSize: Theme.fontCaption
                                        font.bold: true
                                    }
                                }
                            }
                        }

                        Rectangle {
                            anchors {
                                left: parent.left
                                right: parent.right
                                bottom: parent.bottom
                                leftMargin: Theme.iconL + Theme.spaceL * 2
                            }
                            height: 1
                            color: Theme.separator
                        }

                        MouseArea {
                            id: rowMouse
                            anchors.fill: parent
                            onClicked: root.openThread(threadRow.number)
                            onPressAndHold: threadMenu.openFor(threadRow.number)
                        }
                    }
                }
            }

            // FAB — new chat.
            Rectangle {
                anchors {
                    right: parent.right
                    bottom: parent.bottom
                    margins: Theme.spaceL
                }
                width: Theme.iconApp
                height: Theme.iconApp
                radius: Theme.radiusPill
                color: Theme.primary

                Text {
                    anchors.centerIn: parent
                    text: "+"
                    color: Theme.primaryContent
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontHeadline
                }
                MouseArea {
                    anchors.fill: parent
                    onClicked: newChatDialog.open()
                }
            }
        }
    }

    // Conversation view.
    Component {
        id: convComponent

        Item {
            id: convPage

            // Accept key events (e.g. system Back) while this page is up.
            focus: true

            property string threadNumber: ""
            property string threadName: ""

            Keys.onBackPressed: stack.pop()

            function doSend() {
                var body = composer.text.trim();
                if (body.length === 0)
                    return;
                composer.text = "";
                root.sendMessage(threadNumber, body);
            }

            ColumnLayout {
                anchors.fill: parent
                spacing: 0

                // App bar.
                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Theme.iconL + Theme.spaceL
                    color: Theme.bg

                    RowLayout {
                        anchors {
                            fill: parent
                            leftMargin: Theme.spaceM
                            rightMargin: Theme.spaceM
                        }
                        spacing: Theme.spaceM

                        // Back chevron — drawn, no font dependency.
                        Item {
                            id: backBtn

                            Layout.preferredWidth: Theme.iconM
                            Layout.preferredHeight: Theme.iconM
                            Layout.alignment: Qt.AlignVCenter

                            Item {
                                id: backIcon

                                anchors.centerIn: backBtn
                                width: Theme.iconS
                                height: Theme.iconS

                                property real px: width * 0.32
                                property real py: height * 0.5
                                property real len: width * 0.5
                                property real th: 4

                                Rectangle {
                                    width: backIcon.len
                                    height: backIcon.th
                                    radius: backIcon.th / 2
                                    color: Theme.text
                                    x: backIcon.px + backIcon.len * 0.354 - backIcon.len / 2
                                    y: backIcon.py - backIcon.len * 0.354 - backIcon.th / 2
                                    rotation: -45
                                }
                                Rectangle {
                                    width: backIcon.len
                                    height: backIcon.th
                                    radius: backIcon.th / 2
                                    color: Theme.text
                                    x: backIcon.px + backIcon.len * 0.354 - backIcon.len / 2
                                    y: backIcon.py + backIcon.len * 0.354 - backIcon.th / 2
                                    rotation: 45
                                }
                            }
                            MouseArea {
                                anchors.fill: parent
                                onClicked: stack.pop()
                            }
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.alignment: Qt.AlignVCenter
                            spacing: 0

                            Text {
                                Layout.fillWidth: true
                                text: convPage.threadName.length > 0 ? convPage.threadName : convPage.threadNumber
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody
                                font.bold: true
                                elide: Text.ElideRight
                            }
                            Text {
                                Layout.fillWidth: true
                                visible: convPage.threadName.length > 0
                                text: convPage.threadNumber
                                color: Theme.textDim
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontCaption
                                elide: Text.ElideRight
                            }
                        }

                        Loader {
                            sourceComponent: demoChip
                            Layout.alignment: Qt.AlignVCenter
                        }
                    }
                    Rectangle {
                        anchors {
                            left: parent.left
                            right: parent.right
                            bottom: parent.bottom
                        }
                        height: 1
                        color: Theme.separator
                    }
                }

                // Messages.
                ListView {
                    id: msgView

                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: Theme.spaceXS
                    model: root.convModelFor(convPage.threadNumber)
                    topMargin: Theme.spaceM
                    bottomMargin: Theme.spaceS

                    Component.onCompleted: Qt.callLater(positionViewAtEnd)
                    onCountChanged: Qt.callLater(positionViewAtEnd)

                    delegate: Item {
                        id: msgRow

                        required property int index
                        required property string body
                        required property bool incoming
                        required property date ts
                        required property bool demo

                        // New timestamp chip when a group starts: first
                        // message, >5 min gap, or a day change.
                        readonly property bool groupStart: {
                            if (index === 0)
                                return true;
                            var prev = ListView.view.model.get(index - 1);
                            if (prev.ts.toDateString() !== ts.toDateString())
                                return true;
                            return (ts - prev.ts) > 5 * 60 * 1000;
                        }

                        width: ListView.view.width
                        height: (stamp.visible ? stamp.implicitHeight + Theme.spaceXS : 0)
                                + bub.implicitHeight

                        Text {
                            id: stamp
                            anchors {
                                top: parent.top
                                horizontalCenter: parent.horizontalCenter
                            }
                            visible: msgRow.groupStart
                            text: {
                                var now = new Date();
                                if (msgRow.ts.toDateString() === now.toDateString())
                                    return Qt.formatTime(msgRow.ts, "hh:mm");
                                return Qt.formatDate(msgRow.ts, "dd MMM") + "  "
                                        + Qt.formatTime(msgRow.ts, "hh:mm");
                            }
                            color: Theme.textFaint
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontCaption
                        }

                        MessageBubble {
                            id: bub
                            anchors {
                                top: stamp.visible ? stamp.bottom : parent.top
                                topMargin: Theme.spaceXS
                                left: parent.left
                                right: parent.right
                                leftMargin: Theme.spaceM
                                rightMargin: Theme.spaceM
                            }
                            body: msgRow.body
                            incoming: msgRow.incoming
                            maxBubbleWidth: width * 0.75
                            onLongPressed: msgMenu.openFor(convPage.threadNumber,
                                                           msgRow.index, msgRow.body)
                        }
                    }
                }

                // Composer.
                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: composer.implicitHeight + Theme.spaceM * 2
                    color: Theme.surface

                    RowLayout {
                        anchors {
                            fill: parent
                            leftMargin: Theme.spaceM
                            rightMargin: Theme.spaceM
                        }
                        spacing: Theme.spaceM

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: composer.implicitHeight + Theme.spaceS
                            Layout.alignment: Qt.AlignVCenter
                            Layout.maximumHeight: Theme.fontBody * 5
                            radius: Math.min(Theme.radiusPill, height / 2)
                            color: Theme.card

                            TextArea {
                                id: composer
                                anchors {
                                    fill: parent
                                    leftMargin: Theme.spaceM
                                    rightMargin: Theme.spaceM
                                }
                                placeholderText: "Text message"
                                wrapMode: Text.Wrap
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontBody
                                verticalAlignment: Text.AlignVCenter
                                // OSK (squeekboard) is a system-level
                                // follow-up; the field is still focusable.
                                inputMethodHints: Qt.ImhNone

                                function handleEnter(event) {
                                    if (event.modifiers & Qt.ShiftModifier) {
                                        event.accepted = false;
                                    } else {
                                        event.accepted = true;
                                        convPage.doSend();
                                    }
                                }
                                Keys.onReturnPressed: (event) => handleEnter(event)
                                Keys.onEnterPressed: (event) => handleEnter(event)
                            }
                        }

                        // Send FAB — drawn ">" arrow, no font dependency.
                        Rectangle {
                            id: sendBtn

                            Layout.preferredWidth: Theme.iconM + Theme.spaceS
                            Layout.preferredHeight: Theme.iconM + Theme.spaceS
                            Layout.alignment: Qt.AlignBottom | Qt.AlignRight
                            Layout.bottomMargin: Theme.spaceXS
                            radius: Theme.radiusPill
                            color: composer.text.trim().length > 0 ? Theme.primary : Theme.tileOff

                            Item {
                                id: sendIcon

                                anchors.centerIn: sendBtn
                                width: Theme.iconS * 0.6
                                height: Theme.iconS * 0.6

                                property real px: width * 0.7
                                property real py: height * 0.5
                                property real len: width * 0.75
                                property real th: 4
                                property color ink: composer.text.trim().length > 0
                                                    ? Theme.primaryContent : Theme.tileOffText

                                Rectangle {
                                    width: sendIcon.len
                                    height: sendIcon.th
                                    radius: sendIcon.th / 2
                                    color: sendIcon.ink
                                    x: sendIcon.px - sendIcon.len * 0.354 - sendIcon.len / 2
                                    y: sendIcon.py - sendIcon.len * 0.354 - sendIcon.th / 2
                                    rotation: 45
                                }
                                Rectangle {
                                    width: sendIcon.len
                                    height: sendIcon.th
                                    radius: sendIcon.th / 2
                                    color: sendIcon.ink
                                    x: sendIcon.px - sendIcon.len * 0.354 - sendIcon.len / 2
                                    y: sendIcon.py + sendIcon.len * 0.354 - sendIcon.th / 2
                                    rotation: -45
                                }
                            }
                            MouseArea {
                                anchors.fill: parent
                                enabled: composer.text.trim().length > 0
                                onClicked: convPage.doSend()
                            }
                        }
                    }
                }
            }
        }
    }
}
