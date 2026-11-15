pragma Singleton
import QtQuick

// A21s shell design tokens — One UI / Material dark.
// All UI MUST consume these; no magic colors/numbers in components.
QtObject {
    // Palette
    readonly property color bg:           "#0B0C10"
    readonly property color surface:      "#15171D"
    readonly property color surfaceHigh:  "#1E212B"
    readonly property color card:         "#1C1F28"
    readonly property color separator:    "#262A35"
    readonly property color scrim:        "#B3000000"
    readonly property color primary:      "#4C8DFF"
    // NB: named "primaryContent", not "onPrimary" — QML parses "on<Capitalized>"
    // as a signal-handler-style name whenever a sibling property "primary"
    // exists, which makes the singleton fail to load at runtime.
    readonly property color primaryContent: "#FFFFFF"
    readonly property color accent:       "#7BE0C3"
    readonly property color text:         "#F2F4F8"
    readonly property color textDim:      "#9AA3B2"
    readonly property color textFaint:    "#5C6472"
    readonly property color danger:       "#FF5C6C"
    readonly property color success:      "#57D38D"
    readonly property color warn:         "#FFC857"
    readonly property color tileOn:       "#2E4470"
    readonly property color tileOnText:   "#DCE8FF"
    readonly property color tileOff:      "#22252F"
    readonly property color tileOffText:  "#8B93A3"

    // Type (px @ 720x1600, scale=1)
    readonly property string fontFamily: "Noto Sans"
    readonly property int fontStatus:   20
    readonly property int fontCaption:  22
    readonly property int fontBody:     28
    readonly property int fontTitle:    34
    readonly property int fontHeadline: 46

    // Spacing
    readonly property int spaceXS: 6
    readonly property int spaceS:  12
    readonly property int spaceM:  20
    readonly property int spaceL:  32
    readonly property int spaceXL: 48

    // Radii
    readonly property int radiusS:    12
    readonly property int radiusM:    20
    readonly property int radiusL:    28
    readonly property int radiusPill: 999

    // Icon sizes
    readonly property int iconS:   36
    readonly property int iconM:   52
    readonly property int iconL:   64
    readonly property int iconApp: 96

    // Metrics
    readonly property int statusBarHeight: 44
    readonly property int navPillWidth:    200
    readonly property int navPillHeight:   8
    readonly property int navZoneHeight:   48
    readonly property int shadeHandleZone: 32

    // Motion
    readonly property int animFast: 140
    readonly property int animMed:  260
    readonly property int animSlow: 420
    readonly property int animEase: Easing.OutCubic
}
