//@ pragma UseQApplication
import Quickshell
import QtQuick
import A21s.Services
import A21s.Windowing
import A21s.Desktop
import A21s.Panels

ShellRoot {
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
