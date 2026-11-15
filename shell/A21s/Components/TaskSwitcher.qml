import Quickshell
import Quickshell.Wayland
import QtQuick
import A21s.Theme
import A21s.Services

// Android-style recent-apps switcher. This is a full-screen layer-shell window
// of its own (NOT an overlay inside the nav strip). NavBar.qml owns the single
// instance and sets `open`; it may also be instantiated elsewhere if needed.
//
// Tap a card   -> activate that toplevel and close.
// Swipe up on a card -> close() that toplevel (compositor permitting).
// Tap the backdrop, swipe down, or press Escape -> close and go home.
PanelWindow {
    id: switcher

    // Set by NavBar's hold-gesture (or any integrator).
    property bool open: false

    // Close the switcher and return to the home screen.
    function close() {
        if (!open)
            return;
        open = false;
        ShellState.goHome();
    }

    // Activate a toplevel, remember it in ShellState, hide the switcher.
    function activateToplevel(t) {
        if (t) {
            t.minimized = false;
            t.activate();
            ShellState.activeAppId = t.appId || "";
        }
        open = false;
    }

    // Map a toplevel appId or title to a MaterialIconsRound name.
    function materialIconFor(appId, title) {
        var s = (String(appId || "") + " " + String(title || "")).toLowerCase();
        if (s.indexOf("dialer") >= 0 || s.indexOf("phone") >= 0) return "call";
        if (s.indexOf("sms") >= 0 || s.indexOf("message") >= 0) return "sms";
        if (s.indexOf("setting") >= 0) return "settings";
        if (s.indexOf("calc") >= 0) return "calculate";
        if (s.indexOf("clock") >= 0) return "schedule";
        if (s.indexOf("note") >= 0) return "edit_note";
        if (s.indexOf("dolphin") >= 0 || s.indexOf("files") >= 0) return "folder";
        if (s.indexOf("steam") >= 0) return "sports_esports";
        return "";
    }

    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.keyboardFocus: switcher.open ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    color: "transparent"
    visible: switcher.open

    onOpenChanged: {
        if (open)
            content.forceActiveFocus();
    }

    Connections {
        target: ShellState
        function onGoHome() { switcher.open = false; }
    }

    // Dimmed backdrop; taps outside the cards dismiss.
    Rectangle {
        anchors.fill: parent
        color: Theme.scrim
        opacity: switcher.open ? 1.0 : 0.0

        Behavior on opacity {
            NumberAnimation {
                duration: Theme.animMed
                easing.type: Theme.animEase
            }
        }

        MouseArea {
            anchors.fill: parent
            onClicked: switcher.close()
        }

        Item {
            id: content
            anchors.fill: parent
            focus: true
            Keys.onEscapePressed: switcher.close()

            ListView {
                id: cards
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(Theme.spaceXS,
                    Math.min(parent.width, count * (cellW + Theme.spaceM) - Theme.spaceM))
                height: cellH
                orientation: ListView.Horizontal
                spacing: Theme.spaceM
                clip: true
                model: ToplevelManager.toplevels

                // Card size in fractions of the screen — no magic pixels.
                readonly property real cellW: Math.min(parent.width * 0.62, Theme.iconApp * 4)
                readonly property real cellH: parent.height * 0.58

                delegate: Item {
                    id: cardDelegate
                    required property var modelData
                    width: cards.cellW
                    height: cards.cellH

                    readonly property var appEntry: modelData && modelData.appId
                        ? DesktopEntries.heuristicLookup(modelData.appId)
                        : null
                    readonly property string appTitle: modelData && modelData.title
                        ? String(modelData.title)
                        : (modelData && modelData.appId ? String(modelData.appId) : "")
                    readonly property string resolvedMaterialIcon: switcher.materialIconFor(
                        modelData ? modelData.appId : "", appTitle)
                    readonly property url appIconUrl: (appEntry && appEntry.icon && resolvedMaterialIcon.length === 0)
                        ? Quickshell.iconPath(String(appEntry.icon), true)
                        : ""

                    Rectangle {
                        id: card
                        width: cardDelegate.width
                        height: cardDelegate.height
                        radius: Theme.radiusL
                        color: Theme.card

                        // Active-window indicator.
                        Rectangle {
                            visible: cardDelegate.modelData && cardDelegate.modelData.activated
                            anchors {
                                top: parent.top
                                topMargin: Theme.spaceS
                                horizontalCenter: parent.horizontalCenter
                            }
                            width: Theme.iconS
                            height: Theme.navPillHeight
                            radius: Theme.radiusPill
                            color: Theme.accent
                        }

                        // Header row: icon + title. (No Column — positioners
                        // ignore anchors on their children.)
                        Row {
                            id: headerRow
                            anchors {
                                top: card.top
                                topMargin: Theme.spaceL
                                left: card.left
                                right: card.right
                                leftMargin: Theme.spaceM
                                rightMargin: Theme.spaceM
                            }
                            spacing: Theme.spaceS

                            // Small app icon (or first-letter fallback).
                            Rectangle {
                                width: Theme.iconS
                                height: Theme.iconS
                                radius: Theme.radiusS
                                color: Theme.surfaceHigh

                                Image {
                                    id: headerIcon
                                    anchors.centerIn: parent
                                    width: parent.width - Theme.spaceXS
                                    height: parent.height - Theme.spaceXS
                                    source: cardDelegate.appIconUrl
                                    fillMode: Image.PreserveAspectFit
                                    asynchronous: true
                                    visible: status === Image.Ready
                                }
                                MaterialIcon {
                                    anchors.centerIn: parent
                                    visible: headerIcon.status !== Image.Ready && cardDelegate.resolvedMaterialIcon.length > 0
                                    name: cardDelegate.resolvedMaterialIcon
                                    size: Theme.iconS * 0.65
                                    color: ShellPrefs.accentColor
                                }
                                Text {
                                    anchors.centerIn: parent
                                    visible: headerIcon.status !== Image.Ready && cardDelegate.resolvedMaterialIcon.length === 0
                                    text: cardDelegate.appTitle.length > 0
                                        ? cardDelegate.appTitle.charAt(0).toUpperCase()
                                        : "?"
                                    color: Theme.primary
                                    font.pixelSize: Theme.fontCaption
                                    font.family: Theme.fontFamily
                                    font.bold: true
                                }
                            }

                            Text {
                                width: card.width - Theme.iconS - Theme.spaceS - Theme.spaceM * 2
                                height: Theme.iconS
                                verticalAlignment: Text.AlignVCenter
                                text: cardDelegate.appTitle
                                color: Theme.text
                                font.pixelSize: Theme.fontCaption
                                font.family: Theme.fontFamily
                                elide: Text.ElideRight
                            }
                        }

                        // Large centered app icon (MaterialIcon / image / first-letter fallback).
                        Rectangle {
                            anchors.centerIn: card
                            width: Theme.iconApp * 1.15
                            height: Theme.iconApp * 1.15
                            radius: Theme.radiusL
                            color: Theme.surfaceHigh
                            border.width: 1
                            border.color: Qt.alpha(Theme.text, 0.08)

                            Image {
                                id: bodyIcon
                                anchors.centerIn: parent
                                width: parent.width - Theme.spaceM * 2
                                height: parent.height - Theme.spaceM * 2
                                source: cardDelegate.appIconUrl
                                fillMode: Image.PreserveAspectFit
                                asynchronous: true
                                visible: status === Image.Ready
                            }
                            MaterialIcon {
                                anchors.centerIn: parent
                                visible: bodyIcon.status !== Image.Ready && cardDelegate.resolvedMaterialIcon.length > 0
                                name: cardDelegate.resolvedMaterialIcon
                                size: Theme.iconApp * 0.58
                                color: ShellPrefs.accentColor
                            }
                            Text {
                                anchors.centerIn: parent
                                visible: bodyIcon.status !== Image.Ready && cardDelegate.resolvedMaterialIcon.length === 0
                                text: cardDelegate.appTitle.length > 0
                                    ? cardDelegate.appTitle.charAt(0).toUpperCase()
                                    : "?"
                                color: Theme.primary
                                font.pixelSize: Theme.fontHeadline
                                font.family: Theme.fontFamily
                                font.bold: true
                            }
                        }

                        Text {
                            anchors {
                                horizontalCenter: card.horizontalCenter
                                bottom: card.bottom
                                bottomMargin: Theme.spaceM
                            }
                            text: "tap to open · swipe up to close"
                            color: Theme.textFaint
                            font.pixelSize: Theme.fontCaption * 0.8
                            font.family: Theme.fontFamily
                        }

                        TapHandler {
                            onTapped: switcher.activateToplevel(cardDelegate.modelData)
                        }

                        // Swipe the card up to close that app.
                        DragHandler {
                            target: card
                            xAxis.enabled: false
                            yAxis.enabled: true
                            onActiveChanged: {
                                if (!active) {
                                    if (card.y < -card.height / 4) {
                                        if (cardDelegate.modelData)
                                            cardDelegate.modelData.close();
                                    } else {
                                        snapBack.start();
                                    }
                                }
                            }
                        }
                        NumberAnimation {
                            id: snapBack
                            target: card
                            property: "y"
                            to: 0
                            duration: Theme.animFast
                            easing.type: Theme.animEase
                        }
                    }
                }
            }

            Text {
                anchors.centerIn: parent
                visible: cards.count === 0
                text: "No recent apps"
                color: Theme.textDim
                font.pixelSize: Theme.fontBody
                font.family: Theme.fontFamily
            }

            Rectangle {
                anchors {
                    horizontalCenter: parent.horizontalCenter
                    bottom: parent.bottom
                    bottomMargin: Theme.navZoneHeight + Theme.spaceL
                }
                visible: cards.count > 0
                implicitWidth: clearRow.implicitWidth + Theme.spaceL * 1.4
                implicitHeight: 52
                radius: Theme.radiusPill
                color: Theme.surfaceHigh
                border.width: 1
                border.color: Qt.alpha(Theme.text, 0.14)
                scale: clearTap.pressed ? 0.94 : 1.0
                Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

                Row {
                    id: clearRow
                    anchors.centerIn: parent
                    spacing: 8

                    MaterialIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "clear_all"
                        size: 24
                        color: Theme.text
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "Clear all"
                        color: Theme.text
                        font.pixelSize: Theme.fontCaption
                        font.family: Theme.fontFamily
                        font.weight: Font.DemiBold
                    }
                }

                TapHandler {
                    id: clearTap
                    onTapped: {
                        var tops = ToplevelManager.toplevels ? ToplevelManager.toplevels.values : [];
                        for (var i = 0; i < tops.length; ++i) {
                            if (tops[i])
                                tops[i].close();
                        }
                        switcher.close();
                    }
                }
            }
        }
    }
}
