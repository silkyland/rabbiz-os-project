#!/usr/bin/env python3
"""Offscreen preview of the shade's QML pages (no device, no compositor).

Renders `A21s.Panels.QuickSettings` and `A21s.Panels.NotificationCenter` with
the real QML at the phone's 720x1600 frame, so layout/theme work can be checked
on a host. `Quickshell` is a Wayland layer-shell + D-Bus library, so the handful
of types these pages touch (Process, FileView, SystemClock, the notification
server, UPower, MPRIS) are stubbed with inert QML equivalents; the stubs and the
fixture notifications are written to a temp dir at run time.

Requirements: `pip install pyside6-essentials` (Qt 6.12 verified).
Usage:
    python3 tools/shade-preview/preview.py [--out DIR] [--light]

Writes <out>/controls.png and <out>/notifications.png (plus -light.png variants
with --light) and prints content height next to page height -- that pair is what
tells you whether a page still fits without scrolling.

Caveats: the status bar is a separate PanelWindow (absent here); macOS window
decoration shaves ~21 px off the frame height (visible in the printed numbers);
"Noto Sans" is not installed on the host, so text metrics are close but not
identical to the device.
"""
import argparse
import os
import pathlib
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
SHELL = REPO / "shell"
WALLPAPER = SHELL / "assets" / "wallpapers" / "aurora.png"

# relpath -> file contents, written into a temp import path
STUBS = {
    "Quickshell/Io/FileView.qml": "import QtQuick\nQtObject {\n    property string path: \"\"\n    property bool watchChanges: false\n    property bool blockLoading: false\n    property bool printErrors: false\n    property bool atomic: false\n    function text() { return \"\" }\n    function reload() {}\n    signal loaded()\n    signal fileChanged()\n    signal textChanged()\n}\n",
    "Quickshell/Io/IpcHandler.qml": "import QtQuick\nQtObject {\n    property string target: \"\"\n}\n",
    "Quickshell/Io/Process.qml": "import QtQuick\n// Preview stub: records the command, never runs it.\nQtObject {\n    property var command: []\n    property bool running: false\n    property var environment: ({})\n    property string workingDirectory: \"\"\n    property bool stdinEnabled: false\n    property var stdout: null\n    property var stderr: null\n    signal started()\n    signal exited(int code, int status)\n}\n",
    "Quickshell/Io/StdioCollector.qml": "import QtQuick\nQtObject {\n    property string text: \"\"\n    signal streamFinished()\n}\n",
    "Quickshell/Io/qmldir": "module Quickshell.Io\nProcess 1.0 Process.qml\nStdioCollector 1.0 StdioCollector.qml\nFileView 1.0 FileView.qml\nIpcHandler 1.0 IpcHandler.qml\n",
    "Quickshell/Quickshell.qml": "pragma Singleton\nimport QtQuick\n// Host-side preview stub: the real Quickshell exposes these as native helpers.\nQtObject {\n    function env(name) { return __env[name] !== undefined ? __env[name] : \"\" }\n    function iconPath(name) { return \"\" }\n    function execDetached(argv) { return 0 }\n    property var __env: ({})\n}\n",
    "Quickshell/Scope.qml": "import QtQuick\nItem {}\n",
    "Quickshell/Services/Mpris/Mpris.qml": "pragma Singleton\nimport QtQuick\nQtObject {\n    property var players: ({ values: [] })\n}\n",
    "Quickshell/Services/Mpris/qmldir": "module Quickshell.Services.Mpris\nsingleton Mpris 1.0 Mpris.qml\n",
    "Quickshell/Services/Notifications/NotificationDemo.js": ".pragma library\n// Preview fixture: notifications in arrival order (oldest first), shaped like\n// Quickshell's Notification objects (the Panels facade reads these fields).\nfunction list() {\n    return [\n        { id: 1, appName: \"Messenger\", appIcon: \"\", summary: \"Winura · 2 photos\",\n          body: \"Sent 2 photos in Rabbiz OS build\",\n          actions: [{ text: \"Reply\" }, { text: \"Open\" }] },\n        { id: 2, appName: \"Chatty\", appIcon: \"\", summary: \"KEX delivery\",\n          body: \"Parcel ISX315632KRDP is out for delivery today.\", actions: [] },\n        { id: 8, appName: \"Messenger\", appIcon: \"\", summary: \"Winura · 2 photos\",\n          body: \"Also sent a Reels clip\", actions: [] },\n        { id: 3, appName: \"Device share\", appIcon: \"\", summary: \"Device share · 2 devices\",\n          body: \"Sharing as Wi-Fi hotspot — 1 client joined.\", actions: [] },\n        { id: 4, appName: \"Bluetooth\", appIcon: \"\", summary: \"Galaxy Watch4 connected\",\n          body: \"Battery 62% · media controls available.\", actions: [] },\n        { id: 5, appName: \"Random Tool\", appIcon: \"\", summary: \"Index finished\",\n          body: \"12 files indexed in 1.4 s.\", actions: [] },\n        { id: 6, appName: \"Shopee\", appIcon: \"\", summary: \"BevFood — ฿299 Bundle\",\n          body: \"เก็บคูปองชาบูชิคุ้มทุกพุธ 299+ ได้ที่ BevFood เท่านั้น\", actions: [] },\n        { id: 7, appName: \"KBank\", appIcon: \"\", summary: \"KBank Live — Solar loan\",\n          body: \"ลดค่าไฟให้บ้าน เพียงติด Solar Rooftop กับ ttb bank\", actions: [] }\n    ];\n}\n",
    "Quickshell/Services/Notifications/NotificationServer.qml": "import QtQuick\nimport Quickshell\nimport \"NotificationDemo.js\" as Demo\n// Preview stub: a tracked-notification model the host can fill with fixtures.\nQtObject {\n    // Preview harness always previews with fixtures.\n    property var trackedNotifications: ({ values: Demo.list() })\n    property bool keepOnReload: true\n    property bool bodySupported: true\n    property bool imageSupported: true\n    property bool actionsSupported: true\n    property bool actionIconsSupported: true\n    property bool persistenceSupported: true\n    signal notification(var n)\n}\n",
    "Quickshell/Services/Notifications/qmldir": "module Quickshell.Services.Notifications\nNotificationServer 1.0 NotificationServer.qml\n",
    "Quickshell/Services/UPower/UPower.qml": "pragma Singleton\nimport QtQuick\nQtObject {\n    property var displayDevice: null\n    property bool onBattery: false\n}\n",
    "Quickshell/Services/UPower/UpowerDevice.qml": "import QtQuick\nQtObject { property string state: \"\"; property bool isPresent: false; property bool isLaptopBattery: false\n           property bool ready: false; property real percentage: 0 }\n",
    "Quickshell/Services/UPower/qmldir": "module Quickshell.Services.UPower\nsingleton UPower 1.0 UPower.qml\nUpowerDevice 1.0 UpowerDevice.qml\n",
    "Quickshell/Singleton.qml": "import QtQuick\n// Preview stub of Quickshell's per-screen singleton base type.\nQtObject {\n    property bool enabled: true\n    default property list<QtObject> data\n}\n",
    "Quickshell/SystemClock.qml": "import QtQuick\n// Preview stub of Quickshell.SystemClock (Minutes is an enum on the type).\nQtObject {\n    enum Precision { Minutes, Seconds }\n    property int precision: 0\n    property date date: new Date()\n}\n",
    "Quickshell/Wayland/PanelWindow.qml": "import QtQuick\nItem {\n    property var screen: null\n    property int exclusionMode: 0\n    property var mask: null\n    property color color: \"transparent\"\n    signal clicked()\n}\n",
    "Quickshell/Wayland/Region.qml": "import QtQuick\nQtObject {\n    property var item: null\n}\n",
    "Quickshell/Wayland/ToplevelManager.qml": "import QtQuick\nQtObject {\n    property var toplevels: ({ values: [] })\n}\n",
    "Quickshell/Wayland/qmldir": "module Quickshell.Wayland\nToplevelManager 1.0 ToplevelManager.qml\nPanelWindow 1.0 PanelWindow.qml\nRegion 1.0 Region.qml\n",
    "Quickshell/qmldir": "module Quickshell\nsingleton Quickshell 1.0 Quickshell.qml\nSystemClock 1.0 SystemClock.qml\nScope 1.0 Scope.qml\nSingleton 1.0 Singleton.qml\n"
}

PREVIEW_QML = r"""import QtQuick
import A21s.Theme
import A21s.Services
import A21s.Panels
import A21s.Widgets

// Offscreen preview host: reproduces the shade's content frame (720x1600, the
// status bar strip and the 20 px gutter) so Panels can be rendered without a
// Wayland compositor. Not shipped — lives in /tmp with stubbed Quickshell types.
Item {
    id: host
    width: 720
    height: 1600

    property string page: "controls"
    property bool lightTheme: false

    Binding { target: Theme; property: "light"; value: host.lightTheme }

    Image {
        anchors.fill: parent
        source: "file://@WALLPAPER@"
        fillMode: Image.PreserveAspectCrop
    }
    Rectangle { anchors.fill: parent; color: host.lightTheme ? "#66F2F2F5" : "#66000000" }

    // Mirrors Shade.qml: 20 px gutter, dots row, then the page area.
    Item {
        id: frame
        anchors.fill: parent
        anchors.leftMargin: Theme.spaceM
        anchors.rightMargin: Theme.spaceM
        anchors.topMargin: Theme.statusBarHeight + Theme.spaceS
        anchors.bottomMargin: Theme.spaceS

        Column {
            anchors.fill: parent
            spacing: Theme.spaceS

            Item { width: 1; height: 28 }   // page-dots row

            Item {
                id: pageArea
                width: frame.width
                height: frame.height - 28 - Theme.spaceS

                QuickSettings {
                    id: qs
                    anchors.fill: parent
                    visible: host.page === "controls"
                }

                NotificationCenter {
                    id: nc
                    anchors.fill: parent
                    visible: host.page === "notifications"
                }
            }
        }
    }

    readonly property real qsContentHeight: qs.implicitHeight
    readonly property real pageHeight: pageArea.height

    // Give the cards plausible "x ago" labels (the facade keys these by id).
    Component.onCompleted: {
        var now = new Date()
        var ages = { 1: 57, 2: 41, 3: 26, 4: 23, 5: 12, 6: 3, 7: 0, 8: 4 }
        var map = {}
        for (var id in ages)
            map[id] = new Date(now - ages[id] * 60000)
        nc.notificationServer.receivedAt = map
    }
}
"""

GRAB = r"""
import os, sys
os.environ.setdefault("QT_QUICK_BACKEND", "software")
from PySide6.QtCore import QUrl, QTimer
from PySide6.QtGui import QGuiApplication
from PySide6.QtQuick import QQuickView

app = QGuiApplication(sys.argv)
view = QQuickView()
view.setResizeMode(QQuickView.SizeRootObjectToView)
for p in sys.argv[2:]:
    view.engine().addImportPath(p)
view.setSource(QUrl.fromLocalFile(sys.argv[1]))
if view.status() == QQuickView.Error:
    for e in view.errors():
        print("QML ERROR:", e.toString())
    sys.exit(1)
view.resize(720, 1600)
view.show()
root = view.rootObject()
if "light" in sys.argv[-1]:
    root.setProperty("lightTheme", True)

def shoot():
    print("  page height:", root.property("pageHeight"),
          "| content:", root.property("qsContentHeight"))
    img = view.grabWindow()
    print("  saved", os.environ["SHADE_PREVIEW_OUT"], img.save(os.environ["SHADE_PREVIEW_OUT"]))
    app.quit()

QTimer.singleShot(700, shoot)
sys.exit(app.exec())
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="/tmp/shade-preview")
    ap.add_argument("--light", action="store_true", help="also render the light theme")
    args = ap.parse_args()

    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="shade-preview-"))
    imports = tmp / "imports"
    for rel, body in STUBS.items():
        dest = imports / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(body)
    qml = tmp / "preview.qml"
    qml.write_text(PREVIEW_QML.replace("@WALLPAPER@", str(WALLPAPER)))
    grab = tmp / "grab.py"
    grab.write_text(GRAB)

    themes = [False, True] if args.light else [False]
    for page in ["controls", "notifications"]:
        for light in themes:
            name = page + ("-light" if light else "") + ".png"
            print("==", name)
            driver = tmp / "run.py"
            driver.write_text(
                "import sys\n"
                "sys.argv = ['grab', %r, %r, %r, %r]\n"
                % (str(qml), str(imports), str(SHELL),
                   (page + ";light") if light else page)
                + "exec(open(%r).read())\n" % str(grab)
            )
            env = dict(os.environ, SHADE_PREVIEW_OUT=str(out / name))
            subprocess.run([sys.executable, str(driver)], env=env, check=False)
    print("\npreviews in", out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
