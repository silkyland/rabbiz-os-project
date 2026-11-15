import QtQuick
import Quickshell.Services.Notifications as QSNotifs

// a21s-shell notification daemon facade (W6).
//
// Wraps Quickshell's NotificationServer, which implements the
// org.freedesktop.Notifications service for the shell process. The QML type is
// a thin facade over a per-process C++ singleton, so every instance shares the
// same trackedNotifications ObjectModel and the same D-Bus name. It is
// therefore safe for more than one component to instantiate this facade;
// capabilities are configured identically everywhere.
//
// Instantiated in:
//   - StatusBar.qml       -> always alive at boot; owns the daemon even while
//                            the shade is collapsed / lazily loaded.
//   - NotificationCenter  -> feeds the shade's ListView directly.
//
// The local component name intentionally shadows the imported type: inside this
// file the real server is reached via the `QSNotifs` alias.
QtObject {
    id: root

    // ObjectModel<Notification> — usable directly as a view model (single
    // `modelData` role) or as a JS array via `.values`.
    readonly property var trackedNotifications: impl.trackedNotifications
    readonly property int count: impl.trackedNotifications.values.length

    // Notification.id -> Date of arrival. Notification exposes no timestamp in
    // Quickshell 0.3.x, so arrival time is recorded here for "x ago" labels.
    property var receivedAt: ({})

    // Fired for every incoming notification (after it is tracked). Kept as a
    // hook for future popup toasts / banners.
    signal newNotification(var notification)

    // Dismiss every tracked notification (the "Clear all" action).
    function dismissAll() {
        var list = impl.trackedNotifications.values.slice();
        for (var i = 0; i < list.length; ++i)
            list[i].dismiss();
    }

    // Date the notification arrived, or null when unknown (e.g. notifications
    // re-emitted before this facade existed).
    function timeOf(notification) {
        if (!notification)
            return null;
        var t = root.receivedAt[notification.id];
        return t === undefined ? null : t;
    }

    // Short relative label: "", "now", "5m", "3h", "2d".
    function timeAgo(notification) {
        var t = timeOf(notification);
        if (t === null)
            return "";
        var s = Math.max(0, Math.floor((new Date() - t) / 1000));
        if (s < 60)
            return qsTr("now");
        var m = Math.floor(s / 60);
        if (m < 60)
            return m + "m";
        var h = Math.floor(m / 60);
        if (h < 24)
            return h + "h";
        return Math.floor(h / 24) + "d";
    }

    // `var` because the QS type is exported at version 0.0 with no static
    // QML base class; QtObject as property type rejects it at compile time.
    property var impl: QSNotifs.NotificationServer {
        keepOnReload: true
        bodySupported: true
        imageSupported: true
        actionsSupported: true
        actionIconsSupported: true
        persistenceSupported: true

        // `tracked` must be set inside the handler or the server discards the
        // notification. Setting it twice (two facades) is a no-op.
        onNotification: notification => {
            notification.tracked = true;
            var times = root.receivedAt;
            times[notification.id] = new Date();
            root.receivedAt = times;
            root.newNotification(notification);
        }
    }
}
