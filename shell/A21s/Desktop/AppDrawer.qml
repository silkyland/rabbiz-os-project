import QtQuick
import QtQuick.Layouts
import Quickshell
import A21s.Theme
import A21s.Services
import A21s.Widgets

// Full-screen app drawer embedded in HomeScreen.
// Shares ShellState.drawerProgress (0..1) so interactive finger drag,
// home parallax scale morph, search-pill flight, category-pill sliding morph,
// and pull-down-to-close all stay 100% synchronized.
Item {
    id: drawer

    // Bound by the caller to ShellState.drawerOpen.
    property bool open: false
    property string activeCategory: "all"

    signal requestClose()
    signal launched()

    readonly property real progress: ShellState.drawerProgress
    visible: open || progress > 0.005
    opacity: Math.max(0, Math.min(1, progress * 1.15))

    NumberAnimation {
        id: snapAnim
        target: ShellState
        property: "drawerProgress"
        duration: Theme.animMed
        easing.type: Easing.OutQuint
    }

    function snapTo(targetVal) {
        snapAnim.stop()
        snapAnim.from = ShellState.drawerProgress
        snapAnim.to = targetVal
        snapAnim.start()
    }

    readonly property var categories: [
        { id: "all",    label: "All" },
        { id: "system", label: "System" },
        { id: "apps",   label: "Apps" },
        { id: "media",  label: "Media" },
        { id: "dev",    label: "Dev" }
    ]

    function categoryForEntry(entry) {
        var eid = String(entry.id || "").toLowerCase()
        var ename = String(entry.name || "").toLowerCase()
        if (eid === "a21s-settings" || eid.indexOf("dolphin") >= 0 || eid === "org.kde.index" || eid === "org.kde.mobile.plasmasettings"
                || eid.indexOf("gparted") >= 0 || eid.indexOf("htop") >= 0
                || eid.indexOf("btop") >= 0 || eid.indexOf("nm-") >= 0
                || ename.indexOf("settings") >= 0 || ename.indexOf("file") >= 0)
            return "system"
        if (eid.indexOf("steam") >= 0 || eid.indexOf("elisa") >= 0
                || eid.indexOf("spotify") >= 0
                || eid.indexOf("gallery") >= 0 || eid.indexOf("koko") >= 0
                || eid.indexOf("mpv") >= 0 || eid.indexOf("vlc") >= 0
                || eid.indexOf("krecorder") >= 0 || ename.indexOf("music") >= 0
                || ename.indexOf("game") >= 0 || ename.indexOf("recorder") >= 0)
            return "media"
        if (eid.indexOf("konsole") >= 0 || eid.indexOf("terminal") >= 0
                || eid.indexOf("code") >= 0 || eid.indexOf("vim") >= 0
                || eid.indexOf("nvim") >= 0 || ename.indexOf("terminal") >= 0
                || ename.indexOf("console") >= 0)
            return "dev"
        return "apps"
    }

    property var filteredApps: []

    function rebuildFilter() {
        var query = searchInput.text.trim().toLowerCase();
        var source = DesktopEntries.applications ? DesktopEntries.applications.values : [];
        var result = [];
        for (var i = 0; i < source.length; ++i) {
            var entry = source[i];
            if (!entry || entry.noDisplay)
                continue;
            var eid = String(entry.id || "");
            if (eid.indexOf("kcm_") === 0 || eid.indexOf("-geo-handler") >= 0
                    || eid === "org.kde.kiod6" || eid === "ktelnetservice6"
                    || eid === "org.kde.knewstuff-dialog6" || eid === "org.kde.ksecretd"
                    || eid === "org.kde.kwalletd" || eid === "cmake-gui"
                    || eid === "qv4l2" || eid === "qvidcap"
                    || eid === "org.gnupg.pinentry-qt")
                continue;
            if (drawer.activeCategory !== "all" && drawer.categoryForEntry(entry) !== drawer.activeCategory)
                continue;
            if (query.length > 0 && entry.name.toLowerCase().indexOf(query) === -1)
                continue;
            result.push(entry);
        }
        result.sort(function(a, b) { return a.name.localeCompare(b.name); });
        filteredApps = result;
    }

    onOpenChanged: {
        snapTo(open ? 1.0 : 0.0)
        if (open) {
            rebuildFilter();
            searchInput.forceActiveFocus();
        } else {
            searchInput.focus = false;
            searchInput.text = "";
            activeCategory = "all";
        }
    }

    Component.onCompleted: rebuildFilter()

    Connections {
        target: DesktopEntries
        function onApplicationsChanged() { drawer.rebuildFilter(); }
    }

    // Dimmed backdrop behind the sliding sheet.
    Rectangle {
        anchors.fill: parent
        color: Qt.alpha(Theme.bg, 0.58 * drawer.progress)
    }

    // Sliding panel.
    Item {
        id: panel
        width: drawer.width
        height: drawer.height
        y: Math.round((1.0 - drawer.progress) * drawer.height)

        // Sheet background with rounded top corners & subtle top border.
        Rectangle {
            anchors {
                fill: parent
                topMargin: Theme.statusBarHeight
            }
            radius: Theme.radiusL
            color: Qt.alpha(Theme.surface, 0.96)
            border.width: 1
            border.color: Qt.alpha(Theme.text, 0.08)

            // Square off bottom corners.
            Rectangle {
                anchors {
                    left: parent.left
                    right: parent.right
                    bottom: parent.bottom
                }
                height: Theme.radiusL
                color: parent.color
            }
        }

        // Backdrop: taps that miss interactive children close the drawer.
        MouseArea {
            anchors.fill: parent
            onClicked: drawer.requestClose()
        }

        // Interactive pull-down header drag zone (grabber + header space).
        Item {
            id: handleZone
            anchors {
                top: parent.top
                topMargin: Theme.statusBarHeight
                left: parent.left
                right: parent.right
            }
            height: 44

            Rectangle {
                id: grabber
                anchors.centerIn: parent
                width: dragHandleArea.pressed ? 76 : 56
                height: 6
                radius: Theme.radiusPill
                color: dragHandleArea.pressed
                    ? ShellPrefs.accentColor
                    : Qt.alpha(Theme.textDim, 0.45)

                Behavior on width { NumberAnimation { duration: Theme.animFast; easing.type: Theme.animEase } }
                Behavior on color { ColorAnimation { duration: Theme.animFast } }
            }

            MouseArea {
                id: dragHandleArea
                anchors.fill: parent
                preventStealing: true
                property real pressY: 0
                property bool dragging: false

                onPressed: (mouse) => {
                    snapAnim.stop()
                    pressY = mouse.y
                    dragging = false
                }
                onPositionChanged: (mouse) => {
                    if (!pressed) return
                    var dy = mouse.y - pressY
                    if (dy > 8)
                        dragging = true
                    if (dragging) {
                        var range = Math.max(200, drawer.height * 0.55)
                        ShellState.drawerProgress = Math.max(0, Math.min(1, 1.0 - dy / range))
                    }
                }
                onReleased: {
                    if (!dragging) {
                        drawer.requestClose()
                        return
                    }
                    if (ShellState.drawerProgress < 0.72) {
                        ShellState.drawerOpen = false
                        drawer.snapTo(0.0)
                    } else {
                        ShellState.drawerOpen = true
                        drawer.snapTo(1.0)
                    }
                }
            }
        }

        // Search field.
        Rectangle {
            id: searchBar
            anchors {
                top: handleZone.bottom
                topMargin: Theme.spaceXS
                left: parent.left
                right: parent.right
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
            }
            height: Theme.iconM + Theme.spaceS * 2
            radius: Theme.radiusPill
            color: Theme.surfaceHigh
            border.width: 1.5
            border.color: searchInput.activeFocus
                ? ShellPrefs.accentColor
                : Qt.alpha(Theme.text, 0.09)

            Behavior on border.color { ColorAnimation { duration: Theme.animFast } }

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
                    color: searchInput.activeFocus ? ShellPrefs.accentColor : Theme.textDim
                }

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    TextInput {
                        id: searchInput
                        anchors.fill: parent
                        color: Theme.text
                        font.pixelSize: Theme.fontBody
                        font.family: Theme.fontFamily
                        verticalAlignment: TextInput.AlignVCenter
                        clip: true
                        onTextChanged: drawer.rebuildFilter()
                        Keys.onEscapePressed: drawer.requestClose()
                    }

                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: searchInput.text.length === 0
                        text: "Search apps…"
                        color: Theme.textFaint
                        font.pixelSize: Theme.fontBody
                        font.family: Theme.fontFamily
                    }
                }

                Rectangle {
                    visible: searchInput.text.length > 0
                    Layout.preferredWidth: Theme.iconS
                    Layout.preferredHeight: Theme.iconS
                    radius: Theme.radiusPill
                    color: Qt.alpha(Theme.text, 0.12)

                    MaterialIcon {
                        anchors.centerIn: parent
                        name: "close"
                        size: Theme.fontStatus + 4
                        color: Theme.text
                    }

                    TapHandler {
                        onTapped: searchInput.text = ""
                    }
                }
            }
        }

        // Morphing Category Filter Pills with animated sliding highlight capsule
        Item {
            id: categoryStrip
            anchors {
                top: searchBar.bottom
                topMargin: Theme.spaceM
                left: parent.left
                right: parent.right
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
            }
            height: 44

            property Item activeChip: null

            function updateActiveChip() {
                for (var i = 0; i < catRow.children.length; ++i) {
                    var ch = catRow.children[i]
                    if (ch && ch.catId === drawer.activeCategory) {
                        categoryStrip.activeChip = ch
                        return
                    }
                }
            }

            Rectangle {
                id: slidingPill
                visible: categoryStrip.activeChip !== null
                y: 0
                x: categoryStrip.activeChip ? categoryStrip.activeChip.x : 0
                width: categoryStrip.activeChip ? categoryStrip.activeChip.width : 64
                height: categoryStrip.height
                radius: Theme.radiusPill
                color: ShellPrefs.accentColor

                Behavior on x { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutQuint } }
                Behavior on width { NumberAnimation { duration: Theme.animMed; easing.type: Easing.OutQuint } }
                Behavior on color { ColorAnimation { duration: Theme.animFast } }
            }

            Row {
                id: catRow
                anchors.fill: parent
                spacing: 6

                Repeater {
                    model: drawer.categories
                    delegate: Item {
                        required property var modelData
                        readonly property string catId: modelData.id
                        readonly property bool isSelected: drawer.activeCategory === catId

                        width: Math.floor((catRow.width - (drawer.categories.length - 1) * catRow.spacing) / drawer.categories.length)
                        height: catRow.height

                        Component.onCompleted: {
                            if (isSelected)
                                categoryStrip.activeChip = this
                        }

                        onIsSelectedChanged: {
                            if (isSelected)
                                categoryStrip.activeChip = this
                        }

                        Rectangle {
                            anchors.fill: parent
                            radius: Theme.radiusPill
                            color: Qt.alpha(Theme.surfaceHigh, isSelected ? 0.0 : 0.7)
                            border.width: isSelected ? 0 : 1
                            border.color: Qt.alpha(Theme.text, 0.06)
                        }

                        Text {
                            anchors.centerIn: parent
                            text: modelData.label
                            color: isSelected ? ShellPrefs.onAccentColor : Theme.textDim
                            font.pixelSize: Theme.fontStatus
                            font.family: Theme.fontFamily
                            font.weight: isSelected ? Font.DemiBold : Font.Medium
                            Behavior on color { ColorAnimation { duration: Theme.animFast } }
                        }

                        TapHandler {
                            onTapped: {
                                drawer.activeCategory = catId
                                categoryStrip.updateActiveChip()
                                drawer.rebuildFilter()
                            }
                        }
                    }
                }
            }
        }

        // Section header.
        RowLayout {
            id: sectionHeader
            anchors {
                top: categoryStrip.bottom
                topMargin: Theme.spaceM
                left: parent.left
                right: parent.right
                leftMargin: Theme.spaceL
                rightMargin: Theme.spaceL
            }

            Text {
                text: searchInput.text.trim().length > 0
                    ? "RESULTS"
                    : (drawer.activeCategory === "all" ? "ALL APPS" : drawer.activeCategory.toUpperCase())
                color: Theme.textDim
                font.pixelSize: Theme.fontStatus
                font.family: Theme.fontFamily
                font.weight: Font.DemiBold
                font.letterSpacing: 1.5
            }

            Item { Layout.fillWidth: true }

            Rectangle {
                implicitWidth: countTxt.implicitWidth + Theme.spaceS * 2
                implicitHeight: countTxt.implicitHeight + 6
                radius: Theme.radiusPill
                color: Qt.alpha(ShellPrefs.accentColor, 0.16)

                Text {
                    id: countTxt
                    anchors.centerIn: parent
                    text: drawer.filteredApps.length
                    color: ShellPrefs.accentColor
                    font.pixelSize: Theme.fontStatus
                    font.family: Theme.fontFamily
                    font.weight: Font.DemiBold
                }
            }
        }

        // App grid — 4 columns, vertically scrollable.
        GridView {
            id: grid
            anchors {
                top: sectionHeader.bottom
                topMargin: Theme.spaceM
                left: parent.left
                right: parent.right
                bottom: parent.bottom
                leftMargin: Theme.spaceS
                rightMargin: Theme.spaceS
                bottomMargin: Theme.navZoneHeight
            }
            clip: true
            cellWidth: Math.floor(width / 4)
            cellHeight: Theme.iconApp + Theme.fontCaption * 2 + Theme.spaceM + 8
            model: drawer.filteredApps

            delegate: AppIcon {
                required property var modelData
                width: grid.cellWidth
                height: grid.cellHeight
                entry: modelData
                onLaunched: drawer.requestClose()
            }

            // Pull down at the top to dismiss (Android-style).
            onContentYChanged: {
                if (contentY < -Theme.iconApp)
                    drawer.requestClose();
            }
        }

        Text {
            anchors.centerIn: grid
            visible: grid.count === 0
            text: "No matching apps"
            color: Theme.textFaint
            font.pixelSize: Theme.fontBody
            font.family: Theme.fontFamily
        }
    }
}
