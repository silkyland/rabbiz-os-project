import QtQuick
import QtQuick.Layouts
import Quickshell
import A21s.Theme
import A21s.Services

// Full-screen app drawer, embedded in HomeScreen's background window.
// Slides up over the home page (transform/opacity only — Exynos 850 friendly).
Item {
    id: drawer

    // Bound by the caller to ShellState.drawerOpen.
    property bool open: false

    signal requestClose()
    signal launched()

    // 0 = hidden below screen, 1 = fully open.
    property real progress: open ? 1.0 : 0.0
    visible: open || progress > 0
    opacity: progress

    Behavior on progress {
        NumberAnimation {
            duration: Theme.animMed
            easing.type: Theme.animEase
        }
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
            if (entry.id === "org.kde.dolphin" && DesktopEntries.byId("a21s-files"))
                continue;
            if (query.length > 0 && entry.name.toLowerCase().indexOf(query) === -1)
                continue;
            result.push(entry);
        }
        result.sort(function(a, b) { return a.name.localeCompare(b.name); });
        filteredApps = result;
    }

    onOpenChanged: {
        if (open) {
            rebuildFilter();
            searchInput.forceActiveFocus();
        } else {
            searchInput.focus = false;
            searchInput.text = "";
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
        color: Qt.alpha(Theme.bg, 0.55)
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

        // Top drag handle pill.
        Rectangle {
            id: grabber
            anchors {
                horizontalCenter: parent.horizontalCenter
                top: parent.top
                topMargin: Theme.statusBarHeight + Theme.spaceS
            }
            width: 56
            height: 6
            radius: Theme.radiusPill
            color: Qt.alpha(Theme.textDim, 0.45)
        }

        // Search field.
        Rectangle {
            id: searchBar
            anchors {
                top: grabber.bottom
                topMargin: Theme.spaceM
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

        // Section header.
        RowLayout {
            id: sectionHeader
            anchors {
                top: searchBar.bottom
                topMargin: Theme.spaceM
                left: parent.left
                right: parent.right
                leftMargin: Theme.spaceL
                rightMargin: Theme.spaceL
            }

            Text {
                text: searchInput.text.trim().length > 0 ? "RESULTS" : "ALL APPS"
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
            cellHeight: Theme.iconApp + Theme.fontCaption * 2 + Theme.spaceM
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
