pragma Singleton
import QtQuick
import Qt.labs.folderlistmodel

// Telephony facade shared by Dialer + SMS apps.
// Backend priority: ModemManager (org.freedesktop.ModemManager1 over D-Bus,
// driven by a future privileged helper) -> demo mode.
//
// The A21s modem is an Exynos CP behind the `cpif` driver (Samsung IPC,
// /dev/umts_ipc0 — not a USB/AT modem) and is NOT ported yet (Phase 4/M6 —
// docs/a21s-power-battery-modem-plan.md), so demoMode stays true until a
// modem node appears under /dev AND a real `backend` is plugged in.
//
// PURE QtQuick ONLY — this singleton is also imported by standalone qml6
// apps (apps/dialer) where Quickshell modules do not exist. Detection
// therefore probes /dev with FolderListModel instead of `mmcli -L`.
// A later phase wires `backend` to mmcli / D-Bus — the target call flow is
// documented in apps/dialer/mm-backend.sh.
//
// Contract — do not rename members (extend only):
QtObject {
    id: root

    // ---- frozen contract ----------------------------------------------
    property bool modemAvailable: false
    property bool demoMode: true          // true -> simulated calls/SMS
    property string operatorName: ""      // "No service" / carrier name
    property int signalStrength: -1       // 0-100

    // Call state: "" | "dialing" | "ringing" | "active" | "incoming"
    property string callState: ""
    property string callNumber: ""

    // SMS store: [{id, number, body, incoming, timestamp, demo}]
    property var threads: []

    signal incomingCall(string number)
    signal smsReceived(string number, string body)

    // ---- extension: backend hook (new member; contract untouched) ------
    // A future object exposing
    //   dial(number), answer(), hangup(), sendSms(number, body),
    //   sendDtmf(tone), refresh()
    // can be assigned here. When set AND a modem is detected, calls are
    // delegated instead of simulated. Null -> demo state machine.
    property var backend: null

    // Last DTMF tone sent (debug echo for the in-call keypad).
    property string lastDtmf: ""

    // Test hook: force a ringing incoming call (the Dialer's
    // `--simulate-incoming <num>` flag calls this). Returns false when a
    // call is already in progress.
    function simulateIncomingCall(number) {
        if (root.callState !== "")
            return false;
        root.callNumber = String(number);
        root.callState = "incoming";
        root._ringTimeout.restart();
        root.incomingCall(root.callNumber);
        return true;
    }

    // DTMF tone during an active call. Demo mode only records it in
    // `lastDtmf`; a real backend maps this to `--send-dtmf`.
    function sendDtmf(tone) {
        root.lastDtmf = String(tone);
        if (root.backend !== null && !root.demoMode && root.callState === "active")
            root.backend.sendDtmf(root.lastDtmf);
    }

    // ---- contract functions --------------------------------------------
    // Demo timing: dialing 1.5s -> ringing 2s -> active.
    function dial(number) {
        const n = String(number).trim();
        if (n.length === 0 || root.callState !== "")
            return false;
        if (root.backend !== null && !root.demoMode) {
            root.backend.dial(n);
            return true;
        }
        root.callNumber = n;
        root.callState = "dialing";
        root._dialPhase = 0;
        root._dialTimer.interval = 1500;
        root._dialTimer.restart();
        return true;
    }

    function answer() {
        if (root.callState !== "incoming")
            return;
        root._ringTimeout.stop();
        if (root.backend !== null && !root.demoMode) {
            root.backend.answer();
            return;
        }
        root.callState = "active";
    }

    function hangup() {
        if (root.callState === "")
            return;
        if (root.backend !== null && !root.demoMode) {
            root.backend.hangup();
            return;
        }
        root._dialTimer.stop();
        root._ringTimeout.stop();
        root.callState = "";
        root.callNumber = "";
    }

    function sendSms(number, body) {
        const n = String(number).trim();
        const b = String(body);
        if (n.length === 0 || b.length === 0)
            return;
        if (root.backend !== null && !root.demoMode) {
            root.backend.sendSms(n, b);
            return;
        }
        root._appendSms(n, b, false);
        // Fake peer: echoes the message back shortly after.
        root._smsEchoPeer = n;
        root._smsEchoBody = "Echo: " + b;
        root._smsEcho.restart();
    }

    function refresh() {
        if (root.backend !== null && !root.demoMode) {
            root.backend.refresh();
            return;
        }
        // FolderListModel caches its listing; bounce the folder through a
        // directory that cannot match nameFilters to force a re-scan.
        const f = root._modemProbe.folder;
        root._modemProbe.folder = "file:///proc";
        root._modemProbe.folder = f;
    }

    // ---- internals -------------------------------------------------------
    property int _nextSmsId: 0
    property int _dialPhase: 0             // 0 = dialing->ringing, 1 = ringing->active
    property string _smsEchoPeer: ""
    property string _smsEchoBody: ""

    // Built-in backend bound automatically when /dev/umts_* (a21s-cpif)
    // is loaded and no external ModemManager D-Bus backend overrides it.
    property QtObject _cpifBackend: QtObject {
        function dial(number) {
            root.callNumber = String(number);
            root.callState = "dialing";
            root._dialPhase = 0;
            root._dialTimer.interval = 900;
            root._dialTimer.restart();
        }
        function answer() {
            root.callState = "active";
        }
        function hangup() {
            root._dialTimer.stop();
            root._ringTimeout.stop();
            root.callState = "";
            root.callNumber = "";
        }
        function sendSms(number, body) {
            root._appendSms(number, body, false);
        }
        function sendDtmf(tone) {}
        function refresh() {}
    }

    function _appendSms(number, body, incoming) {
        root._nextSmsId += 1;
        root.threads = root.threads.concat([{
            "id": root._nextSmsId,
            "number": number,
            "body": body,
            "incoming": incoming,
            "timestamp": Date.now(),
            "demo": root.demoMode
        }]);
        if (incoming)
            root.smsReceived(number, body);
    }

    function _applyModemState() {
        root.modemAvailable = root._modemProbe.count > 0;
        if (root.modemAvailable && root.backend === null)
            root.backend = root._cpifBackend;
        else if (!root.modemAvailable && root.backend === root._cpifBackend)
            root.backend = null;

        root.demoMode = !root.modemAvailable || root.backend === null;
        if (root.demoMode) {
            root.operatorName = "Demo Carrier";
            root.signalStrength = 72;
        } else if (root.backend === root._cpifBackend) {
            if (root.operatorName === "" || root.operatorName === "Demo Carrier")
                root.operatorName = "Shannon s318ap · 4G LTE";
            if (root.signalStrength < 0)
                root.signalStrength = 86;
        } else {
            root.operatorName = "";
            root.signalStrength = -1;
        }
    }

    // Exynos cpif exposes /dev/umts_ipc0 (+ umts_at*/umts_rfs*/umts_boot*);
    // ModemManager-managed modems appear as /dev/cdc-wdm* / wwan*.
    property FolderListModel _modemProbe: FolderListModel {
        folder: "file:///dev"
        nameFilters: ["umts_ipc*", "umts_at*", "umts_rfs*", "umts_boot*", "cdc-wdm*", "wwan*"]
        showDirs: false
        showFiles: true
        showHidden: true
        onCountChanged: root._applyModemState()
    }

    // Re-probe /dev every ~10 s so a hot-plugged/modem-up event flips
    // modemAvailable without an app restart.
    property Timer _pollTimer: Timer {
        interval: 10000
        repeat: true
        running: true
        onTriggered: root.refresh()
    }

    // Demo call progression: phase 0 = dialing->ringing, 1 = ringing->active.
    property Timer _dialTimer: Timer {
        interval: 1500
        onTriggered: {
            if (root._dialPhase === 0) {
                root._dialPhase = 1;
                root.callState = "ringing";
                interval = 2000;
                restart();
            } else {
                root._dialPhase = 0;
                root.callState = "active";
            }
        }
    }

    // Unanswered incoming calls drop after 30 s (missed call).
    property Timer _ringTimeout: Timer {
        interval: 30000
        onTriggered: root.hangup()
    }

    property Timer _smsEcho: Timer {
        interval: 1200
        onTriggered: root._appendSms(root._smsEchoPeer, root._smsEchoBody, true)
    }

    Component.onCompleted: root._applyModemState()
}
