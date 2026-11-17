.pragma library
// Stable power connections; an already-connected charger at boot is baseline.
var DEBOUNCE_MS = 1500;

function create() {
    return { primed: false, connected: false, candidate: null, since: 0, event: "" };
}

function connection(status, usbState) {
    if (usbState && usbState !== "not attached" && usbState !== "unknown") return true;
    if (status === "Charging" || status === "Full") return true;
    if (status === "Discharging" || status === "Not charging") return false;
    return null;
}

function sample(s, connected, now) {
    if (connected === null)
        return { primed: s.primed, connected: s.connected, candidate: s.candidate,
                 since: s.since, event: "" };
    if (!s.primed || connected === s.connected)
        return { primed: true, connected: connected, candidate: null, since: 0, event: "" };
    if (s.candidate !== connected)
        return { primed: true, connected: s.connected, candidate: connected, since: now, event: "" };
    if (now - s.since < DEBOUNCE_MS)
        return { primed: true, connected: s.connected, candidate: connected, since: s.since, event: "" };
    return { primed: true, connected: connected, candidate: null, since: 0,
             event: connected ? "started" : "stopped" };
}

if (typeof module !== "undefined") module.exports = { create: create, sample: sample, connection: connection };
