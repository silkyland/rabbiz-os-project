import QtQuick
import Quickshell
import Quickshell.Wayland
import A21s.Theme
import A21s.Services

// Reusable launcher tile: squircle/circle/glass icon + label.
// `entry` is normally a Quickshell DesktopEntry, but any object with
// `name`/`icon` (and optional `execString` / `action`) works.
Item {
    id: appIcon
    required property var entry
    property bool labelVisible: true
    property int tileSize: Theme.iconApp
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
            "system-file-manager":    "folder",
            "a21s-files":             "folder",
            "org.kde.dolphin":        "folder",
            "org.kde.calindori":      "calendar_month",
            "calindori":              "calendar_month",
            "accessories-calculator": "calculate",
            "a21s-calculator":        "calculate",
            "org.kde.kalk":           "calculate",
            "kalk":                   "calculate",
            "preferences-system-time": "schedule",
            "a21s-clock":             "schedule",
            "org.kde.kclock":         "schedule",
            "kclock":                 "schedule",
            "org.kde.kweather":       "wb_sunny",
            "kweather":               "wb_sunny",
            "org.kde.krecorder":      "graphic_eq",
            "krecorder":              "graphic_eq",
            "accessories-text-editor": "edit_note",
            "a21s-notes":             "edit_note",
            "org.kde.marknote":       "edit_note",
            "marknote":               "edit_note",
            "org.kde.qmlkonsole":     "terminal",
            "qmlkonsole":             "terminal",
            "utilities-terminal":     "terminal",
            "org.kde.elisa":          "music_note",
            "elisa":                  "music_note",
            "call-start":             "call",
            "a21s-dialer":            "call",
            "mail-message-new":       "sms",
            "a21s-sms":               "sms",
            "preferences-system":     "settings",
            "a21s-settings":          "settings",
            "steam":                  "sports_esports",
            "a21s-steam":             "sports_esports",
            "application-x-executable": "widgets"
        }
        if (iconMap[entryIconName])
            return iconMap[entryIconName]
        if (iconMap[entryId])
            return iconMap[entryId]
        return ""
    }
    readonly property url iconUrl: (entryIconName.length > 0 && resolvedMaterialIcon.length === 0 && !entryGlyph)
        ? Quickshell.iconPath(entryIconName, true) : ""
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

    implicitWidth: tileSize + Theme.spaceL
    implicitHeight: tileSize + (labelVisible ? Theme.fontCaption * 2 : 0) + Theme.spaceS

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
        border.color: appIcon.isGlass
            ? Qt.alpha(Theme.text, 0.16)
            : Qt.alpha(Theme.text, 0.08)

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
            width: Math.round(parent.width * 0.60)
            height: Math.round(parent.height * 0.60)
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
            opacity: tap.pressed ? 0.12 : 0.0
            Behavior on opacity { NumberAnimation { duration: Theme.animFast } }
        }

        scale: tap.pressed ? 0.90 : 1.0
        Behavior on scale {
            NumberAnimation {
                duration: Theme.animFast
                easing.type: Theme.animEase
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
        color: Theme.text
        font.pixelSize: Theme.fontCaption
        font.family: Theme.fontFamily
        font.weight: Font.Medium
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        wrapMode: Text.WordWrap
        maximumLineCount: 2
    }

    TapHandler {
        id: tap
        onTapped: {
            var tops = ToplevelManager.toplevels ? ToplevelManager.toplevels.values : [];
            var idLow = appIcon.entryId.toLowerCase();
            var nameLow = appIcon.entryName.toLowerCase();
            for (var i = 0; i < tops.length; ++i) {
                var t = tops[i];
                if (!t)
                    continue;
                var tid = String(t.appId || "").toLowerCase();
                var ttitle = String(t.title || "").toLowerCase();
                if ((idLow.length > 0 && (tid === idLow || tid.indexOf(idLow) >= 0))
                        || (nameLow.length > 0 && (ttitle === nameLow || ttitle.indexOf(nameLow) >= 0))
                        || (idLow === "a21s-files" && (tid.indexOf("dolphin") >= 0 || ttitle.indexOf("dolphin") >= 0))
                        || (idLow === "a21s-calculator" && (tid.indexOf("kalk") >= 0 || ttitle.indexOf("kalk") >= 0))
                        || (idLow === "a21s-clock" && (tid.indexOf("kclock") >= 0 || ttitle.indexOf("clock") >= 0))
                        || (idLow === "a21s-notes" && (tid.indexOf("marknote") >= 0 || ttitle.indexOf("marknote") >= 0))) {
                    t.minimized = false;
                    t.activate();
                    ShellState.activeAppId = t.appId || appIcon.entryId;
                    appIcon.launched();
                    return;
                }
            }
            if (appIcon.entry && typeof appIcon.entry.execute === "function")
                appIcon.entry.execute();
            else if (appIcon.entry && typeof appIcon.entry.onActivate === "function")
                appIcon.entry.onActivate();
            appIcon.launched();
        }
    }
}
