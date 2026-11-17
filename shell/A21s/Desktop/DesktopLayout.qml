pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import A21s.Services
import "IconLayout.js" as Layout

// The ~/Desktop icon layout: which apps live on the desktop grid and in the dock, in
// what order, and the user's per-icon label/icon overrides.
//
// Ordering/membership rules live in IconLayout.js (host-tested: shell/tests/iconlayout.test.mjs).
// This singleton adds the file backend (~/.config/a21s-shell/desktop.json) and resolves
// ids to live DesktopEntries, so a saved layout survives a shell restart and an app that
// is not installed still renders (as a placeholder) instead of disappearing.
QtObject {
    id: root

    readonly property string statePath: {
        var home = Quickshell.env("HOME") || "/home/gamer"
        return home + "/.config/a21s-shell/desktop.json"
    }
    // What the desktop surface calls itself. The grid + dock are the home page, and the
    // home page is ~/Desktop (docs/ARCHITECTURE.md "shell seams").
    readonly property string surfaceLabel: "~/Desktop"
    readonly property string dockLabel: "Dock"

    // "desktop" | "dock" are the two editable surfaces; "" = drawer/picker tiles.
    readonly property string desktopSurface: Layout.SURFACE_DESKTOP
    readonly property string dockSurface: Layout.SURFACE_DOCK
    readonly property int dockCapacity: Layout.DOCK_MAX

    property var state: Layout.emptyState()
    property bool ready: false
    property string _lastWritten: ""

    readonly property var desktopIds: state.desktop
    readonly property var dockIds: state.dock
    readonly property var overrides: state.overrides
    readonly property bool dockFull: state.dock.length >= Layout.DOCK_MAX

    signal changed()

    // ---- resolution ---------------------------------------------------------
    function entryFor(id) {
        return DesktopEntries.byId(String(id)) || null
    }

    function seedFor(id) {
        return Layout.seedFor(Layout.DESKTOP_SEED, String(id))
            || Layout.seedFor(Layout.DOCK_SEED, String(id))
    }

    // A renderable model for one id: the real desktop entry when installed, otherwise the
    // seed placeholder (right name + glyph), otherwise a de-slugged id.
    function iconFor(id) {
        var e = entryFor(id)
        if (e)
            return e
        var seed = seedFor(id)
        if (seed)
            return { id: String(id), name: seed.name, materialIcon: seed.icon }
        return { id: String(id), name: Layout.prettyId(id), materialIcon: "widgets" }
    }

    function iconsFor(surface) {
        var ids = Layout.list(state, surface)
        var out = []
        for (var i = 0; i < ids.length; ++i)
            out.push(iconFor(ids[i]))
        return out
    }

    // Seed preference order: the first installed id wins; else the seed's placeholder id.
    // A saved id whose app was re-packaged (or a first-boot placeholder whose app got
    // installed later) is migrated to the installed alias, so the layout keeps pointing
    // at a real entry instead of a stale placeholder.
    function migrateIds(ids) {
        var out = []
        var changed = false
        for (var i = 0; i < ids.length; ++i) {
            var id = String(ids[i])
            if (entryFor(id)) {
                out.push(id)
                continue
            }
            var seed = seedFor(id)
            var migrated = ""
            if (seed) {
                for (var j = 0; j < seed.ids.length; ++j) {
                    if (entryFor(seed.ids[j])) {
                        migrated = seed.ids[j]
                        break
                    }
                }
            }
            if (migrated.length > 0 && migrated !== id) {
                out.push(migrated)
                changed = true
            } else {
                out.push(id)
            }
        }
        return { ids: out, changed: changed }
    }

    // Does this surface already hold the app? Placeholder ids and installed aliases of
    // the same seed count as the same app (used by the picker's "added" badge).
    function sameApp(a, b) {
        if (a === b)
            return true
        var sa = seedFor(a)
        var sb = seedFor(b)
        return !!sa && !!sb && sa === sb
    }

    function contains(surface, id) {
        var ids = Layout.list(state, surface)
        for (var i = 0; i < ids.length; ++i) {
            if (sameApp(ids[i], id))
                return true
        }
        return false
    }

    function seedId(seed) {
        for (var i = 0; i < seed.ids.length; ++i) {
            var e = entryFor(seed.ids[i])
            if (e)
                return String(e.id)
        }
        return seed.id
    }

    function defaultState() {
        var desktop = []
        var dock = []
        for (var i = 0; i < Layout.DESKTOP_SEED.length; ++i)
            desktop.push(seedId(Layout.DESKTOP_SEED[i]))
        for (i = 0; i < Layout.DOCK_SEED.length; ++i)
            dock.push(seedId(Layout.DOCK_SEED[i]))
        return Layout.createState(desktop, dock, {})
    }

    // ---- overrides ----------------------------------------------------------
    function overrideFor(id) {
        return Layout.overrideFor(state, id) || ({})
    }
    function labelFor(id, fallback) {
        return Layout.labelFor(state, id, fallback)
    }
    function iconNameFor(id, fallback) {
        return Layout.iconFor(state, id, fallback)
    }
    function hasOverride(id) {
        return Layout.overrideFor(state, id) !== null
    }

    // ---- mutations (each mirrors an IconLayout rule, then persists) ----------
    function apply(next) {
        root.state = next
        save()
        root.changed()
    }

    function add(surface, id, index) {
        apply(Layout.add(state, surface, id, index))
    }
    function remove(surface, id) {
        apply(Layout.remove(state, surface, id))
    }
    function move(surface, fromIndex, toIndex) {
        apply(Layout.move(state, surface, fromIndex, toIndex))
    }
    function transfer(fromSurface, id, toSurface, index) {
        apply(Layout.transfer(state, fromSurface, id, toSurface, index))
    }
    function setOverride(id, patch) {
        apply(Layout.setOverride(state, id, patch))
    }
    function clearOverride(id) {
        apply(Layout.clearOverride(state, id))
    }
    function has(surface, id) {
        return contains(surface, id)
    }
    function indexOf(surface, id) {
        return Layout.indexOf(state, surface, id)
    }
    function count(surface) {
        return Layout.count(state, surface)
    }

    // Rebuild the first-boot layout (used when the file is missing or unreadable).
    function reset() {
        root.state = defaultState()
        root.ready = true
        save()
        root.changed()
    }

    // ---- persistence --------------------------------------------------------
    property var saveTimer: Timer {
        interval: 160
        onTriggered: root.flush()
    }
    property var saveProc: Process {}

    function save() {
        saveTimer.restart()
    }

    function flush() {
        if (!root.ready)
            return
        var text = Layout.serialize(root.state)
        if (text === root._lastWritten)
            return
        root._lastWritten = text
        saveProc.command = [
            "sh", "-c",
            "mkdir -p \"$(dirname \"$1\")\" && printf '%s\\n' \"$2\" > \"$1.tmp\" && mv \"$1.tmp\" \"$1\"",
            "_", root.statePath, text
        ]
        saveProc.running = true
    }

    function applyText(raw) {
        // Our own write echoing back through the watcher: keep the current objects so
        // Repeaters are not rebuilt (and icons do not flicker) on every edit.
        if (root._lastWritten !== "" && String(raw).trim() === root._lastWritten.trim())
            return
        var parsed = Layout.tryParse(raw)
        if (parsed === null) {
            // Missing or corrupt file: start from the seed and write it back.
            reset()
            return
        }
        var desktop = migrateIds(parsed.desktop)
        var dock = migrateIds(parsed.dock)
        root.state = Layout.createState(desktop.ids, dock.ids, parsed.overrides)
        root.ready = true
        if (desktop.changed || dock.changed)
            save()
        root.changed()
    }

    property var fileWatcher: FileView {
        path: root.statePath
        watchChanges: true
        blockLoading: true
        printErrors: false
        onLoaded: root.applyText(text())
        onLoadFailed: root.reset()
        onFileChanged: reload()
    }
}
