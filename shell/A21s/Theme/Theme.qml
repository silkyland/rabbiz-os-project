pragma Singleton
import QtQuick

// A21s shell design tokens — One UI / Material dark.
// All UI MUST consume these; no magic colors/numbers in components.
QtObject {
    // Light/dark switch. Theme cannot import A21s.Services (dependency direction: Theme is
    // the bottom layer), so the shell/app roots bind this from ShellPrefs.lightTheme.
    property bool light: false

    // Palette — neutral graphite (dark) / soft paper (light). No blue/purple tint.
    readonly property color bg:           light ? "#F4F4F7" : "#0A0A0C"
    readonly property color surface:      light ? "#FFFFFF" : "#131316"
    readonly property color surfaceHigh:  light ? "#ECECF1" : "#1C1C20"
    readonly property color card:         light ? "#FFFFFF" : "#17171B"
    readonly property color separator:    light ? "#DCDCE3" : "#27272B"
    readonly property color scrim:        "#B3000000"
    readonly property color primary:      "#3B82F6"
    // NB: named "primaryContent", not "onPrimary" — QML parses "on<Capitalized>"
    // as a signal-handler-style name whenever a sibling property "primary"
    // exists, which makes the singleton fail to load at runtime.
    readonly property color primaryContent: "#FFFFFF"
    readonly property color accent:       "#E4E4E7"
    readonly property color text:         light ? "#101014" : "#F4F4F5"
    readonly property color textDim:      light ? "#57575F" : "#A1A1AA"
    readonly property color textFaint:    light ? "#8E8E98" : "#63636E"
    readonly property color danger:       "#FF5C6C"
    readonly property color success:      "#57D38D"
    readonly property color warn:         "#FFC857"
    readonly property color tileOn:       light ? "#D8E6FF" : "#2E4470"
    readonly property color tileOnText:   light ? "#12305C" : "#DCE8FF"
    // Text drawn straight on the wallpaper (clock, date, app labels). Every bundled
    // wallpaper is dark, so this stays light in BOTH themes; using Theme.text here made the
    // clock nearly invisible in light mode.
    readonly property color onWallpaper:     "#F4F4F5"
    readonly property color onWallpaperDim:  "#C9FFFFFF"
    readonly property color tileOff:      light ? "#E9E9EF" : "#22252F"
    readonly property color tileOffText:  light ? "#4A4A55" : "#8B93A3"

    // Shell and in-house apps use the pinned Panfrost path via a21s-qt-render when CMA allows, else software.
    readonly property color glass:        light ? "#B3FFFFFF" : "#B3131316"
    readonly property color glassHigh:    light ? "#D9FFFFFF" : "#D917171B"
    readonly property color glassLow:     light ? "#8CFFFFFF" : "#8C0F0F12"
    readonly property color glassBorder:  light ? "#33FFFFFF" : "#1FFFFFFF"
    readonly property color glassShadow:  light ? "#14000000" : "#33000000"
    readonly property color glassBlur:     light ? "#80FFFFFF" : "#80131316"
    readonly property color glassBlurHigh: light ? "#B3FFFFFF" : "#B317171B"
    readonly property int backdropBlurMax: 32

    // Shade sheets (grouped notifications / quick settings). These sit on the
    // captured, blurred screen, so cards are alpha layers rather than opaque
    // surfaces: white alpha on the dark theme, black alpha on the light one.
    readonly property color shadeCard:    light ? "#CCFFFFFF" : "#1AFFFFFF"
    readonly property color shadeCardOn:  light ? "#F2FFFFFF" : "#26FFFFFF"
    readonly property color shadeWell:    light ? "#14000000" : "#1AFFFFFF"
    readonly property color shadeLine:    light ? "#24000000" : "#1FFFFFFF"
    // Floating clear-all button; needs its own layer to stay legible over cards.
    readonly property color shadeFab:     light ? "#F2FFFFFF" : "#B318181C"

    // Type (px @ 720x1600, scale=1)
    readonly property string fontFamily: "Noto Sans"
    readonly property int fontStatus:   20
    readonly property int fontCaption:  22
    readonly property int fontBody:     28
    readonly property int fontTitle:    34
    readonly property int fontHeadline: 46
    // Section labels ("CONVERSATIONS", timestamps) sit below fontStatus.
    readonly property int fontMicro:    18
    // Shade clock — the one oversized display number in the shell.
    readonly property int fontClock:    100

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

    // Shade layout (720 px wide; One UI 7-style grouped sheets). Cards use
    // radiusCard so every sheet element shares one corner language.
    readonly property int radiusCard:        36
    readonly property int shadeControlButton: 88   // round header button
    readonly property int shadeTileHeight:    106  // large pill tile
    readonly property int shadeIconWell:      90   // icon circle inside it
    readonly property int shadeGroupWell:     64   // icon circle in a grouped row
    readonly property int shadeGridTile:      124  // dense icon-tile circle
    readonly property int shadeMediaHeight:   336
    readonly property int shadeSliderWidth:   86   // vertical brightness/volume pill

    // Motion
    readonly property int animFast: 140
    readonly property int animMed:  260
    readonly property int animSlow: 420
    readonly property int animEase: Easing.OutCubic
}
