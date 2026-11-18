import QtQuick
import Quickshell
import Quickshell.Wayland
import Rabbiz.Theme
import Rabbiz.Services
import Rabbiz.Widgets

// Material 3 Expressive container-transform overlay.
// Morphs seamlessly between the tapped AppIcon tile bounds and fullscreen
// when launching an app, and reverses back on triggerCloseMorph().
Item {
    id: root

    // 0 = icon tile geometry, 1 = fullscreen surface.
    property real t: 0.0
    property real fadeOpacity: 0.0

    visible: ShellState.morphActive && fadeOpacity > 0.01

    readonly property real curX: ShellState.morphOriginX * (1.0 - t)
    readonly property real curY: ShellState.morphOriginY * (1.0 - t)
    readonly property real curW: ShellState.morphOriginW + (root.width - ShellState.morphOriginW) * t
    readonly property real curH: ShellState.morphOriginH + (root.height - ShellState.morphOriginH) * t
    readonly property real curRadius: ShellState.morphOriginRadius * (1.0 - t)

    function _lerpColor(a, b, f) {
        return Qt.rgba(
            a.r + (b.r - a.r) * f,
            a.g + (b.g - a.g) * f,
            a.b + (b.b - a.b) * f,
            1.0
        )
    }

    Connections {
        target: ShellState

        function onTriggerLaunchMorph(ox, oy, ow, oh, orad, name, matIcon, iconUrl, tint) {
            closeAnim.stop()
            launchAnim.stop()
            fadeOut.stop()
            ShellState.morphOriginX = ox
            ShellState.morphOriginY = oy
            ShellState.morphOriginW = Math.max(40, ow)
            ShellState.morphOriginH = Math.max(40, oh)
            ShellState.morphOriginRadius = orad
            ShellState.morphAppName = name || ""
            ShellState.morphMaterialIcon = matIcon || ""
            ShellState.morphIconUrl = iconUrl || ""
            ShellState.morphTint = tint
            ShellState.morphClosing = false
            ShellState.morphActive = true
            ShellState.morphSnapshot = false
            root._snapWanted = false
            root._closeStarted = false
            root.t = 0.0
            root.fadeOpacity = 1.0
            launchAnim.start()
        }

        function onTriggerCloseMorph() {
            if (root.width <= 0 || root.height <= 0)
                return
            launchAnim.stop()
            fadeOut.stop()
            closeAnim.stop()
            ShellState.morphClosing = true
            ShellState.morphActive = true
            root.t = 1.0
            // Do not fade in yet: try to grab the frame that is still on screen, so the
            // shrink starts from the app's real pixels (M7.6). The fallback timer
            // guarantees the morph runs even when capture is unavailable.
            root.fadeOpacity = 0.0
            root._snapWanted = true
            snap.screen = Quickshell.screens.length > 0 ? Quickshell.screens[0] : null
            snap.captureFrame()
            snapFallback.restart()
        }
    }

    // --- close morph snapshot -------------------------------------------------
    // A single wlr-screencopy frame of the current output, frozen and then shrunk with the
    // container. If the compositor refuses (no area/backbuffer) hasContent stays false and
    // the plain container gradient is used instead.
    property bool _snapWanted: false
    property bool _closeStarted: false

    function _beginClose(useSnapshot) {
        if (root._closeStarted)
            return
        root._closeStarted = true
        snapFallback.stop()
        ShellState.morphSnapshot = useSnapshot
        root.fadeOpacity = 0.92
        closeAnim.start()
    }

    property var snapFallback: Timer {
        interval: 200
        repeat: false
        onTriggered: root._beginClose(snap.hasContent)
    }

    // The overlay stays fully expanded until the app's toplevel has mapped (Launcher.holding
    // goes false) -- the LaunchFlow timeout guarantees it can never stay up forever.
    property bool expanded: false
    function _maybeFinish() {
        if (root.expanded && !Launcher.holding && ShellState.morphActive && !ShellState.morphClosing)
            fadeOut.start()
    }
    Connections {
        target: Launcher
        function onHoldingChanged() { root._maybeFinish() }
    }

    SequentialAnimation {
        id: launchAnim
        ScriptAction { script: root.expanded = false }
        NumberAnimation {
            target: root
            property: "t"
            from: 0.0
            to: 1.0
            duration: 320
            easing.type: Easing.OutQuint
        }
        ScriptAction { script: { root.expanded = true; root._maybeFinish() } }
    }

    SequentialAnimation {
        id: fadeOut
        NumberAnimation {
            target: root
            property: "fadeOpacity"
            to: 0.0
            duration: 180
            easing.type: Easing.OutCubic
        }
        ScriptAction { script: { ShellState.morphActive = false; root.expanded = false } }
    }

    SequentialAnimation {
        id: closeAnim
        ParallelAnimation {
            NumberAnimation {
                target: root
                property: "t"
                from: 1.0
                to: 0.0
                duration: 280
                easing.type: Easing.OutQuint
            }
            NumberAnimation {
                target: root
                property: "fadeOpacity"
                from: 0.92
                to: 0.0
                duration: 260
                easing.type: Easing.InCubic
            }
        }
        ScriptAction {
            script: {
                ShellState.morphActive = false
                ShellState.morphClosing = false
                ShellState.morphSnapshot = false
                root._snapWanted = false
                root._closeStarted = false
            }
        }
    }

    Rectangle {
        id: container
        x: root.curX
        y: root.curY
        width: root.curW
        height: root.curH
        radius: root.curRadius
        opacity: root.fadeOpacity
        clip: true
        color: root._lerpColor(Theme.card, Theme.bg, root.t)
        border.width: Math.max(0, 1.0 - root.t)
        border.color: Qt.alpha(ShellPrefs.accentColor, 0.35)

        ScreencopyView {
            id: snap
            anchors.fill: parent
            visible: root._snapWanted && root._closeStarted && hasContent
            live: false
            paintCursor: false
            property var screen: null
            captureSource: screen
            onHasContentChanged: {
                if (hasContent && root._snapWanted && !root._closeStarted)
                    root._beginClose(true)
            }
        }

        // Subtle radial-like accent bloom inside the splash container
        Rectangle {
            visible: root.t > 0.05 && !ShellState.morphSnapshot
            anchors.centerIn: parent
            width: Math.min(parent.width * 0.9, 340)
            height: width
            radius: width / 2
            color: Qt.alpha(ShellState.morphTint, 0.14 * root.t)
        }

        Column {
            anchors.centerIn: parent
            spacing: Theme.spaceM
            // With a live screenshot as the content, the icon+name would sit on top of the app.
            visible: !ShellState.morphSnapshot

            Item {
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.round(52 + 44 * root.t)
                height: width

                Image {
                    id: morphImg
                    anchors.fill: parent
                    source: ShellState.morphIconUrl
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    smooth: true
                    visible: status === Image.Ready
                }

                MaterialIcon {
                    anchors.centerIn: parent
                    visible: morphImg.status !== Image.Ready && ShellState.morphMaterialIcon.length > 0
                    name: ShellState.morphMaterialIcon
                    size: Math.round(parent.width * 0.85)
                    color: Theme.text
                }

                Text {
                    anchors.centerIn: parent
                    visible: morphImg.status !== Image.Ready && ShellState.morphMaterialIcon.length === 0
                    text: ShellState.morphAppName.length > 0
                        ? ShellState.morphAppName.charAt(0).toUpperCase()
                        : ""
                    color: Theme.text
                    font.pixelSize: Math.round(parent.width * 0.72)
                    font.family: Theme.fontFamily
                    font.weight: Font.DemiBold
                }
            }

            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: ShellState.morphAppName.length > 0
                text: ShellState.morphAppName
                color: Theme.text
                opacity: Math.max(0, (root.t - 0.35) / 0.65)
                font.pixelSize: Theme.fontTitle
                font.family: Theme.fontFamily
                font.weight: Font.Medium
            }
        }
    }
}
