pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Desktop contract writer: pushes the shell's accent (ShellPrefs) into every channel real apps
// read -- KDE colour scheme + kdeglobals (Qt apps) and gsettings color-scheme (GTK/Chromium) --
// by running /usr/local/bin/a21s-appearance. Re-applied whenever the accent or light/dark changes.
Singleton {
    id: root

    property string accent: String(ShellPrefs.accentColor)
    property bool light: ShellPrefs.lightTheme
    property bool applied: false

    function apply() {
        applyProc.command = ["a21s-appearance", "--accent", root.accent].concat(root.light ? ["--light"] : [])
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
