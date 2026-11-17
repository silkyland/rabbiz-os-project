import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Services.UPower
import A21s.Theme
import A21s.Services
import A21s.Widgets
import "NotificationGroups.js" as Groups

// Lock screen surface, owned by PowerMenu's overlay layer.
//
// Design contract: the lock screen speaks the *shade's* language (One UI 7
// sheets), not the home screen's. Everything is reused, not re-drawn:
//   - header: the notification page's clock/date/bell row (Theme.fontClock)
//   - cards: NotificationCard grouped per app, filed under the same
//     CONVERSATIONS / DEVICES / APPS / PROMOTIONS sections as the shade
//   - footer: the controls page's circular "+ label" action buttons
//   - cards: Theme.shadeCard / Theme.shadeLine / Theme.radiusCard
//
// The backdrop is fully opaque (gradient + wallpaper + scrim). The shell draws
// the home screen underneath this layer, and the old translucent backdrop let
// its clock, icons and dock ghost through the lock screen.
//
// The lock screen shows a preview: at most `previewLimit` grouped cards; the
// rest is summarised as "+N more" (the shade lists everything after unlocking).
Item {
    id: root

    signal unlockRequested()
    signal powerMenuRequested()

    property real dragOffsetY: 0
    readonly property real unlockProgress: Math.min(1.0, Math.max(0.0, -root.dragOffsetY / 280.0))
    readonly property int previewLimit: 3
    // Bumped every 30 s so "now"/"5m" ages while the screen is locked.
    property int minuteTick: 0

    // Clock tick: slide/fade the time in when the minute rolls over.
    readonly property int minuteNumber: lockClock.date.getMinutes()
    property real clockTickShift: 0
    property real clockTickOpacity: 1

    onMinuteNumberChanged: tickClock()

    function tickClock() {
        clockTickShift = 12
        clockTickOpacity = 0
        clockTickIn.restart()
        clockTickFade.restart()
    }

    NumberAnimation {
        id: clockTickIn
        target: root
        property: "clockTickShift"
        to: 0
        duration: Theme.animMed
        easing.type: Easing.OutCubic
    }
    NumberAnimation {
        id: clockTickFade
        target: root
        property: "clockTickOpacity"
        to: 1
        duration: Theme.animMed
        easing.type: Easing.OutCubic
    }

    NotificationServer { id: notifs }
    SystemClock { id: lockClock; precision: SystemClock.Minutes }

    Timer {
        interval: 30000
        running: root.visible && notifs.count > 0
        repeat: true
        onTriggered: root.minuteTick++
    }

    // Group the live model exactly like the shade, then cap the preview.
    // Reads notifs.trackedNotifications.values, so it re-evaluates on change.
    function buildPreview() {
        var all = Groups.build(notifs.trackedNotifications.values)
        var budget = root.previewLimit
        var shown = 0
        var total = 0
        var sections = []
        for (var i = 0; i < all.length; ++i) {
            var cards = all[i].cards
            total += cards.length
            var keep = []
            for (var j = 0; j < cards.length && budget > 0; ++j) {
                keep.push(cards[j])
                budget--
            }
            if (keep.length > 0) {
                shown += keep.length
                sections.push({
                    category: all[i].category,
                    title: all[i].title,
                    count: all[i].count,
                    cards: keep
                })
            }
        }
        return { sections: sections, hiddenCards: total - shown }
    }

    readonly property var preview: root.buildPreview()

    // Compact status line values (the system StatusBar is covered by this
    // layer, so the lock owns the only status readout that is visible).
    readonly property bool batteryOk: UPower.displayDevice !== null
        && UPower.displayDevice.ready
        && UPower.displayDevice.isPresent
    readonly property int batteryPct: batteryOk
        ? Math.round(UPower.displayDevice.percentage * 100) : 0
    readonly property bool batteryCharging: batteryOk
        && (!UPower.onBattery
            || UPower.displayDevice.state === UPowerDeviceState.Charging
            || UPower.displayDevice.state === UPowerDeviceState.PendingCharge
            || UPower.displayDevice.state === UPowerDeviceState.FullyCharged)
    readonly property string connectionLabel: SystemControl.airplaneMode
        ? "Airplane mode"
        : (Network.wifiConnected
            ? Network.activeSsid
            : (Telephony.operatorName || "Offline"))

    // Re-arm per open: the swipe offset and keyboard focus are per-open state.
    onVisibleChanged: {
        if (visible) {
            dragOffsetY = 0
            content.forceActiveFocus()
        }
    }

    // ------------------------------------------------------------------
    // Backdrop — opaque, so no home-screen content bleeds through.
    // ------------------------------------------------------------------
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0;  color: ShellPrefs.activeWallpaper.top }
            GradientStop { position: 0.55; color: ShellPrefs.activeWallpaper.mid }
            GradientStop { position: 1.0;  color: ShellPrefs.activeWallpaper.bottom }
        }
    }

    Image {
        anchors.fill: parent
        source: ShellPrefs.activeWallpaper.image || ""
        fillMode: Image.PreserveAspectCrop
        smooth: true
        opacity: 0.62
        visible: status === Image.Ready
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: Qt.alpha("#000000", 0.12) }
            GradientStop { position: 0.5; color: Qt.alpha("#000000", 0.18) }
            GradientStop { position: 1.0; color: Qt.alpha("#000000", 0.46) }
        }
    }

    // ------------------------------------------------------------------
    // Content — follows the finger, fades while unlocking.
    // ------------------------------------------------------------------
    Item {
        id: content
        anchors.fill: parent
        y: Math.min(0, root.dragOffsetY)
        scale: 1.0 - root.unlockProgress * 0.03
        opacity: 1.0 - root.unlockProgress * 0.55
        focus: true

        Keys.onEscapePressed: root.unlockRequested()

        // Swipe-up gesture, beneath the content: cards and buttons keep their
        // own input, empty space (and any pass-through area) unlocks.
        MouseArea {
            anchors.fill: parent
            property real pressY: 0

            onPressed: (mouse) => {
                snapBack.stop()
                pressY = mouse.y
            }
            onPositionChanged: (mouse) => {
                if (pressed)
                    root.dragOffsetY = Math.min(0, mouse.y - pressY)
            }
            onReleased: (mouse) => {
                if (root.dragOffsetY < -130)
                    root.unlockRequested()
                else
                    snapBack.restart()
            }
        }

        NumberAnimation {
            id: snapBack
            target: root
            property: "dragOffsetY"
            to: 0
            duration: Theme.animMed
            easing.type: Easing.OutQuint
        }

        // Status line: one row on the same 20 px grid as the cards.
        RowLayout {
            id: statusLine
            anchors {
                top: parent.top
                topMargin: Theme.spaceM
                left: parent.left
                right: parent.right
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
            }
            spacing: Theme.spaceS

            MaterialIcon {
                name: SystemControl.airplaneMode ? "flight"
                    : (Network.wifiConnected ? "wifi" : "signal_cellular_alt")
                size: Theme.fontStatus
                color: Theme.onWallpaperDim
            }
            Text {
                Layout.fillWidth: true
                text: root.connectionLabel
                color: Theme.onWallpaperDim
                font.pixelSize: Theme.fontStatus
                font.family: Theme.fontFamily
                elide: Text.ElideRight
            }
            Row {
                visible: root.batteryOk
                spacing: Theme.spaceXS

                MaterialIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: root.batteryCharging ? "battery_charging_full"
                        : (root.batteryPct > 80 ? "battery_full"
                        : (root.batteryPct > 35 ? "battery_std" : "battery_alert"))
                    size: Theme.fontStatus
                    color: root.batteryCharging ? Theme.success
                        : (root.batteryPct <= 20 ? Theme.danger : Theme.onWallpaper)
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.batteryPct + "%"
                    color: Theme.onWallpaper
                    font.pixelSize: Theme.fontStatus
                    font.family: Theme.fontFamily
                    font.weight: Font.Medium
                }
            }
        }

        // --------------------------------------------------------------
        // Header — identical shape to the shade's notification page:
        // oversized clock, date on its baseline, round DND button.
        // --------------------------------------------------------------
        RowLayout {
            id: header
            anchors {
                top: statusLine.bottom
                topMargin: Theme.spaceS
                left: parent.left
                right: parent.right
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
            }
            spacing: Theme.spaceS

            Text {
                id: lockClockText
                Layout.alignment: Qt.AlignBottom
                opacity: root.clockTickOpacity
                transform: Translate { y: root.clockTickShift }
                text: Qt.formatTime(lockClock.date, ShellPrefs.use24h ? "HH:mm" : "h:mm")
                color: Theme.onWallpaper
                font.pixelSize: Theme.fontClock
                font.family: Theme.fontFamily
                font.weight: Font.Medium
                font.letterSpacing: -2
            }

            Text {
                Layout.alignment: Qt.AlignBottom
                Layout.bottomMargin: Theme.spaceS + 4
                text: Qt.formatDate(lockClock.date, "ddd d MMM")
                color: Theme.onWallpaperDim
                font.pixelSize: Theme.fontCaption
                font.family: Theme.fontFamily
            }

            Item { Layout.fillWidth: true }

            Rectangle {
                Layout.alignment: Qt.AlignBottom
                width: Theme.shadeControlButton
                height: width
                radius: width / 2
                color: Theme.shadeCard
                border.width: 1
                border.color: SystemControl.dndEnabled
                    ? Qt.alpha(Theme.danger, 0.55) : Theme.shadeLine

                Behavior on border.color { ColorAnimation { duration: Theme.animFast } }

                MaterialIcon {
                    anchors.centerIn: parent
                    name: SystemControl.dndEnabled ? "notifications_off" : "notifications"
                    size: Theme.iconS + 2
                    color: SystemControl.dndEnabled ? Theme.danger : Theme.text
                }
                TapHandler {
                    onTapped: SystemControl.setDndEnabled(!SystemControl.dndEnabled)
                }
            }
        }

        // --------------------------------------------------------------
        // Notification preview — same sections and cards as the shade.
        // --------------------------------------------------------------
        Item {
            id: notifArea
            anchors {
                top: header.bottom
                topMargin: Theme.spaceM
                left: parent.left
                right: parent.right
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
                bottom: dock.top
                bottomMargin: Theme.spaceM
            }
            clip: true

            // No Flickable: the preview is capped at three cards, so it always
            // fits. A Flickable here would take the vertical drag away from the
            // swipe-up unlock (and from the shade's own gesture language).
            Column {
                id: listColumn
                width: parent.width
                spacing: Theme.spaceM

                Repeater {
                    model: root.preview.sections

                    delegate: Column {
                        id: sectionColumn
                        required property var modelData
                        width: listColumn.width
                        spacing: Theme.spaceS

                        RowLayout {
                            width: parent.width
                            spacing: Theme.spaceXS

                            Text {
                                text: sectionColumn.modelData.title.toUpperCase()
                                color: Theme.onWallpaperDim
                                font.pixelSize: Theme.fontMicro
                                font.family: Theme.fontFamily
                                font.letterSpacing: 1.6
                                font.weight: Font.DemiBold
                            }
                            Text {
                                text: String(sectionColumn.modelData.count)
                                color: Theme.onWallpaperFaint
                                font.pixelSize: Theme.fontMicro
                                font.family: Theme.fontFamily
                            }
                            Rectangle {
                                Layout.fillWidth: true
                                Layout.alignment: Qt.AlignVCenter
                                implicitHeight: 1
                                color: Theme.onWallpaperLine
                            }
                        }

                        Repeater {
                            model: sectionColumn.modelData.cards

                            delegate: NotificationCard {
                                width: sectionColumn.width
                                height: implicitHeight
                                server: notifs
                                refreshToken: root.minuteTick
                                actionsEnabled: false
                            }
                        }
                    }
                }

                Text {
                    width: parent.width
                    visible: root.preview.hiddenCards > 0
                    text: "+" + root.preview.hiddenCards + " more"
                    color: Theme.onWallpaperFaint
                    font.pixelSize: Theme.fontStatus
                    font.family: Theme.fontFamily
                }
            }

            // Empty state, centred in the list area like the shade's.
            Column {
                anchors.centerIn: parent
                visible: notifs.count === 0
                spacing: Theme.spaceXS

                MaterialIcon {
                    anchors.horizontalCenter: parent.horizontalCenter
                    name: "notifications_off"
                    size: Theme.iconL
                    color: Theme.onWallpaperFaint
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "No notifications"
                    color: Theme.onWallpaperDim
                    font.pixelSize: Theme.fontCaption
                    font.family: Theme.fontFamily
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "You're all caught up"
                    color: Theme.onWallpaperFaint
                    font.pixelSize: Theme.fontStatus
                    font.family: Theme.fontFamily
                }
            }
        }

        // --------------------------------------------------------------
        // Footer dock — the controls page's language: grouped media card,
        // then one row of circular buttons with labels underneath.
        // --------------------------------------------------------------
        ColumnLayout {
            id: dock
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
                bottomMargin: Theme.navZoneHeight + Theme.spaceM
            }
            spacing: Theme.spaceL

            Rectangle {
                id: mediaCard
                Layout.fillWidth: true
                visible: Media.hasRealPlayer
                implicitHeight: 104
                radius: Theme.radiusCard
                color: Theme.shadeCard
                border.width: 1
                border.color: Media.isPlaying
                    ? Qt.alpha(ShellPrefs.accentColor, 0.45) : Theme.shadeLine

                Behavior on border.color { ColorAnimation { duration: Theme.animFast } }

                RowLayout {
                    anchors {
                        fill: parent
                        margins: Theme.spaceM
                    }
                    spacing: Theme.spaceM

                    Rectangle {
                        Layout.preferredWidth: 56
                        Layout.preferredHeight: 56
                        radius: Theme.radiusM
                        color: Theme.shadeWell
                        clip: true

                        Image {
                            id: lockMediaArt
                            anchors.fill: parent
                            source: Media.artUrl
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            visible: status === Image.Ready
                        }
                        MaterialIcon {
                            anchors.centerIn: parent
                            visible: lockMediaArt.status !== Image.Ready
                            name: "audiotrack"
                            size: Theme.iconS
                            color: Theme.textDim
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0

                        Text {
                            Layout.fillWidth: true
                            text: Media.title
                            color: Media.isPlaying ? Theme.text : Theme.textDim
                            font.pixelSize: Theme.fontCaption + 2
                            font.family: Theme.fontFamily
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                            maximumLineCount: 1
                        }
                        Text {
                            Layout.fillWidth: true
                            text: Media.artist
                            color: Theme.textFaint
                            font.pixelSize: Theme.fontStatus
                            font.family: Theme.fontFamily
                            elide: Text.ElideRight
                            maximumLineCount: 1
                        }

                        TapHandler { onTapped: Media.openMusicApp() }
                    }

                    Row {
                        spacing: Theme.spaceS

                        Rectangle {
                            width: 44
                            height: 44
                            radius: 22
                            color: Theme.shadeWell

                            MaterialIcon {
                                anchors.centerIn: parent
                                name: "skip_previous"
                                size: Theme.fontTitle - 4
                                color: Theme.text
                            }
                            TapHandler { onTapped: Media.previous() }
                        }
                        Rectangle {
                            width: Theme.iconM
                            height: Theme.iconM
                            radius: Media.isPlaying ? Theme.radiusM + 6 : width / 2
                            color: Media.isPlaying ? ShellPrefs.accentColor : Theme.shadeCardOn

                            Behavior on radius {
                                NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutBack }
                            }
                            Behavior on color { ColorAnimation { duration: Theme.animFast } }

                            MaterialIcon {
                                anchors.centerIn: parent
                                name: Media.isPlaying ? "pause" : "play_arrow"
                                size: Theme.fontTitle - 4
                                color: Media.isPlaying ? ShellPrefs.accentContent : Theme.text
                            }
                            TapHandler { onTapped: Media.togglePlaying() }
                        }
                        Rectangle {
                            width: 44
                            height: 44
                            radius: 22
                            color: Theme.shadeWell

                            MaterialIcon {
                                anchors.centerIn: parent
                                name: "skip_next"
                                size: Theme.fontTitle - 4
                                color: Theme.text
                            }
                            TapHandler { onTapped: Media.next() }
                        }
                    }
                }
            }

            RowLayout {
                id: actionRow
                Layout.fillWidth: true
                spacing: 0

                Item { Layout.fillWidth: true }

                // Flashlight
                Column {
                    width: 140
                    spacing: Theme.spaceXS
                    Layout.alignment: Qt.AlignTop

                    Rectangle {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Theme.shadeControlButton
                        height: width
                        radius: width / 2
                        color: SystemControl.flashlightOn ? Theme.text : Theme.shadeCard
                        border.width: SystemControl.flashlightOn ? 0 : 1
                        border.color: Theme.shadeLine
                        scale: torchTap.pressed ? 0.92 : 1.0
                        Behavior on scale {
                            NumberAnimation { duration: Theme.animFast; easing.type: Easing.OutCubic }
                        }

                        MaterialIcon {
                            anchors.centerIn: parent
                            name: SystemControl.flashlightOn ? "flashlight_on" : "flashlight_off"
                            size: Math.round(Theme.shadeControlButton * 0.44)
                            color: SystemControl.flashlightOn ? ShellPrefs.accentInk : Theme.text
                        }
                        TapHandler {
                            id: torchTap
                            onTapped: SystemControl.setFlashlight(!SystemControl.flashlightOn)
                        }
                    }

                    Text {
                        width: parent.width
                        text: "Flashlight"
                        color: Theme.onWallpaperDim
                        font.pixelSize: Theme.fontStatus
                        font.family: Theme.fontFamily
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                    }
                }

                Item { Layout.fillWidth: true }

                // Fingerprint / unlock
                Column {
                    width: 240
                    spacing: Theme.spaceXS
                    Layout.alignment: Qt.AlignTop

                    Timer {
                        id: fpScanHoldTimer
                        interval: 180
                        onTriggered: SystemControl.triggerFingerprintUnlock(true)
                    }

                    Item {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Theme.shadeControlButton
                        height: width

                        Rectangle {
                            id: fpRipple
                            anchors.centerIn: parent
                            width: parent.width
                            height: parent.height
                            radius: width / 2
                            color: "transparent"
                            border.width: 2
                            border.color: SystemControl.fpState === "success"
                                ? Theme.success : ShellPrefs.accentColor
                            scale: SystemControl.fpState === "scanning" ? 1.26
                                 : (SystemControl.fpState === "success" ? 1.36 : 1.0)
                            opacity: SystemControl.fpState === "scanning" ? 0.55 : 0.0
                            Behavior on scale {
                                NumberAnimation { duration: 320; easing.type: Easing.OutCubic }
                            }
                            Behavior on opacity { NumberAnimation { duration: 280 } }
                        }

                        Rectangle {
                            id: fpPad
                            anchors.centerIn: parent
                            width: parent.width
                            height: parent.height
                            radius: width / 2
                            color: SystemControl.fpState === "success" ? Theme.success
                                 : (SystemControl.fpState === "error" ? Theme.danger
                                 : (SystemControl.fpState === "scanning"
                                    ? Qt.alpha(ShellPrefs.accentColor, 0.28)
                                    : Theme.shadeCard))
                            border.width: 2
                            border.color: SystemControl.fpState === "success" ? Theme.success
                                        : (SystemControl.fpState === "error" ? Theme.danger
                                        : Qt.alpha(ShellPrefs.accentColor, 0.55))
                            scale: SystemControl.fpState === "success" ? 1.06
                                 : (fpMouse.pressed ? 0.94 : 1.0)
                            Behavior on scale {
                                NumberAnimation { duration: Theme.animFast; easing.type: Easing.OutBack }
                            }
                            Behavior on color { ColorAnimation { duration: 180 } }
                            Behavior on border.color { ColorAnimation { duration: 180 } }

                            MaterialIcon {
                                anchors.centerIn: parent
                                name: !ShellPrefs.fingerprintEnabled ? "keyboard_arrow_up"
                                    : (SystemControl.fpState === "success" ? "check_circle"
                                    : (SystemControl.fpState === "error" ? "error_outline" : "fingerprint"))
                                size: Math.round(Theme.shadeControlButton * 0.42)
                                color: SystemControl.fpState === "success"
                                        || SystemControl.fpState === "error"
                                    ? "#FFFFFF"
                                    : (SystemControl.fpState === "scanning"
                                        ? ShellPrefs.accentColor : Theme.text)
                            }
                        }

                        MouseArea {
                            id: fpMouse
                            anchors.fill: parent
                            onPressed: {
                                if (!ShellPrefs.fingerprintEnabled)
                                    return
                                if (SystemControl.fpState !== "success") {
                                    SystemControl.fpState = "scanning"
                                    fpScanHoldTimer.restart()
                                }
                            }
                            onReleased: {
                                if (!ShellPrefs.fingerprintEnabled) {
                                    root.unlockRequested()
                                    return
                                }
                                fpScanHoldTimer.stop()
                                if (SystemControl.fpState === "scanning")
                                    SystemControl.triggerFingerprintUnlock(true)
                            }
                            onCanceled: {
                                fpScanHoldTimer.stop()
                                if (SystemControl.fpState === "scanning")
                                    SystemControl.fpState = "idle"
                            }
                        }
                    }

                    Text {
                        id: unlockLabel
                        width: parent.width
                        text: !ShellPrefs.fingerprintEnabled ? "Swipe up to unlock"
                            : (SystemControl.fpState === "success" ? "Unlocked · " + SystemControl.fingerprintChip
                            : (SystemControl.fpState === "error" ? "Fingerprint not recognized"
                            : (SystemControl.fpState === "scanning" ? "Verifying " + SystemControl.fingerprintChip + "…"
                            : "Touch to unlock")))
                        color: SystemControl.fpState === "success" ? Theme.success
                             : (SystemControl.fpState === "error" ? Theme.danger : Theme.onWallpaperDim)
                        font.pixelSize: Theme.fontStatus
                        font.family: Theme.fontFamily
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        Behavior on color { ColorAnimation { duration: Theme.animFast } }
                    }
                }

                Item { Layout.fillWidth: true }

                // Power menu
                Column {
                    width: 140
                    spacing: Theme.spaceXS
                    Layout.alignment: Qt.AlignTop

                    Rectangle {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Theme.shadeControlButton
                        height: width
                        radius: width / 2
                        color: Theme.shadeCard
                        border.width: 1
                        border.color: Theme.shadeLine
                        scale: powerTap.pressed ? 0.92 : 1.0
                        Behavior on scale {
                            NumberAnimation { duration: Theme.animFast; easing.type: Easing.OutCubic }
                        }

                        MaterialIcon {
                            anchors.centerIn: parent
                            name: "power_settings_new"
                            size: Math.round(Theme.shadeControlButton * 0.44)
                            color: Theme.text
                        }
                        TapHandler {
                            id: powerTap
                            onTapped: root.powerMenuRequested()
                        }
                    }

                    Text {
                        width: parent.width
                        text: "Power"
                        color: Theme.onWallpaperDim
                        font.pixelSize: Theme.fontStatus
                        font.family: Theme.fontFamily
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                    }
                }

                Item { Layout.fillWidth: true }
            }
        }

        // Home indicator: the system NavBar sits under this opaque layer, so the
        // lock draws the same slim hint, anchored to the same bottom edge.
        Rectangle {
            anchors {
                horizontalCenter: parent.horizontalCenter
                bottom: parent.bottom
                bottomMargin: Theme.spaceS
            }
            width: Math.round(Theme.navPillWidth * 0.75)
            height: Theme.navPillHeight - 2
            radius: height / 2
            color: Qt.alpha(Theme.onWallpaper, 0.45)
        }
    }
}
