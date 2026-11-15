import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import A21s.Theme
import A21s.Services
import A21s.Components as Components

// Standalone Files manager app — launched by `a21s-files`.
// Edge-swipe back navigates to parent folder (or closes at root).
ApplicationWindow {
    id: win
    visible: true
    width: Screen.width > 0 ? Screen.width : 720
    height: Screen.height > 0 ? Screen.height : 1600
    title: "Files"
    color: Theme.bg
    font.family: Theme.fontFamily

    readonly property string homeDir: Quickshell.env("HOME") || "/home/gamer"
    property string currentDir: homeDir
    property var entries: []

    function openDir(path) {
        currentDir = path
        if (!lsProc.running)
            lsProc.running = true
    }

    function goUp() {
        if (currentDir === "/") {
            win.close()
            return
        }
        var idx = currentDir.lastIndexOf("/")
        var parentPath = idx <= 0 ? "/" : currentDir.slice(0, idx)
        openDir(parentPath)
    }

    function iconFor(item) {
        if (item.isDir) return "folder"
        var n = item.name.toLowerCase()
        if (/\.(png|jpg|jpeg|webp|gif|svg)$/.test(n)) return "image"
        if (/\.(mp3|wav|ogg|flac|m4a)$/.test(n)) return "audio_file"
        if (/\.(mp4|mkv|webm|mov)$/.test(n)) return "movie"
        if (/\.(zip|tar|gz|xz|zst)$/.test(n)) return "folder_zip"
        return "description"
    }

    Process {
        id: lsProc
        command: [
            "sh", "-c",
            "cd \"$1\" 2>/dev/null || exit 1; "
            + "for f in *; do "
            + "  [ -e \"$f\" ] || continue; "
            + "  if [ -d \"$f\" ]; then printf 'D\\t%s\\tFolder\\n' \"$f\"; "
            + "  else sz=$(stat -c '%s' \"$f\" 2>/dev/null || echo 0); printf 'F\\t%s\\t%s B\\n' \"$f\" \"$sz\"; fi; "
            + "done",
            "_", win.currentDir
        ]
        stdout: StdioCollector {
            onStreamFinished: {
                var lines = this.text.split("\n")
                var dirs = []
                var files = []
                for (var i = 0; i < lines.length; ++i) {
                    var line = lines[i].trim()
                    if (!line) continue
                    var parts = line.split("\t")
                    if (parts.length < 3) continue
                    var obj = {
                        isDir: parts[0] === "D",
                        name: parts[1],
                        meta: parts[2]
                    }
                    if (obj.isDir) dirs.push(obj)
                    else files.push(obj)
                }
                win.entries = dirs.concat(files)
            }
        }
    }

    Component.onCompleted: openDir(homeDir)

    ColumnLayout {
        anchors {
            fill: parent
            margins: Theme.spaceM
            bottomMargin: Theme.navZoneHeight + Theme.spaceM
        }
        spacing: Theme.spaceM

        // Header
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceS

            Rectangle {
                visible: win.currentDir !== "/"
                Layout.preferredWidth: Theme.iconM
                Layout.preferredHeight: Theme.iconM
                radius: Theme.radiusPill
                color: Theme.surfaceHigh
                Components.MaterialIcon {
                    anchors.centerIn: parent
                    name: "arrow_back"
                    size: Theme.iconS - 4
                    color: Theme.text
                }
                TapHandler { onTapped: win.goUp() }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Text {
                    text: "Files"
                    color: Theme.text
                    font.pixelSize: Theme.fontTitle
                    font.weight: Font.Medium
                }
                Text {
                    Layout.fillWidth: true
                    text: win.currentDir
                    color: Theme.textDim
                    font.pixelSize: Theme.fontStatus
                    elide: Text.ElideLeft
                }
            }
        }

        // Quick path chips
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceS
            Repeater {
                model: [
                    { label: "Home", path: win.homeDir },
                    { label: "Downloads", path: win.homeDir + "/Downloads" },
                    { label: "Desktop", path: win.homeDir + "/Desktop" },
                    { label: "Root /", path: "/" }
                ]
                delegate: Rectangle {
                    required property var modelData
                    implicitWidth: chipTxt.implicitWidth + Theme.spaceM * 1.5
                    height: 44
                    radius: Theme.radiusPill
                    color: win.currentDir === modelData.path ? ShellPrefs.accentColor : Theme.card
                    border.width: 1
                    border.color: Qt.alpha(Theme.text, 0.08)

                    Text {
                        id: chipTxt
                        anchors.centerIn: parent
                        text: modelData.label
                        color: win.currentDir === modelData.path ? Theme.primaryContent : Theme.text
                        font.pixelSize: Theme.fontStatus
                    }
                    TapHandler { onTapped: win.openDir(modelData.path) }
                }
            }
            Item { Layout.fillWidth: true }
        }

        // File list
        ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            spacing: Theme.spaceXS
            model: win.entries

            delegate: Rectangle {
                required property var modelData
                width: ListView.view.width
                height: 84
                radius: Theme.radiusM
                color: Theme.card
                border.width: 1
                border.color: Qt.alpha(Theme.text, 0.06)

                RowLayout {
                    anchors {
                        fill: parent
                        leftMargin: Theme.spaceM
                        rightMargin: Theme.spaceM
                    }
                    spacing: Theme.spaceM

                    Rectangle {
                        Layout.preferredWidth: Theme.iconM
                        Layout.preferredHeight: Theme.iconM
                        radius: Theme.radiusS
                        color: modelData.isDir
                            ? Qt.alpha(ShellPrefs.accentColor, 0.18)
                            : Qt.alpha(Theme.text, 0.07)

                        Components.MaterialIcon {
                            anchors.centerIn: parent
                            name: win.iconFor(modelData)
                            size: Theme.iconS - 4
                            color: modelData.isDir ? ShellPrefs.accentColor : Theme.textDim
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Text {
                            Layout.fillWidth: true
                            text: modelData.name
                            color: Theme.text
                            font.pixelSize: Theme.fontBody
                            elide: Text.ElideRight
                        }
                        Text {
                            text: modelData.meta
                            color: Theme.textDim
                            font.pixelSize: Theme.fontStatus
                        }
                    }

                    Components.MaterialIcon {
                        visible: modelData.isDir
                        name: "chevron_right"
                        size: Theme.iconS
                        color: Theme.textFaint
                    }
                }

                TapHandler {
                    onTapped: {
                        if (modelData.isDir) {
                            var sep = win.currentDir === "/" ? "" : "/"
                            win.openDir(win.currentDir + sep + modelData.name)
                        }
                    }
                }
            }

            Text {
                anchors.centerIn: parent
                visible: parent.count === 0
                text: "Empty folder"
                color: Theme.textFaint
                font.pixelSize: Theme.fontBody
            }
        }
    }

    Components.AppGestureLayer {
        anchors.fill: parent
        onBackTriggered: win.goUp()
    }
}
