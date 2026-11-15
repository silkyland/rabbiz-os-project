pragma Singleton
import QtQuick

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
    // global overlays
    property bool powerMenuOpen: false
    property bool lockScreenOpen: false
    property bool torchOverlayOpen: false

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
}
