pragma Singleton
import QtQuick
import Quickshell.Wayland

// Global shell UI state. Owned by the integration lead.
QtObject {
    // 0..1 drag progress of the notification/quick-settings shade
    property real shadeProgress: 0
    readonly property bool shadeOpen: shadeProgress > 0.5
    // true while an app grid/drawer page is shown on the home screen
    property bool drawerOpen: false
    // shared 0..1 progress synced between HomeScreen and AppDrawer
    property real drawerProgress: 0.0
    // currently focused app (desktop id), "" when on home
    property string activeAppId: ""
    // true while any app toplevel is visible (i.e. not on the home screen)
    readonly property bool appOpen: {
        var tops = ToplevelManager.toplevels ? ToplevelManager.toplevels.values : [];
        for (var i = 0; i < tops.length; ++i) {
            if (tops[i] && !tops[i].minimized)
                return true;
        }
        return false;
    }
    // global overlays
    property bool powerMenuOpen: false
    property bool lockScreenOpen: false
    property bool torchOverlayOpen: false
    property bool sleeping: false
    // connectivity panel (Wi-Fi / Bluetooth); tab: 0=Wi-Fi, 1=Bluetooth
    property bool connPanelOpen: false
    property int connPanelTab: 0

    // app launch / close container morph origin state
    property bool morphActive: false
    property bool morphClosing: false
    property real morphOriginX: 360
    property real morphOriginY: 800
    property real morphOriginW: 88
    property real morphOriginH: 88
    property real morphOriginRadius: 24
    property string morphAppName: ""
    property string morphMaterialIcon: ""
    property url morphIconUrl: ""
    property color morphTint: "#3B82F6"

    // set by task switcher / nav gesture to return home
    signal goHome()
    signal triggerLaunchMorph(real ox, real oy, real ow, real oh, real orad,
                              string name, string matIcon, url iconUrl, color tint)
    signal triggerCloseMorph()
    // gesture navigation (Windowing/EdgeBack, Panels/NavBar): action = home | back | recents | prev | next
    signal navAction(string action)
    // 0..1 live progress of an in-flight gesture, for visual feedback
    property real gestureProgress: 0.0
    property string gestureKind: ""
}
