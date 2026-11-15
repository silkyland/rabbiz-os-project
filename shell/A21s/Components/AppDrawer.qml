import QtQuick
import Quickshell
import A21s.Theme

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
        }
    }

    Component.onCompleted: rebuildFilter()

    Connections {
        target: DesktopEntries
        function onApplicationsChanged() { drawer.rebuildFilter(); }
    }

    // Sliding panel.
    Item {
        id: panel
        width: drawer.width
        height: drawer.height
        y: Math.round((1.0 - drawer.progress) * drawer.height)

        Rectangle {
            anchors.fill: parent
            color: Theme.surface
        }

        // Backdrop: taps that miss interactive children close the drawer.
        MouseArea {
            anchors.fill: parent
            onClicked: drawer.requestClose()
        }

        // Search field.
        Rectangle {
            id: searchBar
            anchors {
                top: parent.top
                topMargin: Theme.statusBarHeight + Theme.spaceM
                left: parent.left
                right: parent.right
                leftMargin: Theme.spaceM
                rightMargin: Theme.spaceM
            }
            height: Theme.iconM + Theme.spaceS * 2
            radius: Theme.radiusPill
            color: Theme.surfaceHigh

            TextInput {
                id: searchInput
                anchors {
                    fill: parent
                    leftMargin: Theme.spaceM
                    rightMargin: Theme.spaceM
                }
                color: Theme.text
                font.pixelSize: Theme.fontBody
                font.family: Theme.fontFamily
                verticalAlignment: TextInput.AlignVCenter
                clip: true
                onTextChanged: drawer.rebuildFilter()
                Keys.onEscapePressed: drawer.requestClose()
            }

            Text {
                anchors.centerIn: parent
                visible: searchInput.text.length === 0
                text: "Search apps"
                color: Theme.textFaint
                font.pixelSize: Theme.fontCaption
                font.family: Theme.fontFamily
            }
        }

        // App grid — 4 columns, vertically scrollable.
        GridView {
            id: grid
            anchors {
                top: searchBar.bottom
                topMargin: Theme.spaceM
                left: parent.left
                right: parent.right
                bottom: parent.bottom
                leftMargin: Theme.spaceS
                rightMargin: Theme.spaceS
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
            text: "No apps"
            color: Theme.textFaint
            font.pixelSize: Theme.fontBody
            font.family: Theme.fontFamily
        }
    }
}
