pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Desktop contract writer: pushes the shell's accent (ShellPrefs) into every channel real apps
// read -- KDE colour scheme + kdeglobals (Qt apps) and gsettings color-scheme (GTK/Chromium) --
// by running /usr/local/bin/rabbiz-appearance. Re-applied whenever the accent or light/dark changes.
Singleton {
    id: root

    property string accent: String(ShellPrefs.accentColor)
    property bool light: ShellPrefs.lightTheme
    property bool applied: false

    // GPU allocation is private to the shell, never part of the app desktop contract.
    readonly property var appEnvironment: ({
        "QT_QPA_PLATFORMTHEME": "kde",
        "PLASMA_PLATFORM": "phone:handset",
        "QT_QUICK_CONTROLS_STYLE": null,
        "QT_QUICK_BACKEND": "software",
        "A21S_FORCE_SOFTWARE": "1",
        "QSG_RHI_BACKEND": null,
        "QS_DISABLE_DMABUF": null,
        "LD_PRELOAD": null,
        "__EGL_VENDOR_LIBRARY_FILENAMES": null,
        "DRI_PRIME": null,
        "RABBIZ_EXYNOS_RENDER": null,
        "RABBIZ_EXYNOS_PRIMARY": null
    })

    function launchDesktopEntry(entry) {
        if (!entry) return
        const command = entry.command && entry.command.length > 0 && !entry.runInTerminal
            ? entry.command : ["gtk-launch", entry.id]
        Quickshell.execDetached({ command: command, environment: root.appEnvironment,
                                  workingDirectory: entry.workingDirectory || "" })
    }

    function apply() {
        applyProc.command = ["rabbiz-appearance", "--accent", root.accent].concat(root.light ? ["--light"] : [])
        applyProc.running = true
    }

    onAccentChanged: debounce.restart()
    onLightChanged: debounce.restart()
    Timer { id: debounce; interval: 400; onTriggered: root.apply() }
    Component.onCompleted: debounce.start()

    property var applyProc: Process {
        stdout: StdioCollector { onStreamFinished: root.applied = this.text.indexOf("ok") >= 0 }
    }
}
