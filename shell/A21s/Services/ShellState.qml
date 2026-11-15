pragma Singleton
import QtQuick

// Global shell UI state. Owned by the integration lead.
QtObject {
    // 0..1 drag progress of the notification/quick-settings shade
    property real shadeProgress: 0
    readonly property bool shadeOpen: shadeProgress > 0.5
    // true while an app grid/drawer page is shown on the home screen
    property bool drawerOpen: false
    // currently focused app (desktop id), "" when on home
    property string activeAppId: ""
    // set by task switcher / nav gesture to return home
    signal goHome()
}
