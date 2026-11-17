import QtQuick
import Quickshell
import Quickshell.Wayland
import A21s.Theme
import A21s.Services
import A21s.Windowing
import A21s.Widgets

// Reusable launcher tile: squircle/circle/glass icon + label, running-app indicator,
// container-morph launch origin, and — on the editable home surfaces — the long-press
// context menu and hold-to-drag gesture.
//
// `surface` decides everything interactive:
//   "desktop" / "dock" -> long-press opens the context menu, hold-and-drag moves it,
//                         the user's label/icon overrides apply
//   ""                 -> drawer/picker tile: tap to launch only
// DesktopView owns the menu/editor/picker and the drag target math; this component only
// reports gestures (window coordinates) and renders its own visual.
Item {
    id: appIcon
    required property var entry
    property bool labelVisible: true
    property int tileSize: Theme.iconApp
    property bool menuOpen: false
    signal launched()

    property string surface: ""       // "desktop" | "dock" | ""
    property int slotIndex: -1
    property bool interactive: true

    // Set while the visual is lifted into the drag ghost: the source slot reads as an
    // empty hole instead of a duplicate icon.
    property bool pickedUp: false

    signal menuRequested(real sceneX, real sceneY, real sceneW, real sceneH)
    signal dragStarted()
    signal dragMoved(real sceneX, real sceneY)
    signal dragDropped(real sceneX, real sceneY)
    signal dragCancelled()
    signal drawerGesture(real sceneY)
    signal drawerGestureEnded()

    // Home unlock entrance: DesktopView bumps `entranceKey` once per unlock and every
    // tile (grid + dock) rises/fades in together as one motion. Hosts other than the
    // home leave the defaults and are unaffected.
    property int entranceKey: 0
    property int entranceArmKey: 0
    property real entrance: 1.0

    opacity: entrance
    scale: 0.82 + 0.18 * entrance
    transform: Translate { y: (1.0 - entrance) * 32 }

    onEntranceKeyChanged: playEntrance()

    // Arm while the lock is up: the home surface is composited under the lock, so hiding
    // the tiles there removes the one-frame flash of the settled grid.
    onEntranceArmKeyChanged: {
        entranceAnim.stop()
        entrance = 0.0
    }

    function playEntrance() {
        entranceAnim.stop()
        entrance = 0.0
        entranceAnim.start()
    }

    NumberAnimation {
        id: entranceAnim
        target: appIcon
        property: "entrance"
        to: 1.0
        duration: 650
        easing.type: Easing.OutCubic
    }

    // Layout surfaces own the label/icon overrides and the long-press gesture...
    readonly property bool editable: surface === "desktop" || surface === "dock"
    // ...but only an interactive instance may start the menu/drag (the drag ghost may not).
    readonly property bool manipulable: interactive && editable

    readonly property string entryId: (entry && entry.id) ? String(entry.id) : ""
    readonly property string entryIconName: (entry && entry.icon) ? String(entry.icon) : ""
    readonly property string entryMaterialIcon: (entry && entry.materialIcon) ? String(entry.materialIcon) : ""
    readonly property string entryGlyph: (entry && entry.glyph) ? String(entry.glyph) : ""
    readonly property string appName: (entry && entry.name) ? String(entry.name) : ""

    // User renames/glyphs only apply to the home surfaces; the app drawer keeps real names.
    readonly property string overrideLabel: editable ? DesktopLayout.labelFor(entryId, "") : ""
    readonly property string overrideIcon: editable ? DesktopLayout.iconNameFor(entryId, "") : ""
    readonly property string entryName: overrideLabel.length > 0 ? overrideLabel : appName

    readonly property string resolvedMaterialIcon: {
        if (overrideIcon.length > 0)
            return overrideIcon
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
            "a21s-terminal":           "terminal",
            "a21s-spotify":            "headphones",
            "spotify":                 "headphones",
            "a21s-gallery":            "photo_library",
            "a21s-photos":             "photo_library",
            "org.kde.koko":            "photo_library",
            "koko":                    "photo_library",
            "org.kde.elisa":           "music_note",
            "elisa":                   "music_note",
            "a21s-music":              "music_note",
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
    // RabbizOS artwork is only for our own apps. Third-party entries keep the
    // name and icon their package ships (Chrome must stay "Google Chrome" with
    // Chrome's logo, KDE apps must keep theirs).
    readonly property string builtinIconName: {
        var icons = {
            "a21s-dialer": "a21s-dialer",
            "a21s-sms": "a21s-sms",
            "a21s-settings": "a21s-settings",
            "a21s-clock": "a21s-clock",
            "a21s-calculator": "a21s-calculator",
            "a21s-notes": "a21s-notes",
            "a21s-photos": "a21s-gallery",
            "a21s-music": "a21s-music",
            "a21s-terminal": "a21s-terminal"
        }
        return icons[entryId] || ""
    }

    // DesktopEntry.icon may be an icon-theme name, an absolute path or a URI.
    function resolveIcon(ref) {
        if (!ref || ref.length === 0)
            return ""
        if (ref.indexOf("://") >= 0)
            return ref
        if (ref.indexOf("/") === 0)
            return "file://" + ref
        var resolved = Quickshell.iconPath(ref, true)
        if (resolved.length === 0)
            return ""
        return resolved.indexOf("://") < 0 ? "file://" + resolved : resolved
    }
    readonly property bool hasBuiltinIcon: builtinIconName.length > 0
    // A glyph override replaces the artwork entirely.
    readonly property url iconUrl: overrideIcon.length > 0
        ? ""
        : (hasBuiltinIcon
            ? "file:///usr/share/icons/hicolor/scalable/apps/" + builtinIconName + ".svg"
            : resolveIcon(entryIconName))
    readonly property bool hasIconSource: iconUrl.toString().length > 0
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

    // Read-only escape hatches for the drag ghost (it paints a shadow around the tile).
    property alias tileItem: tile
    function tileSceneRect() {
        var p = tile.mapToItem(null, 0, 0)
        return Qt.rect(p.x, p.y, tile.width, tile.height)
    }

    function beginPickup() {
        pickedUp = true
    }
    function endPickup() {
        pickedUp = false
    }

    implicitWidth: tileSize + Theme.spaceL
    implicitHeight: tileSize + (labelVisible ? Theme.fontCaption : 0) + Theme.spaceS

    Timer {
        id: holdTimer
        interval: 420
        onTriggered: {
            if (!appIcon.manipulable || !pointer.pressed)
                return
            pointer.held = true
            var r = appIcon.tileSceneRect()
            appIcon.menuRequested(r.x, r.y, r.width, r.height)
        }
    }

    // Everything visible lives under `visual`, so the entrance transform / press scale
    // (root and tile) never fight the lift-into-the-drag-ghost fade.
    Item {
        id: visual
        anchors.fill: parent
        opacity: appIcon.pickedUp ? 0.16 : 1.0
        Behavior on opacity { NumberAnimation { duration: Theme.animFast } }

        Rectangle {
            id: tile
            anchors {
                top: parent.top
                horizontalCenter: parent.horizontalCenter
            }
            width: appIcon.tileSize
            height: appIcon.tileSize
            radius: appIcon.isCircle ? width / 2 : (Theme.radiusM + 4)

            // No card behind a real icon (the old Theme.card well + border was the
            // dark frame). A well is drawn only when there is no icon image at all
            // (glyph fallback) or for the explicit glass style.
            readonly property bool needsWell: !appIcon.isGlass && !appIcon.hasIconSource

            color: appIcon.isGlass
                ? Qt.alpha(Theme.surfaceHigh, 0.62)
                : (needsWell
                    ? (appIcon.isCircle ? Qt.alpha(Theme.text, 0.05) : Theme.card)
                    : "transparent")

            border.width: appIcon.menuOpen || appIcon.isGlass || needsWell ? 1 : 0
            border.color: appIcon.menuOpen
                ? ShellPrefs.accentColor
                : (appIcon.isGlass ? Qt.alpha(Theme.text, 0.16) : Qt.alpha(Theme.text, 0.08))

            Behavior on radius { NumberAnimation { duration: Theme.animMed; easing.type: Theme.animEase } }
            Behavior on border.color { ColorAnimation { duration: Theme.animFast } }

            // Subtle top-to-bottom sheen (disabled for glass so translucency stays clean).
            Rectangle {
                anchors.fill: parent
                radius: parent.radius
                visible: !appIcon.isGlass && parent.needsWell
                gradient: Gradient {
                    GradientStop { position: 0.0; color: Qt.alpha(Theme.text, 0.04) }
                    GradientStop { position: 1.0; color: "transparent" }
                }
            }

            Image {
                id: image
                anchors.centerIn: parent
                width: Math.round(parent.width * 0.90)
                height: Math.round(parent.height * 0.90)
                source: appIcon.iconUrl
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                smooth: true
                visible: status === Image.Ready
            }

            MaterialIcon {
                anchors.centerIn: parent
                visible: (!appIcon.hasIconSource || image.status === Image.Error)
                         && appIcon.resolvedMaterialIcon.length > 0
                name: appIcon.resolvedMaterialIcon
                size: Math.round(appIcon.tileSize * 0.46)
                color: Theme.text
            }

            Text {
                anchors.centerIn: parent
                visible: (!appIcon.hasIconSource || image.status === Image.Error)
                         && appIcon.resolvedMaterialIcon.length === 0
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
                opacity: pointer.pressed && !appIcon.pickedUp ? 0.14 : 0.0
                Behavior on opacity { NumberAnimation { duration: Theme.animFast } }
            }

            scale: (pointer.pressed && !appIcon.pickedUp) ? 0.88 : (appIcon.menuOpen ? 1.05 : 1.0)
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
            wrapMode: Text.NoWrap
            maximumLineCount: 1
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
    }

    // Tap to launch, hold for the menu, hold-and-drag to move, quick swipe up for the
    // drawer. A MouseArea (not handlers) so the same press can become all three.
    MouseArea {
        id: pointer
        anchors.fill: parent
        enabled: appIcon.interactive
        acceptedButtons: Qt.LeftButton | Qt.RightButton

        property point origin: Qt.point(0, 0)
        property bool held: false
        property bool moved: false
        property bool swiping: false
        property bool secondary: false

        function scenePoint(mx, my) {
            var p = pointer.mapToItem(null, mx, my)
            return { x: p.x, y: p.y }
        }

        onPressed: (mouse) => {
            origin = Qt.point(mouse.x, mouse.y)
            held = false
            moved = false
            swiping = false
            secondary = mouse.button === Qt.RightButton
            if (secondary) {
                // Mouse/keyboard form factors get the same menu on right click.
                if (appIcon.manipulable) {
                    var r = appIcon.tileSceneRect()
                    appIcon.menuRequested(r.x, r.y, r.width, r.height)
                }
                return
            }
            if (appIcon.manipulable)
                holdTimer.restart()
        }

        onPositionChanged: (mouse) => {
            if (!pressed)
                return
            var dx = mouse.x - origin.x
            var dy = mouse.y - origin.y
            var dist = Math.sqrt(dx * dx + dy * dy)
            if (dist > 12)
                moved = true

            if (appIcon.pickedUp) {
                var p = scenePoint(mouse.x, mouse.y)
                appIcon.dragMoved(p.x, p.y)
                return
            }
            if (held && moved && appIcon.manipulable) {
                // Long-press showed the menu; dragging now lifts the icon instead.
                holdTimer.stop()
                appIcon.beginPickup()
                appIcon.dragStarted()
                var start = scenePoint(mouse.x, mouse.y)
                appIcon.dragMoved(start.x, start.y)
                return
            }
            // A quick upward swipe that starts on an icon still opens the drawer.
            if (moved && !swiping && dy < -24 && Math.abs(dy) > Math.abs(dx) * 1.4) {
                swiping = true
                var s = scenePoint(mouse.x, mouse.y)
                appIcon.drawerGesture(s.y)
            } else if (swiping) {
                var s2 = scenePoint(mouse.x, mouse.y)
                appIcon.drawerGesture(s2.y)
            }
        }

        onReleased: (mouse) => {
            holdTimer.stop()
            if (secondary)
                return
            var p = scenePoint(mouse.x, mouse.y)
            if (appIcon.pickedUp) {
                appIcon.endPickup()
                appIcon.dragDropped(p.x, p.y)
            } else if (swiping) {
                appIcon.drawerGestureEnded()
            } else if (held) {
                // The context menu is open; releasing picks one of its actions.
            } else if (!moved) {
                appIcon.triggerLaunch()
            }
        }

        onCanceled: {
            holdTimer.stop()
            if (appIcon.pickedUp) {
                appIcon.endPickup()
                appIcon.dragCancelled()
            }
            if (swiping)
                appIcon.drawerGestureEnded()
        }
    }
}
