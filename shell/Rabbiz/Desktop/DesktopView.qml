import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import Quickshell.Services.UPower
import QtQuick
import QtQuick.Layouts
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets
import Rabbiz.Windowing

// ~/Desktop — the home page. 720x1600 artwork wallpaper, tappable morphing clock +
// telemetry At-a-Glance pill, the editable desktop icon grid, the swipeable Hero Deck
// (RabbizOS welcome / MPRIS Media / System Boost), flying Search pill morph, frosted
// dock, AppDrawer & AppLaunchMorph overlay.
//
// The home page IS ~/Desktop (DesktopLayout.surfaceLabel): grid icons are the files on
// the desktop, dock icons are the pinned favourites. Both surfaces are user-editable —
// long-press an icon for its menu (open / rename / move / remove), hold and drag to
// reorder or to move between grid and dock, and the "+" tile at the end of either
// surface adds an installed app. Membership/order/overrides live in DesktopLayout.qml
// (~/.config/rabbiz-shell/desktop.json); this file owns the surfaces and the interaction.
PanelWindow {
    id: home

    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Background
    // The drawer, the icon editor and the add-app picker all own a text field.
    readonly property bool typingOverlay: editorSheet.open || appPicker.open
    WlrLayershell.keyboardFocus: (ShellState.drawerOpen || typingOverlay)
        ? WlrKeyboardFocus.OnDemand
        : WlrKeyboardFocus.None
    color: Theme.bg

    SystemClock {
        id: clock
        precision: SystemClock.Minutes
    }

    Process {
        id: settingsProc
        environment: Appearance.appEnvironment
        command: ["rabbiz-settings"]
    }

    readonly property var wp: ShellPrefs.activeWallpaper

    // ---- Unlock fly-in: icons start spread along the bottom and glide into place ----
    property real enter: 1.0
    readonly property real enterSpan: 1.5
    function enterLin(delay) { return Math.max(0, Math.min(1, enter * enterSpan - delay)) }
    function enterEase(p) { var q = p - 1; return 1 + 2.1 * q * q * q + 1.1 * q * q }

    NumberAnimation {
        id: enterAnim
        target: home
        property: "enter"
        from: 0
        to: 1
        duration: 1000
        easing.type: Easing.Linear
    }

    Connections {
        target: ShellState
        function onLockScreenOpenChanged() {
            if (ShellState.lockScreenOpen) {
                enterAnim.stop()
                home.enter = 1.0
            } else {
                enterAnim.restart()
            }
        }
    }
    readonly property string timeFormatStr: ShellPrefs.use24h ? "HH:mm" : "h:mm ap"
    readonly property string hourStr: Qt.formatDateTime(clock.date, ShellPrefs.use24h ? "HH" : "h")
    readonly property string minStr: Qt.formatDateTime(clock.date, "mm")
    readonly property string ampmStr: ShellPrefs.use24h ? "" : Qt.formatDateTime(clock.date, "ap").toUpperCase()

    // Clock tick: the minute text (and the hour, when it rolls over) slides up
    // and fades back in instead of snapping to the new digits.
    readonly property int hourNumber: clock.date.getHours()
    readonly property int minuteNumber: clock.date.getMinutes()
    property real minuteTickShift: 0
    property real minuteTickOpacity: 1
    property real hourTickShift: 0
    property real hourTickOpacity: 1

    onMinuteNumberChanged: tickMinute()
    onHourNumberChanged: tickHour()

    function tickMinute() {
        minuteTickShift = 12
        minuteTickOpacity = 0
        minuteTickIn.restart()
        minuteTickFade.restart()
    }

    function tickHour() {
        hourTickShift = 12
        hourTickOpacity = 0
        hourTickIn.restart()
        hourTickFade.restart()
    }

    NumberAnimation {
        id: minuteTickIn
        target: home
        property: "minuteTickShift"
        to: 0
        duration: Theme.animMed
        easing.type: Easing.OutCubic
    }
    NumberAnimation {
        id: minuteTickFade
        target: home
        property: "minuteTickOpacity"
        to: 1
        duration: Theme.animMed
        easing.type: Easing.OutCubic
    }
    NumberAnimation {
        id: hourTickIn
        target: home
        property: "hourTickShift"
        to: 0
        duration: Theme.animMed
        easing.type: Easing.OutCubic
    }
    NumberAnimation {
        id: hourTickFade
        target: home
        property: "hourTickOpacity"
        to: 1
        duration: Theme.animMed
        easing.type: Easing.OutCubic
    }

    readonly property bool batteryOk: UPower.displayDevice !== null
        && UPower.displayDevice.ready
        && UPower.displayDevice.isLaptopBattery
    readonly property string powerLabel: batteryOk
        ? (Math.round(UPower.displayDevice.percentage * 100) + "%")
        : "AC"
    readonly property string netLabel: SystemControl.airplaneMode
        ? "Airplane"
        : (Network.wifiConnected ? Network.activeSsid : (Network.wifiEnabled ? "Wi-Fi" : "Offline"))

    // The media card only exists while something is really playing / a player is open; otherwise the
    // deck leads with the RabbizOS greeting card instead of an empty placeholder player.
    readonly property var heroDeckModel: Media.hasRealPlayer ? ["media", "system"] : ["welcome", "system"]
    readonly property string greeting: {
        var h = clock.date.getHours()
        return h < 5 ? "Good night" : h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening"
    }

    // Bumped on every unlock so the grid + dock tiles play their staggered
    // entrance (AppIcon.entranceKey). Never bump on boot or on app close.
    property int unlockEntranceKey: 0
    // Bumped while the lock is up so the tiles are hidden underneath it; the
    // home surface is composited under the lock and would otherwise flash its
    // settled grid for one frame when the lock unmaps.
    property int entranceArmKey: 0

    Connections {
        target: ShellState
        function onLockScreenOpenChanged() {
            if (ShellState.lockScreenOpen)
                home.entranceArmKey += 1
            else
                home.unlockEntranceKey += 1
        }
    }

    Component.onCompleted: {
        if (ShellState.lockScreenOpen)
            home.entranceArmKey += 1
    }

    function cycleClockStyle() {
        var order = ["editorial", "classic", "compact"]
        var idx = order.indexOf(ShellPrefs.clockStyle)
        ShellPrefs.clockStyle = order[(idx + 1) % order.length]
        ShellPrefs.save()
        clockMorphAnim.restart()
    }

    // ------------------------------------------------------------------
    // ~/Desktop + dock editing
    // ------------------------------------------------------------------
    // The model is Rabbiz.Desktop.DesktopLayout (ordering rules in IconLayout.js, host
    // tested; persisted to ~/.config/rabbiz-shell/desktop.json). Everything below is the
    // presentation layer: drag targets, the context menu, the editor and the picker.

    // Bumped when the installed-app list changes so the icon models rebuild.
    property int entryGeneration: 0

    readonly property var desktopIcons: {
        var gen = home.entryGeneration
        var ids = DesktopLayout.desktopIds
        var out = []
        for (var i = 0; i < ids.length; ++i)
            out.push(DesktopLayout.iconFor(ids[i]))
        return out
    }
    readonly property var dockIcons: {
        var gen = home.entryGeneration
        var ids = DesktopLayout.dockIds
        var out = []
        for (var i = 0; i < ids.length; ++i)
            out.push(DesktopLayout.iconFor(ids[i]))
        return out
    }

    Connections {
        target: DesktopEntries
        function onApplicationsChanged() { home.entryGeneration += 1 }
    }

    // Context menu / editor / picker / drag state.
    property var menuIcon: null          // AppIcon whose menu is open (null = closed)
    property var editorId: ""            // entry id being renamed
    property string editorAppName: ""
    property bool dragActive: false
    property var dragInfo: null          // { id, surface, index }
    property string dropSurface: ""
    property int dropIndex: -1
    property rect dropHole: Qt.rect(0, 0, 0, 0)
    property rect dockCaret: Qt.rect(0, 0, 0, 0)

    function surfaceLabelFor(surface) {
        return surface === "dock" ? DesktopLayout.dockLabel : DesktopLayout.surfaceLabel
    }

    function iconById(id) {
        for (var i = 0; i < desktopRepeater.count; ++i) {
            var a = desktopRepeater.itemAt(i)
            if (a && a.entryId === id)
                return a
        }
        for (var j = 0; j < dockRepeater.count; ++j) {
            var b = dockRepeater.itemAt(j)
            if (b && b.entryId === id)
                return b
        }
        return null
    }

    // Apply a model change after the current input event has unwound: it creates /
    // destroys Repeater delegates, and the delegate that owns the press handler may
    // still be on the stack.
    function commitLayout(fn) {
        Qt.callLater(fn)
    }

    function closeIconMenu() {
        home.menuIcon = null
        iconMenu.open = false
    }

    function closeOverlays() {
        closeIconMenu()
        editorSheet.open = false
        appPicker.open = false
    }

    function openIconMenu(icon, sx, sy, sw, sh) {
        if (!icon)
            return
        editorSheet.open = false
        appPicker.open = false
        home.menuIcon = icon
        var p = homeContent.mapFromItem(null, sx, sy)
        iconMenu.anchorRect = Qt.rect(p.x, p.y, sw, sh)
        iconMenu.target = {
            id: icon.entryId,
            name: icon.entryName,
            materialIcon: icon.resolvedMaterialIcon,
            iconUrl: icon.iconUrl,
            surfaceLabel: surfaceLabelFor(icon.surface),
            running: icon.isRunning
        }
        iconMenu.actions = menuActionsFor(icon)
        iconMenu.open = true
    }

    function menuActionsFor(icon) {
        var label = surfaceLabelFor(icon.surface)
        var actions = [{ id: "open", label: "Open", icon: "play_arrow" }]
        if (icon.isRunning)
            actions.push({ id: "close", label: "Close", icon: "stop_circle" })
        actions.push({ id: "edit", label: "Rename & icon", icon: "palette" })
        if (icon.surface === "dock")
            actions.push({ id: "toDesktop", label: "Move to " + DesktopLayout.surfaceLabel, icon: "desktop_windows" })
        else
            actions.push({ id: "toDock", label: "Move to " + DesktopLayout.dockLabel, icon: "dock",
                           enabled: !DesktopLayout.dockFull })
        actions.push({ id: "addApp", label: "Add app to " + label + "…", icon: "add" })
        actions.push({ id: "remove", label: "Remove from " + label, icon: "delete_outline", danger: true })
        return actions
    }

    function handleIconAction(action) {
        var icon = home.menuIcon
        if (!icon)
            return
        var surface = icon.surface
        var id = icon.entryId
        if (action === "open") {
            closeIconMenu()
            icon.triggerLaunch()
            return
        }
        if (action === "close") {
            var toplevel = icon.findRunningToplevel()
            closeIconMenu()
            if (toplevel)
                toplevel.close()
            return
        }
        if (action === "edit") {
            openIconEditor(icon)
            return
        }
        if (action === "addApp") {
            closeIconMenu()
            openAppPicker(surface)
            return
        }
        closeIconMenu()
        if (action === "toDock")
            commitLayout(function() { DesktopLayout.transfer(surface, id, "dock", DesktopLayout.count("dock")) })
        else if (action === "toDesktop")
            commitLayout(function() { DesktopLayout.transfer(surface, id, "desktop", DesktopLayout.count("desktop")) })
        else if (action === "remove")
            commitLayout(function() { DesktopLayout.remove(surface, id) })
    }

    function openIconEditor(icon) {
        closeIconMenu()
        home.editorId = icon.entryId
        home.editorAppName = icon.appName
        editorSheet.begin({
            id: icon.entryId,
            surface: icon.surface,
            surfaceLabel: surfaceLabelFor(icon.surface),
            name: icon.entryName,
            appName: icon.appName,
            defaultIcon: icon.entryMaterialIcon,
            overrideIcon: icon.overrideIcon,
            iconUrl: icon.iconUrl
        })
    }

    function applyIconEdit(label, iconName) {
        var id = home.editorId
        if (!id)
            return
        var text = String(label).trim()
        var patch = { label: text === home.editorAppName ? "" : text, icon: String(iconName) }
        commitLayout(function() { DesktopLayout.setOverride(id, patch) })
    }

    function resetIconEdit() {
        var id = home.editorId
        if (!id)
            return
        commitLayout(function() { DesktopLayout.clearOverride(id) })
    }

    function openAppPicker(surface) {
        closeIconMenu()
        appPicker.surface = surface
        appPicker.surfaceLabel = surfaceLabelFor(surface)
        appPicker.begin()
    }

    function addPickedApp(surface, entryId) {
        commitLayout(function() { DesktopLayout.add(surface, entryId) })
    }

    // ---- drag targets ------------------------------------------------------
    function scrollRectToContent(r) {
        var p = homeContent.mapFromItem(null, r.x, r.y)
        return Qt.rect(p.x, p.y, r.width, r.height)
    }

    function gridMetrics() {
        var probe = desktopRepeater.count > 0 ? desktopRepeater.itemAt(0) : null
        var cellW = probe ? probe.width : (Theme.iconApp - 4 + Theme.spaceM)
        var cellH = probe ? probe.height : (Theme.iconApp - 4 + Theme.fontCaption + Theme.spaceS + 4)
        return {
            x: homeGridRow.x,
            y: homeGridRow.y,
            colStep: cellW + homeGridRow.columnSpacing,
            rowStep: cellH + homeGridRow.rowSpacing,
            w: cellW,
            h: cellH,
            cols: homeGridRow.columns
        }
    }

    function gridCellRect(index) {
        var m = gridMetrics()
        return Qt.rect(m.x + (index % m.cols) * m.colStep,
                       m.y + Math.floor(index / m.cols) * m.rowStep,
                       m.w, m.h)
    }

    function gridIndexAt(sx, sy) {
        var m = gridMetrics()
        var p = homeContent.mapFromItem(null, sx, sy)
        var col = Math.floor((p.x - m.x + m.colStep / 2) / m.colStep)
        var row = Math.floor((p.y - m.y + m.rowStep / 2) / m.rowStep)
        col = Math.max(0, Math.min(m.cols - 1, col))
        row = Math.max(0, row)
        return row * m.cols + col
    }

    function dockTileRects() {
        var out = []
        for (var i = 0; i < dockRepeater.count; ++i) {
            var tile = dockRepeater.itemAt(i)
            if (!tile)
                continue
            var p = tile.mapToItem(homeContent, 0, 0)
            out.push({ index: i, x: p.x, y: p.y, w: tile.width, h: tile.height, cx: p.x + tile.width / 2 })
        }
        return out
    }

    // Dock icons that will stay put: the dragged one is not a drop candidate.
    function dockOthers() {
        var rects = dockTileRects()
        var out = []
        for (var i = 0; i < rects.length; ++i) {
            if (home.dragInfo && home.dragInfo.surface === "dock" && rects[i].index === home.dragInfo.index)
                continue
            out.push(rects[i])
        }
        return out
    }

    function dockIndexAt(sx, sy) {
        var p = homeContent.mapFromItem(null, sx, sy)
        var others = dockOthers()
        var n = 0
        for (var i = 0; i < others.length; ++i) {
            if (others[i].cx < p.x)
                n++
        }
        return n
    }

    function desktopTargetIndex(sx, sy) {
        var count = desktopRepeater.count
        var idx = gridIndexAt(sx, sy)
        var sameSurface = home.dragInfo && home.dragInfo.surface === "desktop"
        return Math.max(0, Math.min(sameSurface ? count - 1 : count, idx))
    }

    // Two forgiving drop bands: the dock (plus a margin above it, because a thumb
    // aiming at the dock usually lifts slightly high) and everything else on the
    // desktop, which lands in the nearest grid cell (or appends past the last one).
    function dropTargetAt(sx, sy) {
        var p = homeContent.mapFromItem(null, sx, sy)
        var dock = dockContainer.mapToItem(homeContent, 0, 0)
        if (p.x >= dock.x - 24 && p.x <= dock.x + dockContainer.width + 24
                && p.y >= dock.y - 28)
            return { surface: "dock", index: dockIndexAt(sx, sy) }
        return { surface: "desktop", index: desktopTargetIndex(sx, sy) }
    }

    function beginIconDrag(icon) {
        if (!icon || !icon.entryId)
            return
        closeOverlays()
        home.dragInfo = { id: icon.entryId, surface: icon.surface, index: icon.slotIndex }
        home.dragActive = true
        dragGhost.entry = icon.entry
        dragGhost.surface = icon.surface
        dragGhost.tileSize = icon.tileSize
        var r = scrollRectToContent(icon.tileSceneRect())
        dragGhost.x = r.x + r.width / 2 - dragGhost.width / 2
        dragGhost.y = r.y + r.height - dragGhost.height
        dragGhost.visible = true
        home.dropSurface = ""
        home.dropIndex = -1
        home.updateDropHints()
    }

    function updateIconDrag(icon, sx, sy) {
        if (!home.dragActive)
            return
        var p = homeContent.mapFromItem(null, sx, sy)
        dragGhost.x = Math.max(Theme.spaceXS, Math.min(home.width - dragGhost.width - Theme.spaceXS,
                                                       p.x - dragGhost.width / 2))
        dragGhost.y = Math.max(Theme.statusBarHeight + Theme.spaceXS, p.y - dragGhost.height - 18)
        var target = dropTargetAt(sx, sy)
        home.dropSurface = target.surface
        home.dropIndex = target.index
        home.updateDropHints()
    }

    function updateDropHints() {
        if (!home.dragActive || home.dropSurface === "") {
            home.dropHole = Qt.rect(0, 0, 0, 0)
            home.dockCaret = Qt.rect(0, 0, 0, 0)
            return
        }
        if (home.dropSurface === "desktop") {
            home.dropHole = gridCellRect(Math.max(0, Math.min(desktopRepeater.count, home.dropIndex)))
            home.dockCaret = Qt.rect(0, 0, 0, 0)
            return
        }
        var others = dockOthers()
        var dockRect = dockContainer.mapToItem(homeContent, 0, 0)
        var x
        if (others.length === 0) {
            x = dockRect.x + dockContainer.width / 2
        } else if (home.dropIndex <= 0) {
            x = others[0].x - dockRow.spacing / 2
        } else if (home.dropIndex >= others.length) {
            var last = others[others.length - 1]
            x = last.x + last.w + dockRow.spacing / 2
        } else {
            x = (others[home.dropIndex - 1].x + others[home.dropIndex - 1].w + others[home.dropIndex].x) / 2
        }
        home.dockCaret = Qt.rect(x - 2, dockRect.y + dockContainer.height / 2 - 18, 4, 36)
        home.dropHole = Qt.rect(0, 0, 0, 0)
    }

    function finishIconDrag(sx, sy) {
        if (!home.dragActive)
            return
        var info = home.dragInfo
        var target = dropTargetAt(sx, sy)
        home.dragActive = false
        home.dragInfo = null
        home.dropSurface = ""
        home.dropIndex = -1
        home.dropHole = Qt.rect(0, 0, 0, 0)
        home.dockCaret = Qt.rect(0, 0, 0, 0)
        dragGhost.visible = false
        if (!info || !target || target.surface === "")
            return
        var fromSurface = info.surface
        var id = info.id
        var toSurface = target.surface
        var toIndex = target.index
        commitLayout(function() {
            if (toSurface === fromSurface) {
                var from = DesktopLayout.indexOf(toSurface, id)
                if (from >= 0 && from !== toIndex)
                    DesktopLayout.move(toSurface, from, toIndex)
            } else {
                DesktopLayout.transfer(fromSurface, id, toSurface, toIndex)
            }
        })
    }

    function cancelIconDrag() {
        home.dragActive = false
        home.dragInfo = null
        home.dropSurface = ""
        home.dropIndex = -1
        home.dropHole = Qt.rect(0, 0, 0, 0)
        home.dockCaret = Qt.rect(0, 0, 0, 0)
        dragGhost.visible = false
    }

    // Automation/debug surface, mirroring the drawer's: `quickshell ... ipc call desktop state`
    IpcHandler {
        target: "desktop"

        function state(): string {
            return JSON.stringify({
                label: DesktopLayout.surfaceLabel,
                desktop: DesktopLayout.desktopIds,
                dock: DesktopLayout.dockIds,
                overrides: DesktopLayout.overrides,
                menuId: home.menuIcon ? home.menuIcon.entryId : "",
                editor: editorSheet.open,
                picker: appPicker.open,
                drag: home.dragActive,
                dropSurface: home.dropSurface,
                dropIndex: home.dropIndex
            })
        }
        function add(surface: string, id: string): string { DesktopLayout.add(surface, id); return "ok" }
        function remove(surface: string, id: string): string { DesktopLayout.remove(surface, id); return "ok" }
        function move(surface: string, from: int, to: int): string { DesktopLayout.move(surface, from, to); return "ok" }
        function reset(): string { DesktopLayout.reset(); return "ok" }
        function menu(id: string): string {
            var icon = home.iconById(id)
            if (!icon)
                return "no icon " + id
            var r = icon.tileSceneRect()
            home.openIconMenu(icon, r.x, r.y, r.width, r.height)
            return "menu " + id
        }
        function action(id: string): string { home.handleIconAction(id); return "ok" }
        function edit(): string {
            if (!home.menuIcon)
                return "no icon"
            home.openIconEditor(home.menuIcon)
            return "edit"
        }
        function applyEdit(label: string, iconName: string): string { home.applyIconEdit(label, iconName); return "ok" }
        function picker(surface: string): string { home.openAppPicker(surface); return "picker" }
        function pick(id: string): string {
            var surface = appPicker.surface
            appPicker.open = false
            home.addPickedApp(surface, id)
            return "ok"
        }
        function close(): string { home.closeOverlays(); home.cancelIconDrag(); return "closed" }
        // Drives the same drawer-swipe maths a finger on the wallpaper (or on an icon)
        // goes through, so the gesture can be exercised without a touch device.
        function swipe(dy: real): string {
            homeContent.beginDrawerDrag(600)
            homeContent.updateDrawerDrag(600 - dy)
            homeContent.endDrawerDrag()
            return ShellState.drawerOpen ? "open" : "closed"
        }
        function dragStart(id: string): string {
            var icon = home.iconById(id)
            if (!icon)
                return "no icon " + id
            icon.beginPickup()
            home.beginIconDrag(icon)
            return "dragging " + id
        }
        function dragTo(x: real, y: real): string {
            var icon = home.dragInfo ? home.iconById(home.dragInfo.id) : null
            if (!icon)
                return "no drag"
            home.updateIconDrag(icon, x, y)
            return "at " + home.dropSurface + ":" + home.dropIndex
        }
        function dragDrop(x: real, y: real): string {
            var icon = home.dragInfo ? home.iconById(home.dragInfo.id) : null
            if (icon)
                icon.endPickup()
            home.finishIconDrag(x, y)
            return "dropped"
        }
    }

    // The drawer samples this sibling scene, which excludes the drawer itself.
    Item {
        id: desktopScene
        anchors.fill: parent

    // ---- Fixed-frame Wallpaper layer with subtle parallax zoom when Drawer opens ----
    Item {
        id: wallpaperScene
        anchors.fill: parent
        clip: true

        Item {
            anchors.fill: parent
            // No scale here: a full-screen image resampled every frame is the most
            // expensive thing the software renderer can do during the drawer slide
            // (owner: "drawer stutters, not 60 fps"). The drawer dim below still gives
            // the depth cue without touching the bitmap.

            Rectangle {
                anchors.fill: parent
                gradient: Gradient {
                    GradientStop { position: 0.0; color: home.wp.top }
                    GradientStop { position: 0.48; color: home.wp.mid }
                    GradientStop { position: 1.0; color: home.wp.bottom }
                }
            }

            Image {
                id: wallpaperImg
                anchors.fill: parent
                source: home.wp.image || ""
                fillMode: Image.PreserveAspectCrop
                smooth: true
                asynchronous: true
                visible: status === Image.Ready
            }

            Item {
                anchors.fill: parent
                opacity: wallpaperImg.status === Image.Ready ? 0.09 : 0.12

                Rectangle {
                    width: home.width * 1.15
                    height: width
                    radius: width / 2
                    x: home.width * 0.25
                    y: -home.height * 0.08
                    color: Qt.alpha(home.wp.orb1, 0.45)

                    Rectangle {
                        anchors.centerIn: parent
                        width: parent.width * 0.68
                        height: width
                        radius: width / 2
                        color: Qt.alpha(home.wp.orb1, 0.55)
                    }
                }

                Rectangle {
                    width: home.width * 1.05
                    height: width
                    radius: width / 2
                    x: -home.width * 0.35
                    y: home.height * 0.32
                    color: Qt.alpha(home.wp.orb2, 0.40)

                    Rectangle {
                        anchors.centerIn: parent
                        width: parent.width * 0.65
                        height: width
                        radius: width / 2
                        color: Qt.alpha(home.wp.orb2, 0.50)
                    }
                }
            }
        }

        // Top & bottom contrast vignettes
        Rectangle {
            anchors { left: parent.left; right: parent.right; top: parent.top }
            height: home.height * 0.22
            gradient: Gradient {
                GradientStop { position: 0.0; color: Qt.alpha("#000000", 0.28) }
                GradientStop { position: 1.0; color: "transparent" }
            }
        }

        Rectangle {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: home.height * 0.28
            gradient: Gradient {
                GradientStop { position: 0.0; color: "transparent" }
                GradientStop { position: 1.0; color: Qt.alpha("#000000", 0.52) }
            }
        }
    }

    // ---- Foreground Home Content (scales & fades smoothly into depth when drawer opens) ----
    Item {
        id: homeContent
        anchors.fill: parent
        // Fade only. Scaling the whole home subtree (icons, cards, text) every frame
        // forces the software renderer to resample everything, which is what made the
        // drawer slide stutter; the dim alone still reads as "pushed back".
        opacity: 1.0 - 0.55 * ShellState.drawerProgress

        // ---- Gesture layer: interactive vertical drag to open the App Drawer ----
        // The maths lives in these functions because a swipe that starts on an icon is
        // forwarded here too (AppIcon.drawerGesture) instead of being swallowed by it.
        property real drawerPressY: 0
        property bool drawerDragging: false
        property bool drawerFromIcon: false

        function beginDrawerDrag(sceneY) {
            drawerPressY = sceneY
            drawerDragging = false
        }
        function updateDrawerDrag(sceneY) {
            if (ShellState.drawerOpen)
                return
            var dy = drawerPressY - sceneY
            if (dy > 14)
                drawerDragging = true
            if (drawerDragging) {
                var range = Math.max(220, home.height * 0.45)
                ShellState.drawerProgress = Math.max(0, Math.min(1, dy / range))
            }
        }
        function endDrawerDrag() {
            if (!drawerDragging)
                return
            drawerDragging = false
            if (ShellState.drawerProgress > 0.24) {
                ShellState.drawerOpen = true
                appDrawer.snapTo(1.0)
            } else {
                ShellState.drawerOpen = false
                appDrawer.snapTo(0.0)
            }
        }

        // An icon only starts reporting after the finger has travelled far enough to be
        // a swipe, so use its first sample as the press point (minus the threshold).
        function iconDrawerGesture(sceneY) {
            var y = homeContent.mapFromItem(null, 0, sceneY).y
            if (!drawerFromIcon) {
                drawerFromIcon = true
                beginDrawerDrag(y + 24)
            }
            updateDrawerDrag(y)
        }
        function iconDrawerGestureEnded() {
            drawerFromIcon = false
            endDrawerDrag()
        }

        MouseArea {
            id: homeSwipe
            anchors.fill: parent
            onPressed: (mouse) => homeContent.beginDrawerDrag(mouse.y)
            onPositionChanged: (mouse) => { if (pressed) homeContent.updateDrawerDrag(mouse.y) }
            onReleased: homeContent.endDrawerDrag()
            onCanceled: homeContent.endDrawerDrag()
        }

        // ---- Clock (tap to morph style) + Date + Telemetry At-a-Glance Pill ----
        Item {
            id: clockBlock
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
                topMargin: Theme.statusBarHeight + parent.height * 0.032
                leftMargin: Theme.spaceL
                rightMargin: Theme.spaceL
            }
            height: clockCol.implicitHeight
            readonly property real fp: home.enterLin(0.0)
            opacity: Math.min(1, fp * 2.5)
            transform: Translate { y: -36 * (1 - home.enterEase(clockBlock.fp)) }

            ColumnLayout {
                id: clockCol
                anchors.fill: parent
                spacing: Theme.spaceS

                // Clickable Clock wrapper with style-cycle morph animation
                Item {
                    id: clockMorphWrap
                    Layout.fillWidth: true
                    Layout.preferredHeight: clockStack.implicitHeight

                    SequentialAnimation {
                        id: clockMorphAnim
                        ParallelAnimation {
                            NumberAnimation { target: clockMorphWrap; property: "scale"; from: 0.92; to: 1.0; duration: Theme.animMed; easing.type: Easing.OutBack }
                            NumberAnimation { target: clockMorphWrap; property: "opacity"; from: 0.45; to: 1.0; duration: Theme.animMed; easing.type: Theme.animEase }
                        }
                    }

                    Item {
                        id: clockStack
                        anchors.fill: parent
                        implicitHeight: ShellPrefs.clockStyle === "compact"
                            ? compactClock.implicitHeight
                            : (ShellPrefs.clockStyle === "classic" ? classicClock.implicitHeight : editorialClock.implicitHeight)

                        // Style 1: Editorial
                        RowLayout {
                            id: editorialClock
                            visible: ShellPrefs.clockStyle === "editorial"
                            anchors.horizontalCenter: parent.horizontalCenter
                            spacing: 4

                            Text {
                                id: editorialHour
                                text: home.hourStr
                                opacity: home.hourTickOpacity
                                transform: Translate { y: home.hourTickShift }
                                color: Theme.onWallpaper
                                font.pixelSize: 120
                                font.family: Theme.fontFamily
                                font.weight: Font.Light
                            }
                            Text {
                                text: ":"
                                color: Qt.alpha(ShellPrefs.accentColor, 0.75)
                                font.pixelSize: 108
                                font.family: Theme.fontFamily
                                font.weight: Font.ExtraLight
                            }
                            Text {
                                id: editorialMinute
                                text: home.minStr
                                opacity: home.minuteTickOpacity
                                transform: Translate { y: home.minuteTickShift }
                                color: ShellPrefs.accentColor
                                font.pixelSize: 120
                                font.family: Theme.fontFamily
                                font.weight: Font.DemiBold
                            }
                            Text {
                                visible: home.ampmStr.length > 0
                                text: home.ampmStr
                                color: Theme.onWallpaperDim
                                font.pixelSize: Theme.fontCaption
                                font.family: Theme.fontFamily
                                font.weight: Font.DemiBold
                                Layout.alignment: Qt.AlignBottom
                                Layout.bottomMargin: 22
                            }
                        }

                        // Style 2: Classic
                        Text {
                            id: classicClock
                            visible: ShellPrefs.clockStyle === "classic"
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: Qt.formatDateTime(clock.date, home.timeFormatStr)
                            opacity: home.minuteTickOpacity
                            transform: Translate { y: home.minuteTickShift }
                            color: Theme.onWallpaper
                            font.pixelSize: 114
                            font.family: Theme.fontFamily
                            font.weight: Font.Light
                        }

                        // Style 3: Compact
                        Text {
                            id: compactClock
                            visible: ShellPrefs.clockStyle === "compact"
                            width: parent.width
                            text: Qt.formatDateTime(clock.date, home.timeFormatStr)
                            opacity: home.minuteTickOpacity
                            transform: Translate { y: home.minuteTickShift }
                            color: Theme.onWallpaper
                            font.pixelSize: 82
                            font.family: Theme.fontFamily
                            font.weight: Font.DemiBold
                        }
                    }

                    TapHandler {
                        onTapped: home.cycleClockStyle()
                    }
                }

                Text {
                    Layout.alignment: ShellPrefs.clockStyle === "compact"
                        ? Qt.AlignLeft : Qt.AlignHCenter
                    text: Qt.formatDateTime(clock.date, "dddd, MMMM d")
                    color: Theme.onWallpaperDim
                    font.pixelSize: Theme.fontBody
                    font.family: Theme.fontFamily
                    font.weight: Font.Medium
                }

                // Telemetry + Status At-a-Glance pill (tap -> Settings)
                Rectangle {
                    Layout.alignment: ShellPrefs.clockStyle === "compact"
                        ? Qt.AlignLeft : Qt.AlignHCenter
                    Layout.topMargin: Theme.spaceXS
                    implicitWidth: Math.min(parent.width, glanceRow.implicitWidth + Theme.spaceM * 2)
                    implicitHeight: glanceRow.implicitHeight + Theme.spaceS + 4
                    radius: Theme.radiusPill
                    color: Qt.alpha(Theme.surfaceHigh, 0.70)
                    border.width: 1
                    border.color: Qt.alpha(Theme.text, 0.11)
                    scale: glanceTap.pressed ? 0.96 : 1.0
                    Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

                    RowLayout {
                        id: glanceRow
                        anchors.centerIn: parent
                        spacing: Theme.spaceS

                        MaterialIcon {
                            name: SystemControl.airplaneMode ? "flight" : (Network.wifiConnected ? "wifi" : "wifi_off")
                            size: Theme.fontBody - 2
                            color: SystemControl.airplaneMode ? Theme.warn : (Network.wifiConnected ? ShellPrefs.accentColor : Theme.textFaint)
                        }

                        Text {
                            text: home.netLabel
                            color: Theme.text
                            font.pixelSize: Theme.fontStatus
                            font.family: Theme.fontFamily
                            font.weight: Font.Medium
                        }

                        Rectangle { width: 1; height: 16; color: Qt.alpha(Theme.text, 0.18) }

                        MaterialIcon {
                            name: "memory"
                            size: Theme.fontBody - 2
                            color: ShellPrefs.accentColor
                        }

                        Text {
                            text: SystemControl.cpuPercent + "% CPU"
                            color: Theme.text
                            font.pixelSize: Theme.fontStatus
                            font.family: Theme.fontFamily
                            font.weight: Font.Medium
                        }

                        Rectangle { width: 1; height: 16; color: Qt.alpha(Theme.text, 0.18) }

                        MaterialIcon {
                            name: SystemControl.dndEnabled ? "do_not_disturb_on" : "bolt"
                            size: Theme.fontBody - 2
                            color: SystemControl.dndEnabled ? Theme.warn : Theme.textDim
                        }

                        Text {
                            text: home.powerLabel
                            color: Theme.text
                            font.pixelSize: Theme.fontStatus
                            font.family: Theme.fontFamily
                            font.weight: Font.Medium
                        }
                    }

                    TapHandler {
                        id: glanceTap
                        onTapped: {
                            if (!settingsProc.running)
                                settingsProc.running = true
                        }
                    }
                }
            }
        }

        // ---- Location chip: this home page is ~/Desktop ----
        Rectangle {
            id: locationChip
            anchors {
                left: parent.left
                top: parent.top
                leftMargin: Theme.spaceM
                topMargin: Theme.statusBarHeight + Theme.spaceXS
            }
            width: chipRow.implicitWidth + Theme.spaceM
            height: 30
            radius: Theme.radiusPill
            color: Qt.alpha(Theme.surfaceHigh, 0.55)
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.10)
            opacity: 0.92

            Row {
                id: chipRow
                anchors.centerIn: parent
                spacing: Theme.spaceXS

                MaterialIcon {
                    name: "folder_open"
                    size: Theme.fontStatus - 4
                    color: Theme.onWallpaperDim
                }
                Text {
                    text: DesktopLayout.surfaceLabel
                    color: Theme.onWallpaperDim
                    font.pixelSize: Theme.fontStatus - 3
                    font.family: Theme.fontFamily
                    font.weight: Font.Medium
                }
            }
        }

        // ---- ~/Desktop icon grid (user-editable: hold to drag, long-press for menu) ----
        Grid {
            id: homeGridRow
            readonly property real cellW: Theme.iconApp - 4 + Theme.spaceM
            readonly property real sideMargin: Theme.spaceL
            anchors.horizontalCenter: parent.horizontalCenter
            // Upper third of the free space between the clock and the hero deck: reads as one group
            // with the clock and keeps the labels clear of the wallpaper's bright sun below.
            y: clockBlock.y + clockBlock.height
               + Math.max(Theme.spaceM, (heroDeckSection.y - (clockBlock.y + clockBlock.height) - height) / 3)
            columns: 4
            columnSpacing: Math.max(Theme.spaceS,
                (homeContent.width - 2 * sideMargin - columns * cellW) / (columns - 1))
            rowSpacing: Theme.spaceL

            Repeater {
                id: desktopRepeater
                model: home.desktopIcons
                delegate: AppIcon {
                    id: gridIcon
                    required property var modelData
                    required property int index
                    tileSize: Theme.iconApp - 4
                    width: homeGridRow.cellW
                    height: tileSize + Theme.fontCaption + Theme.spaceS + 4
                    entry: modelData
                    surface: "desktop"
                    slotIndex: index
                    labelVisible: true
                    menuOpen: home.menuIcon === gridIcon
                    entranceKey: home.unlockEntranceKey
                    entranceArmKey: home.entranceArmKey

                    // Unlock fly-in, kept alongside AppIcon's own entrance.
                    readonly property real fp: home.enterLin(0.05 + index * 0.035)
                    readonly property real fe: home.enterEase(fp)
                    opacity: Math.min(1, fp * 2.5)
                    transform: [
                        Translate {
                            x: (homeGridRow.x + gridIcon.x + gridIcon.width / 2 - home.width / 2) * 0.75 * (1 - gridIcon.fe)
                            y: (home.height * 0.38 + Math.floor(gridIcon.index / 4) * 36) * (1 - gridIcon.fe)
                        },
                        Scale {
                            origin.x: gridIcon.width / 2
                            origin.y: gridIcon.height / 2
                            xScale: 0.6 + 0.4 * gridIcon.fe
                            yScale: 0.6 + 0.4 * gridIcon.fe
                        }
                    ]

                    onMenuRequested: (sx, sy, sw, sh) => home.openIconMenu(gridIcon, sx, sy, sw, sh)
                    onDragStarted: home.beginIconDrag(gridIcon)
                    onDragMoved: (sx, sy) => home.updateIconDrag(gridIcon, sx, sy)
                    onDragDropped: (sx, sy) => home.finishIconDrag(sx, sy)
                    onDragCancelled: home.cancelIconDrag()
                    onDrawerGesture: (sceneY) => homeContent.iconDrawerGesture(sceneY)
                    onDrawerGestureEnded: homeContent.iconDrawerGestureEnded()
                }
            }

            AddTile {
                tileSize: Theme.iconApp - 4
                labelText: "Add app"
                onTapped: home.openAppPicker("desktop")
            }
        }

        // ---- Interactive 3-Card Hero Pager Deck (Media / System) ----
        Item {
            id: heroDeckSection
            anchors {
                left: parent.left
                right: parent.right
                bottom: searchPill.top
                bottomMargin: Theme.spaceM
            }
            height: 192
            readonly property real fp: home.enterLin(0.32)
            opacity: Math.min(1, fp * 2.5)
            transform: Translate { y: 140 * (1 - home.enterEase(heroDeckSection.fp)) }

            ListView {
                id: heroList
                anchors {
                    top: parent.top
                    left: parent.left
                    right: parent.right
                }
                height: 168
                orientation: ListView.Horizontal
                snapMode: ListView.SnapOneItem
                highlightRangeMode: ListView.StrictlyEnforceRange
                highlightMoveDuration: Theme.animMed
                boundsBehavior: Flickable.StopAtBounds
                clip: true
                model: home.heroDeckModel
                // The deck's first card changes with the media state; always land on the first card.
                onModelChanged: { currentIndex = 0; positionViewAtBeginning() }

                delegate: Item {
                    id: deckDelegate
                    required property string modelData
                    width: heroList.width
                    height: heroList.height

                    // --- Card 0: RabbizOS welcome (shown while no media player is active) ---
                    Rectangle {
                        visible: deckDelegate.modelData === "welcome"
                        anchors {
                            fill: parent
                            leftMargin: Theme.spaceM
                            rightMargin: Theme.spaceM
                        }
                        radius: Theme.radiusL
                        color: Qt.alpha(Theme.card, 0.90)
                        border.width: 1
                        border.color: Qt.alpha(Theme.text, 0.10)
                        clip: true

                        RowLayout {
                            anchors { fill: parent; margins: Theme.spaceM }
                            spacing: Theme.spaceM

                            Image {
                                source: "../Assets/rabbiz-mark.png"
                                Layout.preferredWidth: 96
                                Layout.preferredHeight: 96
                                sourceSize: Qt.size(192, 192)
                                fillMode: Image.PreserveAspectFit
                                smooth: true
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 4
                                Text {
                                    text: "RabbizOS"
                                    color: ShellPrefs.accentColor
                                    font.pixelSize: Theme.fontCaption
                                    font.family: Theme.fontFamily
                                    font.letterSpacing: 2
                                    font.weight: Font.DemiBold
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: home.greeting
                                    color: Theme.text
                                    font.pixelSize: Theme.fontTitle
                                    font.family: Theme.fontFamily
                                    font.weight: Font.DemiBold
                                    elide: Text.ElideRight
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: "Swipe up for all apps"
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontCaption
                                    font.family: Theme.fontFamily
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }

                    // --- Card 1: Now Playing Media Card (MPRIS + Morphing Play/Pause) ---
                    Rectangle {
                        visible: deckDelegate.modelData === "media"
                        anchors {
                            fill: parent
                            leftMargin: Theme.spaceM
                            rightMargin: Theme.spaceM
                        }
                        radius: Theme.radiusL
                        color: Qt.alpha(Theme.card, 0.90)
                        border.width: 1
                        border.color: Media.isPlaying
                            ? Qt.alpha(ShellPrefs.accentColor, 0.45)
                            : Qt.alpha(Theme.text, 0.10)
                        Behavior on border.color { ColorAnimation { duration: Theme.animMed } }
                        clip: true

                        ColumnLayout {
                            anchors { fill: parent; margins: Theme.spaceM }
                            spacing: Theme.spaceS

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceM

                                // Album art / animated equalizer badge (tap -> Elisa)
                                Rectangle {
                                    width: 62; height: 62
                                    radius: Theme.radiusM
                                    color: Qt.alpha(ShellPrefs.accentColor, 0.18)
                                    border.width: 1
                                    border.color: Qt.alpha(ShellPrefs.accentColor, 0.35)
                                    clip: true

                                    Image {
                                        id: mediaArt
                                        anchors.fill: parent
                                        source: Media.artUrl
                                        fillMode: Image.PreserveAspectCrop
                                        asynchronous: true
                                        visible: status === Image.Ready
                                    }

                                    MaterialIcon {
                                        anchors.centerIn: parent
                                        visible: mediaArt.status !== Image.Ready && !Media.isPlaying
                                        name: "music_note"
                                        size: 30
                                        color: ShellPrefs.accentColor
                                    }

                                    Row {
                                        anchors.centerIn: parent
                                        visible: mediaArt.status !== Image.Ready && Media.isPlaying
                                        spacing: 4

                                        Repeater {
                                            model: [
                                                { hi: 30, lo: 10, dur: 380 },
                                                { hi: 38, lo: 14, dur: 470 },
                                                { hi: 26, lo: 8,  dur: 340 },
                                                { hi: 34, lo: 12, dur: 520 }
                                            ]
                                            delegate: Item {
                                                required property var modelData
                                                width: 5
                                                height: 38

                                                Rectangle {
                                                    anchors {
                                                        left: parent.left
                                                        right: parent.right
                                                        bottom: parent.bottom
                                                    }
                                                    height: modelData.lo
                                                    radius: 2
                                                    color: ShellPrefs.accentColor

                                                    SequentialAnimation on height {
                                                        running: Media.isPlaying
                                                        loops: Animation.Infinite
                                                        NumberAnimation { to: modelData.hi; duration: modelData.dur; easing.type: Easing.InOutSine }
                                                        NumberAnimation { to: modelData.lo; duration: modelData.dur; easing.type: Easing.InOutSine }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    TapHandler {
                                        onTapped: Media.openMusicApp()
                                    }
                                }

                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 2

                                    Text {
                                        text: Media.hasRealPlayer ? "NOW PLAYING" : "MEDIA DECK"
                                        color: ShellPrefs.accentColor
                                        font.pixelSize: Theme.fontStatus - 2
                                        font.family: Theme.fontFamily
                                        font.weight: Font.DemiBold
                                        font.letterSpacing: 1.4
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        text: Media.title
                                        color: Theme.text
                                        font.pixelSize: Theme.fontBody
                                        font.family: Theme.fontFamily
                                        font.weight: Font.DemiBold
                                        elide: Text.ElideRight
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        text: Media.artist
                                        color: Theme.textDim
                                        font.pixelSize: Theme.fontStatus
                                        font.family: Theme.fontFamily
                                        elide: Text.ElideRight
                                    }

                                    TapHandler {
                                        onTapped: Media.openMusicApp()
                                    }
                                }

                                // Transport controls with shape-morphing Play/Pause button
                                Row {
                                    spacing: 8

                                    Rectangle {
                                        width: 44; height: 44; radius: 22
                                        color: Theme.surfaceHigh
                                        border.width: 1; border.color: Qt.alpha(Theme.text, 0.08)
                                        scale: prevTap.pressed ? 0.88 : 1.0
                                        Behavior on scale { NumberAnimation { duration: Theme.animFast } }
                                        MaterialIcon {
                                            anchors.centerIn: parent
                                            name: "skip_previous"
                                            size: 24; color: Theme.text
                                        }
                                        TapHandler {
                                            id: prevTap
                                            onTapped: Media.previous()
                                        }
                                    }

                                    Rectangle {
                                        width: 52; height: 52
                                        // Morph circle <-> rounded squircle on playback state
                                        radius: Media.isPlaying ? 15 : 26
                                        color: Media.isPlaying ? ShellPrefs.accentColor : Theme.surfaceHigh
                                        border.width: Media.isPlaying ? 0 : 1
                                        border.color: Qt.alpha(ShellPrefs.accentColor, 0.6)
                                        scale: ppTap.pressed ? 0.90 : 1.0

                                        Behavior on radius { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }
                                        Behavior on color { ColorAnimation { duration: Theme.animFast } }
                                        Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                                        MaterialIcon {
                                            anchors.centerIn: parent
                                            name: Media.isPlaying ? "pause" : "play_arrow"
                                            size: 28
                                            color: Media.isPlaying ? ShellPrefs.onAccentColor : Theme.text
                                        }
                                        TapHandler {
                                            id: ppTap
                                            onTapped: Media.togglePlaying()
                                        }
                                    }

                                    Rectangle {
                                        width: 44; height: 44; radius: 22
                                        color: Theme.surfaceHigh
                                        border.width: 1; border.color: Qt.alpha(Theme.text, 0.08)
                                        scale: nextTap.pressed ? 0.88 : 1.0
                                        Behavior on scale { NumberAnimation { duration: Theme.animFast } }
                                        MaterialIcon {
                                            anchors.centerIn: parent
                                            name: "skip_next"
                                            size: 24; color: Theme.text
                                        }
                                        TapHandler {
                                            id: nextTap
                                            onTapped: Media.next()
                                        }
                                    }
                                }
                            }

                            // Playback progress track
                            Rectangle {
                                Layout.fillWidth: true
                                height: 6
                                radius: 3
                                color: Theme.surfaceHigh

                                Rectangle {
                                    anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
                                    width: Math.max(6, parent.width * Media.progress)
                                    radius: 3
                                    color: ShellPrefs.accentColor
                                    Behavior on width { NumberAnimation { duration: Theme.animFast } }
                                }
                            }
                        }
                    }

                    // --- Card 2: System Telemetry & Boost ---
                    Rectangle {
                        visible: deckDelegate.modelData === "system"
                        anchors {
                            fill: parent
                            leftMargin: Theme.spaceM
                            rightMargin: Theme.spaceM
                        }
                        radius: Theme.radiusL
                        color: Qt.alpha(Theme.card, 0.90)
                        border.width: 1
                        border.color: Qt.alpha(Theme.text, 0.10)
                        clip: true

                        ColumnLayout {
                            anchors { fill: parent; margins: Theme.spaceM }
                            spacing: Theme.spaceS

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceS

                                MaterialIcon {
                                    name: "speed"
                                    size: 22
                                    color: ShellPrefs.accentColor
                                }
                                Text {
                                    text: "SYSTEM TELEMETRY"
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontStatus - 1
                                    font.family: Theme.fontFamily
                                    font.weight: Font.DemiBold
                                    font.letterSpacing: 1.3
                                }
                                Item { Layout.fillWidth: true }
                                Text {
                                    text: SystemControl.cpuFreqMhz > 0 ? (SystemControl.cpuFreqMhz + " MHz") : ""
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontStatus - 2
                                    font.family: Theme.fontFamily
                                }
                                Rectangle {
                                    readonly property bool isPerf: SystemControl.cpuGovernor === "performance"
                                    implicitWidth: govLabel.implicitWidth + 20
                                    implicitHeight: 30
                                    radius: isPerf ? 15 : 8
                                    color: isPerf ? ShellPrefs.accentColor : Theme.surfaceHigh
                                    border.width: isPerf ? 0 : 1
                                    border.color: Qt.alpha(Theme.text, 0.14)
                                    scale: govTap.pressed ? 0.92 : 1.0

                                    Behavior on radius { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }
                                    Behavior on color { ColorAnimation { duration: Theme.animFast } }
                                    Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                                    Text {
                                        id: govLabel
                                        anchors.centerIn: parent
                                        text: SystemControl.cpuGovernor.toUpperCase()
                                        color: parent.isPerf ? ShellPrefs.onAccentColor : Theme.text
                                        font.pixelSize: Theme.fontStatus - 3
                                        font.family: Theme.fontFamily
                                        font.weight: Font.Bold
                                    }
                                    TapHandler {
                                        id: govTap
                                        onTapped: SystemControl.toggleGovernor()
                                    }
                                }
                            }

                            // CPU bar
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceS
                                Text {
                                    Layout.preferredWidth: 52
                                    text: "CPU"
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontStatus - 1
                                    font.family: Theme.fontFamily
                                }
                                Rectangle {
                                    Layout.fillWidth: true
                                    height: 8; radius: 4
                                    color: Theme.surfaceHigh
                                    Rectangle {
                                        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
                                        width: Math.max(8, parent.width * (SystemControl.cpuPercent / 100.0))
                                        radius: 4
                                        color: SystemControl.cpuPercent > 80 ? Theme.warn : ShellPrefs.accentColor
                                        Behavior on width { NumberAnimation { duration: Theme.animMed; easing.type: Theme.animEase } }
                                    }
                                }
                                Text {
                                    Layout.preferredWidth: 52
                                    horizontalAlignment: Text.AlignRight
                                    text: SystemControl.cpuPercent + "%"
                                    color: Theme.text
                                    font.pixelSize: Theme.fontStatus - 1
                                    font.family: Theme.fontFamily
                                    font.weight: Font.DemiBold
                                }
                            }

                            // RAM & Disk row
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceS
                                Text {
                                    Layout.preferredWidth: 52
                                    text: "RAM"
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontStatus - 1
                                    font.family: Theme.fontFamily
                                }
                                Rectangle {
                                    Layout.fillWidth: true
                                    height: 8; radius: 4
                                    color: Theme.surfaceHigh
                                    Rectangle {
                                        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
                                        width: Math.max(8, parent.width * (SystemControl.memPercent / 100.0))
                                        radius: 4
                                        color: Theme.success
                                        Behavior on width { NumberAnimation { duration: Theme.animMed; easing.type: Theme.animEase } }
                                    }
                                }
                                Text {
                                    horizontalAlignment: Text.AlignRight
                                    text: SystemControl.memPercent + "% · Free " + SystemControl.diskFreeText
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontStatus - 2
                                    font.family: Theme.fontFamily
                                }
                            }
                        }
                    }
                }
            }

            // Morphing pill pagination dots
            Row {
                anchors {
                    horizontalCenter: parent.horizontalCenter
                    bottom: parent.bottom
                }
                spacing: 8

                Repeater {
                    model: heroList.count
                    delegate: Rectangle {
                        required property int index
                        readonly property bool active: heroList.currentIndex === index
                        width: active ? 26 : 8
                        height: 8
                        radius: 4
                        color: active ? ShellPrefs.accentColor : Qt.alpha(Theme.text, 0.28)

                        Behavior on width { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }
                        Behavior on color { ColorAnimation { duration: Theme.animFast } }

                        TapHandler {
                            onTapped: heroList.currentIndex = index
                        }
                    }
                }
            }
        }

        // ---- Search Pill (morphs upward into AppDrawer on swipe-up) ----
        Rectangle {
            id: searchPill
            anchors {
                left: parent.left
                right: parent.right
                bottom: dockContainer.top
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
                bottomMargin: Theme.spaceM
            }
            height: Theme.iconM + Theme.spaceS + 4
            radius: Theme.radiusPill
            color: Qt.alpha(Theme.surfaceHigh, 0.72)
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.11)
            readonly property real fp: home.enterLin(0.38)
            opacity: Math.max(0, 1.0 - ShellState.drawerProgress * 1.45) * Math.min(1, fp * 2.5)
            scale: (searchTap.pressed ? 0.97 : 1.0) * (1.0 + 0.03 * ShellState.drawerProgress)
            transform: [
                Translate { y: -ShellState.drawerProgress * (home.height * 0.42) },
                Translate { y: 110 * (1 - home.enterEase(searchPill.fp)) }
            ]
            Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

            RowLayout {
                anchors {
                    fill: parent
                    leftMargin: Theme.spaceM
                    rightMargin: Theme.spaceM
                }
                spacing: Theme.spaceS

                MaterialIcon {
                    name: "search"
                    size: Theme.fontTitle
                    color: ShellPrefs.accentColor
                }

                Text {
                    Layout.fillWidth: true
                    text: "Search apps, files & settings…"
                    color: Theme.textDim
                    font.pixelSize: Theme.fontCaption
                    font.family: Theme.fontFamily
                }

                MaterialIcon {
                    name: "apps"
                    size: Theme.fontTitle - 2
                    color: Theme.textDim
                }
            }

            TapHandler {
                id: searchTap
                onTapped: ShellState.drawerOpen = true
            }
        }

        // ---- Frosted Glass Dock ----
        BackdropBlur {
            id: dockContainer
            sourceItem: wallpaperScene
            sourceRect: Qt.rect(x, y, width, height)
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
                bottomMargin: Theme.navZoneHeight + Theme.spaceS + 8
            }
            height: Theme.iconApp + Theme.spaceM
            radius: Theme.radiusL + 10
            color: gpu ? Theme.glassBlur : Theme.glass
            border.width: 1
            border.color: Theme.glassBorder
            opacity: Math.min(1, home.enterLin(0.30) * 2.5)

            Row {
                id: dockRow
                anchors.centerIn: parent
                // Five favourites plus the add tile still fit in the 680 px dock.
                spacing: home.dockIcons.length >= 4 ? Theme.spaceS : Theme.spaceL

                Repeater {
                    id: dockRepeater
                    model: home.dockIcons
                    delegate: AppIcon {
                        id: dockIcon
                        required property var modelData
                        required property int index
                        tileSize: Theme.iconApp - 8
                        width: tileSize + Theme.spaceS
                        height: tileSize + 8
                        entry: modelData
                        surface: "dock"
                        slotIndex: index
                        labelVisible: false
                        menuOpen: home.menuIcon === dockIcon
                        entranceKey: home.unlockEntranceKey
                        entranceArmKey: home.entranceArmKey
                        onMenuRequested: (sx, sy, sw, sh) => home.openIconMenu(dockIcon, sx, sy, sw, sh)
                        onDragStarted: home.beginIconDrag(dockIcon)
                        onDragMoved: (sx, sy) => home.updateIconDrag(dockIcon, sx, sy)
                        onDragDropped: (sx, sy) => home.finishIconDrag(sx, sy)
                        onDragCancelled: home.cancelIconDrag()
                        onDrawerGesture: (sceneY) => homeContent.iconDrawerGesture(sceneY)
                        onDrawerGestureEnded: homeContent.iconDrawerGestureEnded()
                    }
                }

                Item {
                    width: dockAddTile.implicitWidth
                    height: Theme.iconApp - 8 + 8
                    // A full dock has no room; the icon menu's "Move to Dock" explains why.
                    visible: !DesktopLayout.dockFull

                    AddTile {
                        id: dockAddTile
                        compact: true
                        anchors.centerIn: parent
                        onTapped: home.openAppPicker("dock")
                    }
                }
            }
        }
        // ---- Editing overlays ---------------------------------------------------
        // Drop affordances: the source tile fades to a hole; these show where it lands.
        Rectangle {
            id: gridDropHole
            visible: home.dragActive && home.dropSurface === "desktop" && home.dropHole.width > 0
            x: home.dropHole.x
            y: home.dropHole.y
            width: home.dropHole.width
            height: home.dropHole.height
            radius: Theme.radiusM + 4
            color: Qt.alpha(ShellPrefs.accentColor, 0.10)
            border.width: 2
            border.color: Qt.alpha(ShellPrefs.accentColor, 0.55)
            z: 6
        }

        Rectangle {
            id: dockDropCaret
            visible: home.dragActive && home.dropSurface === "dock"
            x: home.dockCaret.x
            y: home.dockCaret.y
            width: 4
            height: 36
            radius: 2
            color: ShellPrefs.accentColor
            z: 8
        }

        DragGhost {
            id: dragGhost
            z: 40
        }

        IconContextMenu {
            id: iconMenu
            anchors.fill: parent
            z: 60
            onActionTriggered: (action) => home.handleIconAction(action)
            onDismissed: home.closeIconMenu()
        }

        IconEditor {
            id: editorSheet
            anchors.fill: parent
            z: 61
            backdropSource: wallpaperScene
            onApplied: (label, iconName) => home.applyIconEdit(label, iconName)
            onResetRequested: home.resetIconEdit()
        }

        AppPicker {
            id: appPicker
            anchors.fill: parent
            z: 62
            backdropSource: wallpaperScene
            existingIds: appPicker.surface === "dock" ? DesktopLayout.dockIds : DesktopLayout.desktopIds
            full: appPicker.surface === "dock" ? DesktopLayout.dockFull : false
            onPicked: (entryId) => home.addPickedApp(appPicker.surface, entryId)
        }
    }

    } // desktopScene (must be nested: PanelWindow reparents its direct children)

    // ---- Sliding App Drawer ----
    AppDrawer {
        id: appDrawer
        anchors.fill: parent
        backdropSource: desktopScene
        open: ShellState.drawerOpen
        onRequestClose: ShellState.drawerOpen = false
    }

    // ---- Material 3 Expressive Container Transform Launch/Close Morph ----
    AppLaunchMorph {
        id: launchMorph
        z: 50
        anchors.fill: parent
    }

    Connections {
        target: ShellState
        function onGoHome() {
            ShellState.drawerOpen = false
            home.closeOverlays()
            home.cancelIconDrag()
        }
        // The drawer is the app catalog: opening it dismisses any icon editing UI.
        function onDrawerOpenChanged() {
            if (ShellState.drawerOpen) {
                home.closeOverlays()
                home.cancelIconDrag()
            }
        }
    }

}
