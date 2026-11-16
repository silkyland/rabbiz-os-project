import QtQuick
import Quickshell
import Quickshell.Wayland
import A21s.Theme
import A21s.Services
import A21s.Windowing
import A21s.Widgets

// Reusable launcher tile: squircle/circle/glass icon + label,
// running-app indicator pill, container-morph launch origin, and
// long-press quick action badge (Open / Close).
Item {
    id: appIcon
    required property var entry
    property bool labelVisible: true
    property int tileSize: Theme.iconApp
    property bool menuOpen: false
    signal launched()

    readonly property string entryId: (entry && entry.id) ? String(entry.id) : ""
    readonly property string entryName: (entry && entry.name) ? String(entry.name) : ""
    readonly property string entryIconName: (entry && entry.icon) ? String(entry.icon) : ""
    readonly property string entryMaterialIcon: (entry && entry.materialIcon) ? String(entry.materialIcon) : ""
    readonly property string entryGlyph: (entry && entry.glyph) ? String(entry.glyph) : ""
    readonly property string resolvedMaterialIcon: {
        if (entryMaterialIcon.length > 0)
            return entryMaterialIcon
        var iconMap = {
            "system-file-manager":     "folder",
            "a21s-files":              "folder",
            "org.kde.dolphin":         "folder",
            "org.kde.calindori":       "calendar_month",
            "calindori":               "calendar_month",
            "accessories-calculator":  "calculate",
            "a21s-calculator":         "calculate",
            "org.kde.kalk":            "calculate",
            "kalk":                    "calculate",
            "preferences-system-time": "schedule",
            "a21s-clock":              "schedule",
            "org.kde.kclock":          "schedule",
            "kclock":                  "schedule",
            "org.kde.kweather":        "wb_sunny",
            "kweather":                "wb_sunny",
            "org.kde.krecorder":       "graphic_eq",
            "krecorder":               "graphic_eq",
            "accessories-text-editor": "edit_note",
            "a21s-notes":              "edit_note",
            "org.kde.marknote":        "edit_note",
            "marknote":                "edit_note",
            "org.kde.qmlkonsole":      "terminal",
            "qmlkonsole":              "terminal",
            "utilities-terminal":      "terminal",
            "a21s-spotify":            "headphones",
            "spotify":                 "headphones",
            "a21s-gallery":            "photo_library",
            "org.kde.koko":            "photo_library",
            "koko":                    "photo_library",
            "org.kde.elisa":           "music_note",
            "elisa":                   "music_note",
            "call-start":              "call",
            "a21s-dialer":             "call",
            "mail-message-new":        "sms",
            "a21s-sms":                "sms",
            "preferences-system":      "settings",
            "a21s-settings":           "settings",
            "steam":                   "sports_esports",
            "a21s-steam":              "sports_esports",
            "application-x-executable": "widgets"
        }
        if (iconMap[entryIconName])
            return iconMap[entryIconName]
        if (iconMap[entryId])
            return iconMap[entryId]
        return ""
    }
    // Keep our app artwork independent of the selected system icon theme, including
    // home-screen placeholders whose DesktopEntry has not been discovered yet.
    readonly property bool hasRabbizIcon: ["a21s-dialer", "a21s-sms", "a21s-settings"].indexOf(entryId) >= 0
    readonly property url iconUrl: hasRabbizIcon
        ? "file:///usr/share/icons/hicolor/scalable/apps/" + entryId + ".svg"
        : ((entryIconName.length > 0 && resolvedMaterialIcon.length === 0 && !entryGlyph)
            ? Quickshell.iconPath(entryIconName, true) : "")
    readonly property bool isGlass: ShellPrefs.iconStyle === "glass"
    readonly property bool isCircle: ShellPrefs.iconStyle === "circle"

    readonly property color fallbackTint: {
        var palette = [
            "#4C8DFF", "#3DD9B6", "#9B7BFF",
            "#FF6B7A", "#FFB547", "#38BDF8"
        ]
        var h = 0
        for (var i = 0; i < entryName.length; ++i)
            h = (h * 31 + entryName.charCodeAt(i)) & 0x7fffffff
        return palette[h % palette.length]
    }

    function findRunningToplevel() { return Launcher.findToplevel(appIcon.entry) }

    readonly property var runningToplevel: findRunningToplevel()
    readonly property bool isRunning: runningToplevel !== null

    function triggerLaunch() {
        var p = tile.mapToItem(null, 0, 0);
        ShellState.activeAppId = appIcon.entryId || appIcon.entryName;
        Launcher.launch(appIcon.entry, {
            x: p.x, y: p.y, w: tile.width, h: tile.height, radius: tile.radius,
            name: appIcon.entryName, materialIcon: appIcon.resolvedMaterialIcon,
            iconUrl: appIcon.iconUrl, tint: appIcon.fallbackTint
        }, findRunningToplevel());
        appIcon.launched();
    }

    implicitWidth: tileSize + Theme.spaceL
    implicitHeight: tileSize + (labelVisible ? Theme.fontCaption * 2 : 0) + Theme.spaceS

    Connections {
        target: ShellState
        function onGoHome() { appIcon.menuOpen = false; }
    }

    Timer {
        id: menuAutoClose
        interval: 3500
        onTriggered: appIcon.menuOpen = false
    }

    Rectangle {
        id: tile
        anchors {
            top: parent.top
            horizontalCenter: parent.horizontalCenter
        }
        width: appIcon.tileSize
        height: appIcon.tileSize
        radius: appIcon.isCircle ? width / 2 : (Theme.radiusM + 4)

        color: appIcon.isGlass
            ? Qt.alpha(Theme.surfaceHigh, 0.62)
            : Theme.card

        border.width: 1
        border.color: appIcon.menuOpen
            ? ShellPrefs.accentColor
            : (appIcon.isGlass ? Qt.alpha(Theme.text, 0.16) : Qt.alpha(Theme.text, 0.08))

        Behavior on radius { NumberAnimation { duration: Theme.animMed; easing.type: Theme.animEase } }
        Behavior on border.color { ColorAnimation { duration: Theme.animFast } }

        // Subtle top-to-bottom sheen (disabled for glass so translucency stays clean).
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            visible: !appIcon.isGlass
            gradient: Gradient {
                GradientStop { position: 0.0; color: Qt.alpha(Theme.text, 0.04) }
                GradientStop { position: 1.0; color: "transparent" }
            }
        }

        Image {
            id: image
            anchors.centerIn: parent
            width: Math.round(parent.width * (appIcon.hasRabbizIcon ? 0.90 : 0.60))
            height: Math.round(parent.height * (appIcon.hasRabbizIcon ? 0.90 : 0.60))
            source: appIcon.iconUrl
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            smooth: true
            visible: status === Image.Ready
        }

        MaterialIcon {
            anchors.centerIn: parent
            visible: image.status !== Image.Ready && appIcon.resolvedMaterialIcon.length > 0
            name: appIcon.resolvedMaterialIcon
            size: Math.round(appIcon.tileSize * 0.46)
            color: Theme.text
        }

        Text {
            anchors.centerIn: parent
            visible: image.status !== Image.Ready && appIcon.resolvedMaterialIcon.length === 0
            text: appIcon.entryGlyph.length > 0
                ? appIcon.entryGlyph
                : (appIcon.entryName.length > 0 ? appIcon.entryName.charAt(0).toUpperCase() : "?")
            color: Theme.text
            font.pixelSize: Math.round(appIcon.tileSize * 0.40)
            font.family: Theme.fontFamily
            font.weight: Font.DemiBold
        }

        // Press highlight overlay.
        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: Theme.text
            opacity: tap.pressed ? 0.14 : 0.0
            Behavior on opacity { NumberAnimation { duration: Theme.animFast } }
        }

        scale: tap.pressed ? 0.88 : (appIcon.menuOpen ? 1.05 : 1.0)
        Behavior on scale {
            NumberAnimation {
                duration: Theme.animFast
                easing.type: Easing.OutBack
            }
        }
    }

    Text {
        id: label
        anchors {
            top: tile.bottom
            topMargin: Theme.spaceXS
            left: parent.left
            right: parent.right
        }
        visible: appIcon.labelVisible
        text: appIcon.entryName
        color: Theme.onWallpaper
        font.pixelSize: Theme.fontCaption
        font.family: Theme.fontFamily
        font.weight: Font.Medium
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        wrapMode: Text.WordWrap
        maximumLineCount: 2
    }

    // Running-app indicator pill (morphs width between 0 and 16px).
    Rectangle {
        anchors {
            top: appIcon.labelVisible ? label.bottom : tile.bottom
            topMargin: appIcon.labelVisible ? 3 : 4
            horizontalCenter: parent.horizontalCenter
        }
        width: appIcon.isRunning ? 16 : 0
        height: 4
        radius: 2
        color: ShellPrefs.accentColor
        opacity: appIcon.isRunning ? 1.0 : 0.0

        Behavior on width { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }
        Behavior on opacity { NumberAnimation { duration: Theme.animFast } }
    }

    TapHandler {
        id: tap
        onTapped: {
            if (appIcon.menuOpen) {
                appIcon.menuOpen = false;
                return;
            }
            appIcon.triggerLaunch();
        }
        onLongPressed: {
            appIcon.menuOpen = !appIcon.menuOpen;
            if (appIcon.menuOpen)
                menuAutoClose.restart();
            else
                menuAutoClose.stop();
        }
    }

    // Floating quick-action morph badge on long-press (Open / Close).
    Rectangle {
        id: actionBadge
        z: 30
        anchors {
            horizontalCenter: tile.horizontalCenter
            bottom: tile.bottom
            bottomMargin: 4
        }
        visible: opacity > 0.01
        opacity: appIcon.menuOpen ? 1.0 : 0.0
        scale: appIcon.menuOpen ? 1.0 : 0.72
        transformOrigin: Item.Bottom
        width: Math.max(tile.width + 12, badgeRow.implicitWidth + 16)
        height: 36
        radius: Theme.radiusPill
        color: Qt.alpha(Theme.surfaceHigh, 0.96)
        border.width: 1
        border.color: ShellPrefs.accentColor

        Behavior on opacity { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }
        Behavior on scale { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }

        Row {
            id: badgeRow
            anchors.centerIn: parent
            spacing: 8

            Rectangle {
                implicitWidth: openTxt.implicitWidth + 14
                implicitHeight: 26
                radius: Theme.radiusPill
                color: Qt.alpha(ShellPrefs.accentColor, 0.22)

                Text {
                    id: openTxt
                    anchors.centerIn: parent
                    text: "Open"
                    color: Theme.text
                    font.pixelSize: Theme.fontStatus - 2
                    font.family: Theme.fontFamily
                    font.weight: Font.DemiBold
                }

                TapHandler {
                    onTapped: {
                        appIcon.menuOpen = false;
                        menuAutoClose.stop();
                        appIcon.triggerLaunch();
                    }
                }
            }

            Rectangle {
                visible: appIcon.isRunning
                implicitWidth: closeTxt.implicitWidth + 14
                implicitHeight: 26
                radius: Theme.radiusPill
                color: Qt.alpha(Theme.danger, 0.22)

                Text {
                    id: closeTxt
                    anchors.centerIn: parent
                    text: "Close"
                    color: Theme.danger
                    font.pixelSize: Theme.fontStatus - 2
                    font.family: Theme.fontFamily
                    font.weight: Font.DemiBold
                }

                TapHandler {
                    onTapped: {
                        appIcon.menuOpen = false;
                        menuAutoClose.stop();
                        var t = appIcon.findRunningToplevel();
                        if (t)
                            t.close();
                    }
                }
            }
        }
    }
}
