pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import A21s.Services
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
        _spawn(entry)
    }

    // The shell process runs with the platform theme disabled (it draws its own UI); apps must not
    // inherit that, so they are started with the desktop-contract environment instead.
    readonly property var appEnv: ({
        "QT_QPA_PLATFORMTHEME": "kde",
        "PLASMA_PLATFORM": "phone:handset",
        "QT_QUICK_CONTROLS_STYLE": null
    })

    function _spawn(entry) {
        if (!entry) return
        if (entry.command && entry.command.length > 0 && !entry.runInTerminal)
            Quickshell.execDetached({ command: entry.command, environment: root.appEnv,
                                      workingDirectory: entry.workingDirectory || "" })
        else if (typeof entry.execute === "function")
            entry.execute()
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
        for (var i = 0; i < tops.length; ++i)
            if (Flow.matches(tops[i].appId, root.currentEntry.id, root.currentEntry.startupClass)) {
                root.flow = Flow.toplevelMapped(root.flow, root.flow.appId, Date.now())
                return
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
    }
}
