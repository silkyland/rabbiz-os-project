//@ pragma UseQApplication
import Quickshell
import QtQuick
import Rabbiz.Services
import Rabbiz.Theme
import Rabbiz.Windowing
import Rabbiz.Desktop
import Rabbiz.Panels

ShellRoot {
    // Theme is the bottom layer and may not import Services, so the light/dark switch is
    // wired here (shell.qml is wiring only).
    Binding { target: Theme; property: "light"; value: ShellPrefs.lightTheme }

    // instantiate the desktop-contract writer (singletons are lazy)
    Component.onCompleted: {
        Appearance.apply()
        Power.start()
    }
    StatusBar {}
    DesktopView {}
    Shade {}
    NavBar {}
    EdgeBack {}
    ConnectivityPanel {}
    PowerMenu {}
    HudOverlay {}
    CaptureFlash {}
}
