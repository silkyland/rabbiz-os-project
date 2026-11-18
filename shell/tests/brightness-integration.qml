// Device integration probe: run with QML_IMPORT_PATH=/usr/share/rabbiz-shell.
// Intentionally persists 64%; use it before a reboot verification.
import QtQuick
import Quickshell
import Rabbiz.Services

Scope {
    property var backlight: Brightness
    property int phase: 0
    property int ticks: 0

    Timer {
        interval: 500
        running: true
        repeat: true
        onTriggered: {
            ticks++
            if (ticks > 40) {
                console.error("BRIGHTNESS_TEST_FAIL: readiness timeout")
                Qt.quit()
                return
            }
            if (!backlight.available || !ShellPrefs.loaded) return
            if (phase === 0) {
                backlight.setLevel(64)
            } else if (phase === 1) {
                if (ShellPrefs.brightnessLevel !== 64) {
                    console.error("BRIGHTNESS_TEST_FAIL: user setting not retained")
                    Qt.quit()
                    return
                }
                backlight.setTransientLevel(0)
            } else if (phase === 2) {
                if (ShellPrefs.brightnessLevel !== 64) {
                    console.error("BRIGHTNESS_TEST_FAIL: screen-off replaced preference")
                    Qt.quit()
                    return
                }
                backlight.setTransientLevel(64)
            } else {
                console.log("BRIGHTNESS_INTEGRATION_PASS: user=64, transient zero excluded")
                Qt.quit()
            }
            phase++
        }
    }
}
