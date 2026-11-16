//@ pragma UseQApplication
import Quickshell
import QtQuick
import A21s.Services
import A21s.Theme
import A21s.Windowing
import A21s.Desktop
import A21s.Panels

ShellRoot {
    // Theme is the bottom layer and may not import Services, so the light/dark switch is
    // wired here (shell.qml is wiring only).
    Binding { target: Theme; property: "light"; value: ShellPrefs.lightTheme }

    // instantiate the desktop-contract writer (singletons are lazy)
    Component.onCompleted: Appearance.apply()
    StatusBar {}
    HomeScreen {}
    Shade {}
    NavBar {}
    EdgeBack {}
    ConnectivityPanel {}
    PowerMenu {}
    HudOverlay {}
}
