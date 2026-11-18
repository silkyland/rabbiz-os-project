pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import Rabbiz.Services
import "LaunchFlow.js" as Flow

// Owns "tap an icon -> app window appears". Pure logic lives in LaunchFlow.js (host-tested);
// this singleton only wires it to DesktopEntry.execute(), ToplevelManager and the morph overlay.
Singleton {
    id: root

    property var flow: Flow.create()
    readonly property string state: flow.state
    // true from the tap until the app's first frame is likely on screen
    readonly property bool holding: flow.state === "launching" || flow.state === "mapped"
    property var currentEntry: null
    property var _preexisting: []   // toplevels open at launch time (for the "first new window" fallback)

    function launch(entry, origin, runningToplevel) {
        ShellState.triggerLaunchMorph(origin.x, origin.y, origin.w, origin.h, origin.radius,
                                      origin.name, origin.materialIcon, origin.iconUrl, origin.tint)
        root.currentEntry = entry
        if (runningToplevel) {            // already running: just raise it, no wait for a map event
            runningToplevel.minimized = false
            runningToplevel.activate()
            root.flow = Flow.toplevelMapped(Flow.launch(root.flow, entry ? entry.id : "", origin, Date.now()),
                                            entry ? entry.id : "", Date.now())
            return
        }
        root.flow = Flow.launch(root.flow, entry ? entry.id : "", origin, Date.now())
        var cur = ToplevelManager.toplevels ? ToplevelManager.toplevels.values : []
        root._preexisting = cur.slice()
        _spawn(entry)
    }

    function _spawn(entry) {
        Appearance.launchDesktopEntry(entry)
    }

    // The running window of a desktop entry, or null. Generic: no per-app table.
    function findToplevel(entry) {
        if (!entry) return null
        var tops = ToplevelManager.toplevels ? ToplevelManager.toplevels.values : []
        var name = String(entry.name || "").toLowerCase()
        for (var i = 0; i < tops.length; ++i) {
            var t = tops[i]
            if (!t) continue
            if (Flow.matches(t.appId, entry.id, entry.startupClass)) return t
            if (name.length > 0 && String(t.title || "").toLowerCase() === name) return t
        }
        return null
    }

    function launchEntry(entry, origin) {
        launch(entry, origin, findToplevel(entry))
    }

    function _scan() {
        if (root.flow.state !== "launching" || !root.currentEntry) return
        var tops = ToplevelManager.toplevels ? ToplevelManager.toplevels.values : []
        for (var i = 0; i < tops.length; ++i) {
            // appId match, or -- for hosts that publish no app_id (e.g. a nested Weston) -- the first
            // window that was not open when the launch started
            var isNew = root._preexisting.indexOf(tops[i]) < 0
            if (Flow.matches(tops[i].appId, root.currentEntry.id, root.currentEntry.startupClass)
                    || (isNew && !tops[i].appId)) {
                root.flow = Flow.toplevelMapped(root.flow, root.flow.appId, Date.now())
                return
            }
        }
    }

    Connections {
        target: ToplevelManager.toplevels
        function onObjectInsertedPost() { root._scan() }
    }

    Timer {
        interval: 100
        running: root.flow.state !== "idle"
        repeat: true
        onTriggered: { root._scan(); root.flow = Flow.tick(root.flow, Date.now()) }
    }

    // Test/automation hook: `quickshell -p shell.qml ipc call launcher launch kalk`
    IpcHandler {
        target: "launcher"
        function launch(name: string): string {
            var e = DesktopEntries.byId(name) || DesktopEntries.byId("org.kde." + name)
                  || DesktopEntries.heuristicLookup(name)
            if (!e) return "no desktop entry for " + name
            root.launch(e, { x: 316, y: 1100, w: 88, h: 88, radius: 24, name: e.name,
                             materialIcon: "", iconUrl: "", tint: "#3B82F6" }, null)
            return "launching " + e.id
        }
        function state(): string { return root.flow.state }
        // debugging aid: "appId|title" per open toplevel, one per line
        function toplevels(): string {
            var tops = ToplevelManager.toplevels ? ToplevelManager.toplevels.values : []
            var out = []
            for (var i = 0; i < tops.length; ++i) out.push(String(tops[i].appId) + "|" + String(tops[i].title))
            return out.join("\n")
        }
    }
}
