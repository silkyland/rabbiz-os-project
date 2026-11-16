import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import Quickshell.Services.UPower
import QtQuick
import QtQuick.Layouts
import A21s.Theme
import A21s.Services
import A21s.Widgets
import A21s.Windowing

// Home screen — 720x1600 artwork wallpaper + parallax scale morph on drawer drag,
// tappable morphing clock + telemetry At-a-Glance pill, 2x4 utility grid,
// swipeable 3-card Hero Deck (MPRIS Media / System Boost),
// flying Search pill morph, frosted dock, AppDrawer & AppLaunchMorph overlay.
PanelWindow {
    id: home

    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Background
    WlrLayershell.keyboardFocus: ShellState.drawerOpen
        ? WlrKeyboardFocus.OnDemand
        : WlrKeyboardFocus.None
    color: Theme.bg

    SystemClock {
        id: clock
        precision: SystemClock.Minutes
    }

    Process {
        id: settingsProc
        environment: Appearance.appEnvironment
        command: ["a21s-settings"]
    }

    readonly property var wp: ShellPrefs.activeWallpaper
    readonly property string timeFormatStr: ShellPrefs.use24h ? "HH:mm" : "h:mm ap"
    readonly property string hourStr: Qt.formatDateTime(clock.date, ShellPrefs.use24h ? "HH" : "h")
    readonly property string minStr: Qt.formatDateTime(clock.date, "mm")
    readonly property string ampmStr: ShellPrefs.use24h ? "" : Qt.formatDateTime(clock.date, "ap").toUpperCase()

    readonly property bool batteryOk: UPower.displayDevice !== null
        && UPower.displayDevice.ready
        && UPower.displayDevice.isLaptopBattery
    readonly property string powerLabel: batteryOk
        ? (Math.round(UPower.displayDevice.percentage * 100) + "%")
        : "AC"
    readonly property string netLabel: SystemControl.airplaneMode
        ? "Airplane"
        : (Network.wifiConnected ? Network.activeSsid : (Network.wifiEnabled ? "Wi-Fi" : "Offline"))

    // The media card only exists while something is really playing / a player is open; otherwise the
    // deck leads with the RabbizOS greeting card instead of an empty placeholder player.
    readonly property var heroDeckModel: Media.hasRealPlayer ? ["media", "system"] : ["welcome", "system"]
    readonly property string greeting: {
        var h = clock.date.getHours()
        return h < 5 ? "Good night" : h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening"
    }

    function cycleClockStyle() {
        var order = ["editorial", "classic", "compact"]
        var idx = order.indexOf(ShellPrefs.clockStyle)
        ShellPrefs.clockStyle = order[(idx + 1) % order.length]
        ShellPrefs.save()
        clockMorphAnim.restart()
    }

    // The drawer samples this sibling scene, which excludes the drawer itself.
    Item {
        id: desktopScene
        anchors.fill: parent

    // ---- Fixed-frame Wallpaper layer with subtle parallax zoom when Drawer opens ----
    Item {
        id: wallpaperScene
        anchors.fill: parent
        clip: true

        Item {
            anchors.fill: parent
            // No scale here: a full-screen image resampled every frame is the most
            // expensive thing the software renderer can do during the drawer slide
            // (owner: "drawer stutters, not 60 fps"). The drawer dim below still gives
            // the depth cue without touching the bitmap.

            Rectangle {
                anchors.fill: parent
                gradient: Gradient {
                    GradientStop { position: 0.0; color: home.wp.top }
                    GradientStop { position: 0.48; color: home.wp.mid }
                    GradientStop { position: 1.0; color: home.wp.bottom }
                }
            }

            Image {
                id: wallpaperImg
                anchors.fill: parent
                source: home.wp.image || ""
                fillMode: Image.PreserveAspectCrop
                smooth: true
                asynchronous: true
                visible: status === Image.Ready
            }

            Item {
                anchors.fill: parent
                opacity: wallpaperImg.status === Image.Ready ? 0.09 : 0.12

                Rectangle {
                    width: home.width * 1.15
                    height: width
                    radius: width / 2
                    x: home.width * 0.25
                    y: -home.height * 0.08
                    color: Qt.alpha(home.wp.orb1, 0.45)

                    Rectangle {
                        anchors.centerIn: parent
                        width: parent.width * 0.68
                        height: width
                        radius: width / 2
                        color: Qt.alpha(home.wp.orb1, 0.55)
                    }
                }

                Rectangle {
                    width: home.width * 1.05
                    height: width
                    radius: width / 2
                    x: -home.width * 0.35
                    y: home.height * 0.32
                    color: Qt.alpha(home.wp.orb2, 0.40)

                    Rectangle {
                        anchors.centerIn: parent
                        width: parent.width * 0.65
                        height: width
                        radius: width / 2
                        color: Qt.alpha(home.wp.orb2, 0.50)
                    }
                }
            }
        }

        // Top & bottom contrast vignettes
        Rectangle {
            anchors { left: parent.left; right: parent.right; top: parent.top }
            height: home.height * 0.22
            gradient: Gradient {
                GradientStop { position: 0.0; color: Qt.alpha("#000000", 0.28) }
                GradientStop { position: 1.0; color: "transparent" }
            }
        }

        Rectangle {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: home.height * 0.28
            gradient: Gradient {
                GradientStop { position: 0.0; color: "transparent" }
                GradientStop { position: 1.0; color: Qt.alpha("#000000", 0.52) }
            }
        }
    }

    // ---- Foreground Home Content (scales & fades smoothly into depth when drawer opens) ----
    Item {
        id: homeContent
        anchors.fill: parent
        // Fade only. Scaling the whole home subtree (icons, cards, text) every frame
        // forces the software renderer to resample everything, which is what made the
        // drawer slide stutter; the dim alone still reads as "pushed back".
        opacity: 1.0 - 0.55 * ShellState.drawerProgress

        // ---- Gesture layer: interactive vertical drag to open the App Drawer ----
        MouseArea {
            id: homeSwipe
            anchors.fill: parent
            property real pressY: 0
            property bool draggingDrawer: false

            onPressed: (mouse) => {
                pressY = mouse.y
                draggingDrawer = false
            }
            onPositionChanged: (mouse) => {
                if (!pressed || ShellState.drawerOpen)
                    return
                var dy = pressY - mouse.y
                if (dy > 14)
                    draggingDrawer = true
                if (draggingDrawer) {
                    var range = Math.max(220, home.height * 0.45)
                    ShellState.drawerProgress = Math.max(0, Math.min(1, dy / range))
                }
            }
            onReleased: {
                if (!draggingDrawer)
                    return
                draggingDrawer = false
                if (ShellState.drawerProgress > 0.24) {
                    ShellState.drawerOpen = true
                    appDrawer.snapTo(1.0)
                } else {
                    ShellState.drawerOpen = false
                    appDrawer.snapTo(0.0)
                }
            }
        }

        // ---- Clock (tap to morph style) + Date + Telemetry At-a-Glance Pill ----
        Item {
            id: clockBlock
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
                topMargin: Theme.statusBarHeight + parent.height * 0.032
                leftMargin: Theme.spaceL
                rightMargin: Theme.spaceL
            }
            height: clockCol.implicitHeight

            ColumnLayout {
                id: clockCol
                anchors.fill: parent
                spacing: Theme.spaceS

                // Clickable Clock wrapper with style-cycle morph animation
                Item {
                    id: clockMorphWrap
                    Layout.fillWidth: true
                    Layout.preferredHeight: clockStack.implicitHeight

                    SequentialAnimation {
                        id: clockMorphAnim
                        ParallelAnimation {
                            NumberAnimation { target: clockMorphWrap; property: "scale"; from: 0.92; to: 1.0; duration: Theme.animMed; easing.type: Easing.OutBack }
                            NumberAnimation { target: clockMorphWrap; property: "opacity"; from: 0.45; to: 1.0; duration: Theme.animMed; easing.type: Theme.animEase }
                        }
                    }

                    Item {
                        id: clockStack
                        anchors.fill: parent
                        implicitHeight: ShellPrefs.clockStyle === "compact"
                            ? compactClock.implicitHeight
                            : (ShellPrefs.clockStyle === "classic" ? classicClock.implicitHeight : editorialClock.implicitHeight)

                        // Style 1: Editorial
                        RowLayout {
                            id: editorialClock
                            visible: ShellPrefs.clockStyle === "editorial"
                            anchors.horizontalCenter: parent.horizontalCenter
                            spacing: 4

                            Text {
                                text: home.hourStr
                                color: Theme.onWallpaper
                                font.pixelSize: 120
                                font.family: Theme.fontFamily
                                font.weight: Font.Light
                            }
                            Text {
                                text: ":"
                                color: Qt.alpha(ShellPrefs.accentColor, 0.75)
                                font.pixelSize: 108
                                font.family: Theme.fontFamily
                                font.weight: Font.ExtraLight
                            }
                            Text {
                                text: home.minStr
                                color: ShellPrefs.accentColor
                                font.pixelSize: 120
                                font.family: Theme.fontFamily
                                font.weight: Font.DemiBold
                            }
                            Text {
                                visible: home.ampmStr.length > 0
                                text: home.ampmStr
                                color: Theme.onWallpaperDim
                                font.pixelSize: Theme.fontCaption
                                font.family: Theme.fontFamily
                                font.weight: Font.DemiBold
                                Layout.alignment: Qt.AlignBottom
                                Layout.bottomMargin: 22
                            }
                        }

                        // Style 2: Classic
                        Text {
                            id: classicClock
                            visible: ShellPrefs.clockStyle === "classic"
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: Qt.formatDateTime(clock.date, home.timeFormatStr)
                            color: Theme.onWallpaper
                            font.pixelSize: 114
                            font.family: Theme.fontFamily
                            font.weight: Font.Light
                        }

                        // Style 3: Compact
                        Text {
                            id: compactClock
                            visible: ShellPrefs.clockStyle === "compact"
                            width: parent.width
                            text: Qt.formatDateTime(clock.date, home.timeFormatStr)
                            color: Theme.onWallpaper
                            font.pixelSize: 82
                            font.family: Theme.fontFamily
                            font.weight: Font.DemiBold
                        }
                    }

                    TapHandler {
                        onTapped: home.cycleClockStyle()
                    }
                }

                Text {
                    Layout.alignment: ShellPrefs.clockStyle === "compact"
                        ? Qt.AlignLeft : Qt.AlignHCenter
                    text: Qt.formatDateTime(clock.date, "dddd, MMMM d")
                    color: Theme.onWallpaperDim
                    font.pixelSize: Theme.fontBody
                    font.family: Theme.fontFamily
                    font.weight: Font.Medium
                }

                // Telemetry + Status At-a-Glance pill (tap -> Settings)
                Rectangle {
                    Layout.alignment: ShellPrefs.clockStyle === "compact"
                        ? Qt.AlignLeft : Qt.AlignHCenter
                    Layout.topMargin: Theme.spaceXS
                    implicitWidth: Math.min(parent.width, glanceRow.implicitWidth + Theme.spaceM * 2)
                    implicitHeight: glanceRow.implicitHeight + Theme.spaceS + 4
                    radius: Theme.radiusPill
                    color: Qt.alpha(Theme.surfaceHigh, 0.70)
                    border.width: 1
                    border.color: Qt.alpha(Theme.text, 0.11)
                    scale: glanceTap.pressed ? 0.96 : 1.0
                    Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

                    RowLayout {
                        id: glanceRow
                        anchors.centerIn: parent
                        spacing: Theme.spaceS

                        MaterialIcon {
                            name: SystemControl.airplaneMode ? "flight" : (Network.wifiConnected ? "wifi" : "wifi_off")
                            size: Theme.fontBody - 2
                            color: SystemControl.airplaneMode ? Theme.warn : (Network.wifiConnected ? ShellPrefs.accentColor : Theme.textFaint)
                        }

                        Text {
                            text: home.netLabel
                            color: Theme.text
                            font.pixelSize: Theme.fontStatus
                            font.family: Theme.fontFamily
                            font.weight: Font.Medium
                        }

                        Rectangle { width: 1; height: 16; color: Qt.alpha(Theme.text, 0.18) }

                        MaterialIcon {
                            name: "memory"
                            size: Theme.fontBody - 2
                            color: ShellPrefs.accentColor
                        }

                        Text {
                            text: SystemControl.cpuPercent + "% CPU"
                            color: Theme.text
                            font.pixelSize: Theme.fontStatus
                            font.family: Theme.fontFamily
                            font.weight: Font.Medium
                        }

                        Rectangle { width: 1; height: 16; color: Qt.alpha(Theme.text, 0.18) }

                        MaterialIcon {
                            name: SystemControl.dndEnabled ? "do_not_disturb_on" : "bolt"
                            size: Theme.fontBody - 2
                            color: SystemControl.dndEnabled ? Theme.warn : Theme.textDim
                        }

                        Text {
                            text: home.powerLabel
                            color: Theme.text
                            font.pixelSize: Theme.fontStatus
                            font.family: Theme.fontFamily
                            font.weight: Font.Medium
                        }
                    }

                    TapHandler {
                        id: glanceTap
                        onTapped: {
                            if (!settingsProc.running)
                                settingsProc.running = true
                        }
                    }
                }
            }
        }

        // ---- Home Utility Apps Grid (2x4) ----
        property var homeApps: []
        property var dockApps: []

        function buildApps() {
            homeApps = [
                DesktopEntries.byId("org.kde.index")   || DesktopEntries.byId("org.kde.dolphin") || ({ id: "a21s-files", name: "Files", materialIcon: "folder" }),
                DesktopEntries.byId("org.kde.merkuro.calendar") || DesktopEntries.byId("org.kde.merkuro") || DesktopEntries.byId("org.kde.calindori") || ({ id: "org.kde.calindori", name: "Calendar", materialIcon: "calendar_month" }),
                DesktopEntries.byId("a21s-clock") || DesktopEntries.byId("org.kde.kclock") || ({ id: "a21s-clock", name: "Clock", materialIcon: "schedule" }),
                DesktopEntries.byId("a21s-calculator") || DesktopEntries.byId("org.kde.kalk") || ({ id: "a21s-calculator", name: "Calculator", materialIcon: "calculate" }),
                DesktopEntries.byId("org.kde.kweather") || ({ id: "org.kde.kweather", name: "Weather", materialIcon: "wb_sunny" }),
                DesktopEntries.byId("a21s-notes") || DesktopEntries.byId("org.kde.marknote") || ({ id: "a21s-notes", name: "Notes", materialIcon: "edit_note" }),
                DesktopEntries.byId("a21s-photos") || DesktopEntries.byId("org.kde.pix")     || DesktopEntries.byId("org.kde.koko") || ({ id: "a21s-gallery", name: "Gallery", materialIcon: "photo_library" }),
                DesktopEntries.byId("a21s-terminal") || DesktopEntries.byId("org.kde.qmlkonsole") || ({ id: "org.kde.qmlkonsole", name: "Terminal", materialIcon: "terminal" })
            ];
            dockApps = [
                DesktopEntries.byId("a21s-dialer")   || ({ id: "a21s-dialer", name: "Phone", materialIcon: "call" }),
                DesktopEntries.byId("a21s-sms")      || ({ id: "a21s-sms", name: "Messages", materialIcon: "sms" }),
                DesktopEntries.byId("a21s-settings") || ({ id: "a21s-settings", name: "Settings", materialIcon: "settings" }),
                DesktopEntries.byId("a21s-browser")  || ({ id: "a21s-browser", name: "Browser", materialIcon: "language" })
            ];
        }

        Component.onCompleted: buildApps()

        Connections {
            target: DesktopEntries
            function onApplicationsChanged() { homeContent.buildApps(); }
        }

        Grid {
            id: homeGridRow
            readonly property real cellW: Theme.iconApp - 4 + Theme.spaceM
            readonly property real sideMargin: Theme.spaceL
            anchors.horizontalCenter: parent.horizontalCenter
            // Upper third of the free space between the clock and the hero deck: reads as one group
            // with the clock and keeps the labels clear of the wallpaper's bright sun below.
            y: clockBlock.y + clockBlock.height
               + Math.max(Theme.spaceM, (heroDeckSection.y - (clockBlock.y + clockBlock.height) - height) / 3)
            columns: 4
            columnSpacing: Math.max(Theme.spaceS,
                (homeContent.width - 2 * sideMargin - columns * cellW) / (columns - 1))
            rowSpacing: Theme.spaceL

            Repeater {
                model: homeContent.homeApps
                delegate: AppIcon {
                    required property var modelData
                    tileSize: Theme.iconApp - 4
                    width: homeGridRow.cellW
                    height: tileSize + Theme.fontCaption + Theme.spaceS + 4
                    entry: modelData
                    labelVisible: true
                }
            }
        }

        // ---- Interactive 3-Card Hero Pager Deck (Media / System) ----
        Item {
            id: heroDeckSection
            anchors {
                left: parent.left
                right: parent.right
                bottom: searchPill.top
                bottomMargin: Theme.spaceM
            }
            height: 192

            ListView {
                id: heroList
                anchors {
                    top: parent.top
                    left: parent.left
                    right: parent.right
                }
                height: 168
                orientation: ListView.Horizontal
                snapMode: ListView.SnapOneItem
                highlightRangeMode: ListView.StrictlyEnforceRange
                highlightMoveDuration: Theme.animMed
                boundsBehavior: Flickable.StopAtBounds
                clip: true
                model: home.heroDeckModel
                // The deck's first card changes with the media state; always land on the first card.
                onModelChanged: { currentIndex = 0; positionViewAtBeginning() }

                delegate: Item {
                    id: deckDelegate
                    required property string modelData
                    width: heroList.width
                    height: heroList.height

                    // --- Card 0: RabbizOS welcome (shown while no media player is active) ---
                    Rectangle {
                        visible: deckDelegate.modelData === "welcome"
                        anchors {
                            fill: parent
                            leftMargin: Theme.spaceM
                            rightMargin: Theme.spaceM
                        }
                        radius: Theme.radiusL
                        color: Qt.alpha(Theme.card, 0.90)
                        border.width: 1
                        border.color: Qt.alpha(Theme.text, 0.10)
                        clip: true

                        RowLayout {
                            anchors { fill: parent; margins: Theme.spaceM }
                            spacing: Theme.spaceM

                            Image {
                                source: "../Assets/rabbiz-mark.png"
                                Layout.preferredWidth: 96
                                Layout.preferredHeight: 96
                                sourceSize: Qt.size(192, 192)
                                fillMode: Image.PreserveAspectFit
                                smooth: true
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 4
                                Text {
                                    text: "RabbizOS"
                                    color: ShellPrefs.accentColor
                                    font.pixelSize: Theme.fontCaption
                                    font.family: Theme.fontFamily
                                    font.letterSpacing: 2
                                    font.weight: Font.DemiBold
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: home.greeting
                                    color: Theme.text
                                    font.pixelSize: Theme.fontTitle
                                    font.family: Theme.fontFamily
                                    font.weight: Font.DemiBold
                                    elide: Text.ElideRight
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: "Swipe up for all apps"
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontCaption
                                    font.family: Theme.fontFamily
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }

                    // --- Card 1: Now Playing Media Card (MPRIS + Morphing Play/Pause) ---
                    Rectangle {
                        visible: deckDelegate.modelData === "media"
                        anchors {
                            fill: parent
                            leftMargin: Theme.spaceM
                            rightMargin: Theme.spaceM
                        }
                        radius: Theme.radiusL
                        color: Qt.alpha(Theme.card, 0.90)
                        border.width: 1
                        border.color: Media.isPlaying
                            ? Qt.alpha(ShellPrefs.accentColor, 0.45)
                            : Qt.alpha(Theme.text, 0.10)
                        Behavior on border.color { ColorAnimation { duration: Theme.animMed } }
                        clip: true

                        ColumnLayout {
                            anchors { fill: parent; margins: Theme.spaceM }
                            spacing: Theme.spaceS

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceM

                                // Album art / animated equalizer badge (tap -> Elisa)
                                Rectangle {
                                    width: 62; height: 62
                                    radius: Theme.radiusM
                                    color: Qt.alpha(ShellPrefs.accentColor, 0.18)
                                    border.width: 1
                                    border.color: Qt.alpha(ShellPrefs.accentColor, 0.35)
                                    clip: true

                                    Image {
                                        id: mediaArt
                                        anchors.fill: parent
                                        source: Media.artUrl
                                        fillMode: Image.PreserveAspectCrop
                                        asynchronous: true
                                        visible: status === Image.Ready
                                    }

                                    MaterialIcon {
                                        anchors.centerIn: parent
                                        visible: mediaArt.status !== Image.Ready && !Media.isPlaying
                                        name: "music_note"
                                        size: 30
                                        color: ShellPrefs.accentColor
                                    }

                                    Row {
                                        anchors.centerIn: parent
                                        visible: mediaArt.status !== Image.Ready && Media.isPlaying
                                        spacing: 4

                                        Repeater {
                                            model: [
                                                { hi: 30, lo: 10, dur: 380 },
                                                { hi: 38, lo: 14, dur: 470 },
                                                { hi: 26, lo: 8,  dur: 340 },
                                                { hi: 34, lo: 12, dur: 520 }
                                            ]
                                            delegate: Item {
                                                required property var modelData
                                                width: 5
                                                height: 38

                                                Rectangle {
                                                    anchors {
                                                        left: parent.left
                                                        right: parent.right
                                                        bottom: parent.bottom
                                                    }
                                                    height: modelData.lo
                                                    radius: 2
                                                    color: ShellPrefs.accentColor

                                                    SequentialAnimation on height {
                                                        running: Media.isPlaying
                                                        loops: Animation.Infinite
                                                        NumberAnimation { to: modelData.hi; duration: modelData.dur; easing.type: Easing.InOutSine }
                                                        NumberAnimation { to: modelData.lo; duration: modelData.dur; easing.type: Easing.InOutSine }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    TapHandler {
                                        onTapped: Media.openMusicApp()
                                    }
                                }

                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 2

                                    Text {
                                        text: Media.hasRealPlayer ? "NOW PLAYING" : "MEDIA DECK"
                                        color: ShellPrefs.accentColor
                                        font.pixelSize: Theme.fontStatus - 2
                                        font.family: Theme.fontFamily
                                        font.weight: Font.DemiBold
                                        font.letterSpacing: 1.4
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        text: Media.title
                                        color: Theme.text
                                        font.pixelSize: Theme.fontBody
                                        font.family: Theme.fontFamily
                                        font.weight: Font.DemiBold
                                        elide: Text.ElideRight
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        text: Media.artist
                                        color: Theme.textDim
                                        font.pixelSize: Theme.fontStatus
                                        font.family: Theme.fontFamily
                                        elide: Text.ElideRight
                                    }

                                    TapHandler {
                                        onTapped: Media.openMusicApp()
                                    }
                                }

                                // Transport controls with shape-morphing Play/Pause button
                                Row {
                                    spacing: 8

                                    Rectangle {
                                        width: 44; height: 44; radius: 22
                                        color: Theme.surfaceHigh
                                        border.width: 1; border.color: Qt.alpha(Theme.text, 0.08)
                                        scale: prevTap.pressed ? 0.88 : 1.0
                                        Behavior on scale { NumberAnimation { duration: Theme.animFast } }
                                        MaterialIcon {
                                            anchors.centerIn: parent
                                            name: "skip_previous"
                                            size: 24; color: Theme.text
                                        }
                                        TapHandler {
                                            id: prevTap
                                            onTapped: Media.previous()
                                        }
                                    }

                                    Rectangle {
                                        width: 52; height: 52
                                        // Morph circle <-> rounded squircle on playback state
                                        radius: Media.isPlaying ? 15 : 26
                                        color: Media.isPlaying ? ShellPrefs.accentColor : Theme.surfaceHigh
                                        border.width: Media.isPlaying ? 0 : 1
                                        border.color: Qt.alpha(ShellPrefs.accentColor, 0.6)
                                        scale: ppTap.pressed ? 0.90 : 1.0

                                        Behavior on radius { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }
                                        Behavior on color { ColorAnimation { duration: Theme.animFast } }
                                        Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                                        MaterialIcon {
                                            anchors.centerIn: parent
                                            name: Media.isPlaying ? "pause" : "play_arrow"
                                            size: 28
                                            color: Media.isPlaying ? ShellPrefs.onAccentColor : Theme.text
                                        }
                                        TapHandler {
                                            id: ppTap
                                            onTapped: Media.togglePlaying()
                                        }
                                    }

                                    Rectangle {
                                        width: 44; height: 44; radius: 22
                                        color: Theme.surfaceHigh
                                        border.width: 1; border.color: Qt.alpha(Theme.text, 0.08)
                                        scale: nextTap.pressed ? 0.88 : 1.0
                                        Behavior on scale { NumberAnimation { duration: Theme.animFast } }
                                        MaterialIcon {
                                            anchors.centerIn: parent
                                            name: "skip_next"
                                            size: 24; color: Theme.text
                                        }
                                        TapHandler {
                                            id: nextTap
                                            onTapped: Media.next()
                                        }
                                    }
                                }
                            }

                            // Playback progress track
                            Rectangle {
                                Layout.fillWidth: true
                                height: 6
                                radius: 3
                                color: Theme.surfaceHigh

                                Rectangle {
                                    anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
                                    width: Math.max(6, parent.width * Media.progress)
                                    radius: 3
                                    color: ShellPrefs.accentColor
                                    Behavior on width { NumberAnimation { duration: Theme.animFast } }
                                }
                            }
                        }
                    }

                    // --- Card 2: System Telemetry & Boost ---
                    Rectangle {
                        visible: deckDelegate.modelData === "system"
                        anchors {
                            fill: parent
                            leftMargin: Theme.spaceM
                            rightMargin: Theme.spaceM
                        }
                        radius: Theme.radiusL
                        color: Qt.alpha(Theme.card, 0.90)
                        border.width: 1
                        border.color: Qt.alpha(Theme.text, 0.10)
                        clip: true

                        ColumnLayout {
                            anchors { fill: parent; margins: Theme.spaceM }
                            spacing: Theme.spaceS

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceS

                                MaterialIcon {
                                    name: "speed"
                                    size: 22
                                    color: ShellPrefs.accentColor
                                }
                                Text {
                                    text: "SYSTEM TELEMETRY"
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontStatus - 1
                                    font.family: Theme.fontFamily
                                    font.weight: Font.DemiBold
                                    font.letterSpacing: 1.3
                                }
                                Item { Layout.fillWidth: true }
                                Text {
                                    text: SystemControl.cpuFreqMhz > 0 ? (SystemControl.cpuFreqMhz + " MHz") : ""
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontStatus - 2
                                    font.family: Theme.fontFamily
                                }
                                Rectangle {
                                    readonly property bool isPerf: SystemControl.cpuGovernor === "performance"
                                    implicitWidth: govLabel.implicitWidth + 20
                                    implicitHeight: 30
                                    radius: isPerf ? 15 : 8
                                    color: isPerf ? ShellPrefs.accentColor : Theme.surfaceHigh
                                    border.width: isPerf ? 0 : 1
                                    border.color: Qt.alpha(Theme.text, 0.14)
                                    scale: govTap.pressed ? 0.92 : 1.0

                                    Behavior on radius { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }
                                    Behavior on color { ColorAnimation { duration: Theme.animFast } }
                                    Behavior on scale { NumberAnimation { duration: Theme.animFast } }

                                    Text {
                                        id: govLabel
                                        anchors.centerIn: parent
                                        text: SystemControl.cpuGovernor.toUpperCase()
                                        color: parent.isPerf ? ShellPrefs.onAccentColor : Theme.text
                                        font.pixelSize: Theme.fontStatus - 3
                                        font.family: Theme.fontFamily
                                        font.weight: Font.Bold
                                    }
                                    TapHandler {
                                        id: govTap
                                        onTapped: SystemControl.toggleGovernor()
                                    }
                                }
                            }

                            // CPU bar
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceS
                                Text {
                                    Layout.preferredWidth: 52
                                    text: "CPU"
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontStatus - 1
                                    font.family: Theme.fontFamily
                                }
                                Rectangle {
                                    Layout.fillWidth: true
                                    height: 8; radius: 4
                                    color: Theme.surfaceHigh
                                    Rectangle {
                                        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
                                        width: Math.max(8, parent.width * (SystemControl.cpuPercent / 100.0))
                                        radius: 4
                                        color: SystemControl.cpuPercent > 80 ? Theme.warn : ShellPrefs.accentColor
                                        Behavior on width { NumberAnimation { duration: Theme.animMed; easing.type: Theme.animEase } }
                                    }
                                }
                                Text {
                                    Layout.preferredWidth: 52
                                    horizontalAlignment: Text.AlignRight
                                    text: SystemControl.cpuPercent + "%"
                                    color: Theme.text
                                    font.pixelSize: Theme.fontStatus - 1
                                    font.family: Theme.fontFamily
                                    font.weight: Font.DemiBold
                                }
                            }

                            // RAM & Disk row
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: Theme.spaceS
                                Text {
                                    Layout.preferredWidth: 52
                                    text: "RAM"
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontStatus - 1
                                    font.family: Theme.fontFamily
                                }
                                Rectangle {
                                    Layout.fillWidth: true
                                    height: 8; radius: 4
                                    color: Theme.surfaceHigh
                                    Rectangle {
                                        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
                                        width: Math.max(8, parent.width * (SystemControl.memPercent / 100.0))
                                        radius: 4
                                        color: Theme.success
                                        Behavior on width { NumberAnimation { duration: Theme.animMed; easing.type: Theme.animEase } }
                                    }
                                }
                                Text {
                                    horizontalAlignment: Text.AlignRight
                                    text: SystemControl.memPercent + "% · Free " + SystemControl.diskFreeText
                                    color: Theme.textDim
                                    font.pixelSize: Theme.fontStatus - 2
                                    font.family: Theme.fontFamily
                                }
                            }
                        }
                    }
                }
            }

            // Morphing pill pagination dots
            Row {
                anchors {
                    horizontalCenter: parent.horizontalCenter
                    bottom: parent.bottom
                }
                spacing: 8

                Repeater {
                    model: heroList.count
                    delegate: Rectangle {
                        required property int index
                        readonly property bool active: heroList.currentIndex === index
                        width: active ? 26 : 8
                        height: 8
                        radius: 4
                        color: active ? ShellPrefs.accentColor : Qt.alpha(Theme.text, 0.28)

                        Behavior on width { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack } }
                        Behavior on color { ColorAnimation { duration: Theme.animFast } }

                        TapHandler {
                            onTapped: heroList.currentIndex = index
                        }
                    }
                }
            }
        }

        // ---- Search Pill (morphs upward into AppDrawer on swipe-up) ----
        Rectangle {
            id: searchPill
            anchors {
                left: parent.left
                right: parent.right
                bottom: dockContainer.top
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
                bottomMargin: Theme.spaceM
            }
            height: Theme.iconM + Theme.spaceS + 4
            radius: Theme.radiusPill
            color: Qt.alpha(Theme.surfaceHigh, 0.72)
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.11)
            opacity: Math.max(0, 1.0 - ShellState.drawerProgress * 1.45)
            scale: (searchTap.pressed ? 0.97 : 1.0) * (1.0 + 0.03 * ShellState.drawerProgress)
            transform: Translate {
                y: -ShellState.drawerProgress * (home.height * 0.42)
            }
            Behavior on scale { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }

            RowLayout {
                anchors {
                    fill: parent
                    leftMargin: Theme.spaceM
                    rightMargin: Theme.spaceM
                }
                spacing: Theme.spaceS

                MaterialIcon {
                    name: "search"
                    size: Theme.fontTitle
                    color: ShellPrefs.accentColor
                }

                Text {
                    Layout.fillWidth: true
                    text: "Search apps, files & settings…"
                    color: Theme.textDim
                    font.pixelSize: Theme.fontCaption
                    font.family: Theme.fontFamily
                }

                MaterialIcon {
                    name: "apps"
                    size: Theme.fontTitle - 2
                    color: Theme.textDim
                }
            }

            TapHandler {
                id: searchTap
                onTapped: ShellState.drawerOpen = true
            }
        }

        // ---- Frosted Glass Dock ----
        BackdropBlur {
            id: dockContainer
            sourceItem: wallpaperScene
            sourceRect: Qt.rect(x, y, width, height)
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
                bottomMargin: Theme.navZoneHeight + Theme.spaceS + 8
            }
            height: Theme.iconApp + Theme.spaceM
            radius: Theme.radiusL + 10
            color: gpu ? Theme.glassBlur : Theme.glass
            border.width: 1
            border.color: Theme.glassBorder

            Row {
                anchors.centerIn: parent
                spacing: Theme.spaceL

                Repeater {
                    model: homeContent.dockApps
                    delegate: AppIcon {
                        required property var modelData
                        tileSize: Theme.iconApp - 8
                        width: tileSize + Theme.spaceS
                        height: tileSize + 8
                        entry: modelData
                        labelVisible: false
                    }
                }
            }
        }
    }

    } // desktopScene (must be nested: PanelWindow reparents its direct children)

    // ---- Sliding App Drawer ----
    AppDrawer {
        id: appDrawer
        anchors.fill: parent
        backdropSource: desktopScene
        open: ShellState.drawerOpen
        onRequestClose: ShellState.drawerOpen = false
    }

    // ---- Material 3 Expressive Container Transform Launch/Close Morph ----
    AppLaunchMorph {
        id: launchMorph
        z: 50
        anchors.fill: parent
    }

    Connections {
        target: ShellState
        function onGoHome() { ShellState.drawerOpen = false; }
    }
}
