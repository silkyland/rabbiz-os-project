.pragma library
// Gesture — pure classifier for Android-style gesture navigation (no Qt; tested with node).
// Bottom edge: one continuous swipe decides the action when the finger lifts:
//   swipe up past HOME_DIST or flick faster than HOME_FLICK        -> "home"
//   swipe up past HOLD_DIST then HOLD_MS without lifting ("hold")   -> "recents" (fires while still down)
//   mostly-horizontal swipe along the bar                           -> "prev" (right) / "next" (left)
// Side edges: swipe inward past BACK_DIST                           -> "back"
// All distances in px at 720x1600; velocities in px/s.
var HOME_DIST = 110;
var HOME_FLICK = 900;
var HOLD_DIST = 90;
var HOLD_MS = 300;
var HSWIPE_DIST = 90;
var BACK_DIST = 72;

// Called on every move while the finger is down. Returns "recents" once the hold condition is met.
function duringBottom(dx, dy, heldMs) {
    var up = -dy;
    if (up >= HOLD_DIST && heldMs >= HOLD_MS && Math.abs(dx) < up) return "recents";
    return "none";
}

// Called on release. dx/dy = total displacement (dy < 0 is up), vy = release velocity (negative = up).
function endBottom(dx, dy, vy, alreadyRecents) {
    if (alreadyRecents) return "none";
    var up = -dy;
    if (Math.abs(dx) >= HSWIPE_DIST && Math.abs(dx) > 2 * Math.max(0, up)) return dx > 0 ? "prev" : "next";
    if (up >= HOME_DIST || (up > 30 && -vy >= HOME_FLICK)) return "home";
    if (Math.abs(dx) < 16 && Math.abs(dy) < 16) return "home";  // plain tap on the pill keeps the old behaviour
    return "none";
}

// side: "left" | "right"; dx: finger displacement in px (positive = rightwards).
function endEdge(side, dx) {
    var inward = side === "left" ? dx : -dx;
    return inward >= BACK_DIST ? "back" : "none";
}

// 0..1 visual progress for the bottom drag (pill lift / card scale).
function bottomProgress(dy) {
    return Math.max(0, Math.min(1, -dy / (HOME_DIST * 2)));
}
function edgeProgress(side, dx) {
    var inward = side === "left" ? dx : -dx;
    return Math.max(0, Math.min(1, inward / BACK_DIST));
}

if (typeof module !== "undefined") module.exports = { duringBottom, endBottom, endEdge, bottomProgress, edgeProgress,
    HOME_DIST, HOME_FLICK, HOLD_DIST, HOLD_MS, HSWIPE_DIST, BACK_DIST };
