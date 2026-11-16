.pragma library
// LaunchFlow — pure state machine for "tap icon -> starting window -> app maps -> done".
// No Qt, no Quickshell: runs under node for tests (see shell/tests/launchflow.test.mjs).
// States: idle -> launching -> mapped -> (idle) ; launching -> timedOut -> idle ; mapped -> closing -> idle
// TODO(M7.1): drive from ToplevelManager.toplevels (QML side) and render in A21s.Windowing.

var LAUNCH_TIMEOUT_MS = 8000;
var MAPPED_HOLD_MS = 140; // keep overlay until the first frame is likely drawn

function create() {
    return { state: "idle", appId: "", origin: null, startedAt: 0, mappedAt: 0 };
}

// origin = {x, y, w, h, radius} of the tapped icon, so the morph can start there.
function launch(s, appId, origin, now) {
    if (s.state === "launching") return s; // ignore double tap
    return { state: "launching", appId: appId, origin: origin, startedAt: now, mappedAt: 0 };
}

function toplevelMapped(s, appId, now) {
    if (s.state !== "launching" || s.appId !== appId) return s;
    return { state: "mapped", appId: s.appId, origin: s.origin, startedAt: s.startedAt, mappedAt: now };
}

function close(s, now) {
    if (s.state !== "mapped" && s.state !== "launching") return s;
    return { state: "closing", appId: s.appId, origin: s.origin, startedAt: now, mappedAt: s.mappedAt };
}

// Called on a timer. Returns the next state; never leaves a stuck overlay.
function tick(s, now) {
    if (s.state === "launching" && now - s.startedAt >= LAUNCH_TIMEOUT_MS) return create();
    if (s.state === "mapped" && now - s.mappedAt >= MAPPED_HOLD_MS) return create();
    return s;
}

function overlayVisible(s) { return s.state === "launching" || s.state === "closing"; }

if (typeof module !== "undefined") module.exports = { create, launch, toplevelMapped, close, tick, overlayVisible, LAUNCH_TIMEOUT_MS, MAPPED_HOLD_MS };
