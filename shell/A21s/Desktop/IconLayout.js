.pragma library
// IconLayout — pure data model for the ~/Desktop icon grid and the dock.
// No Qt, no Quickshell: loaded by DesktopLayout.qml and run under node for tests
// (shell/tests/iconlayout.test.mjs). Owns membership, order and per-icon overrides;
// DesktopLayout.qml adds the file backend and desktop-entry resolution.
//
// Persisted state (~/.config/a21s-shell/desktop.json):
//   { version: 1,
//     desktop: ["org.kde.kalk", ...],
//     dock:    ["a21s-dialer", ...],
//     overrides: { "<entry id>": { label: "Calc", icon: "calculate" } } }
//
// Arrays hold desktop-entry ids in display order. An id that no longer resolves to an
// installed entry is kept (not silently dropped) so removing one app does not reorder
// the user's desktop; DesktopView renders those as placeholder tiles.

var VERSION = 1;
// 720 px dock, 100 px tiles + spacing: five fit, six do not.
var DOCK_MAX = 5;
// Sanity bound — the grid does not scroll, so an unbounded list would run off-screen.
var DESKTOP_MAX = 40;

var SURFACE_DESKTOP = "desktop";
var SURFACE_DOCK = "dock";

// First-boot layout. `ids` is the desktop-entry preference list (first installed id wins);
// `id` is the placeholder used when nothing in the list is installed, and `name`/`icon`
// describe that placeholder. Mirrors the pre-layout HomeScreen seed exactly.
var DESKTOP_SEED = [
    { ids: ["org.kde.index", "org.kde.dolphin", "a21s-files"], id: "a21s-files", name: "Files", icon: "folder" },
    { ids: ["org.kde.merkuro.calendar", "org.kde.merkuro", "org.kde.calindori", "a21s-calendar"],
      id: "org.kde.calindori", name: "Calendar", icon: "calendar_month" },
    { ids: ["a21s-clock", "org.kde.kclock"], id: "a21s-clock", name: "Clock", icon: "schedule" },
    { ids: ["a21s-calculator", "org.kde.kalk"], id: "a21s-calculator", name: "Calculator", icon: "calculate" },
    { ids: ["org.kde.kweather"], id: "org.kde.kweather", name: "Weather", icon: "wb_sunny" },
    { ids: ["a21s-notes", "org.kde.marknote"], id: "a21s-notes", name: "Notes", icon: "edit_note" },
    { ids: ["a21s-photos", "org.kde.pix", "org.kde.koko"], id: "a21s-gallery", name: "Gallery", icon: "photo_library" },
    { ids: ["a21s-terminal", "org.kde.qmlkonsole"], id: "org.kde.qmlkonsole", name: "Terminal", icon: "terminal" }
];

// Dock favourites. The browser keeps whatever identity its package ships (Chrome stays
// "Google Chrome" with Chrome's logo); the a21s wrapper is only the fallback.
var DOCK_SEED = [
    { ids: ["a21s-dialer"], id: "a21s-dialer", name: "Phone", icon: "call" },
    { ids: ["a21s-sms"], id: "a21s-sms", name: "Messages", icon: "sms" },
    { ids: ["a21s-settings"], id: "a21s-settings", name: "Settings", icon: "settings" },
    { ids: ["google-chrome", "com.google.Chrome", "a21s-browser"], id: "a21s-browser", name: "Browser", icon: "language" }
];

function surfaceOf(value) {
    return value === SURFACE_DOCK ? SURFACE_DOCK : SURFACE_DESKTOP;
}

function capacity(surface) {
    return surfaceOf(surface) === SURFACE_DOCK ? DOCK_MAX : DESKTOP_MAX;
}

// Ids come from desktop files and user JSON; keep them short and printable.
function cleanId(raw) {
    if (raw === null || raw === undefined)
        return "";
    var s = String(raw).replace(/[\u0000-\u001f]/g, "").trim();
    return s.length > 200 ? s.slice(0, 200) : s;
}

function copyOverrides(src) {
    var out = {};
    if (!src || typeof src !== "object")
        return out;
    for (var key in src) {
        if (!Object.prototype.hasOwnProperty.call(src, key))
            continue;
        var raw = src[key];
        if (!raw || typeof raw !== "object")
            continue;
        var patch = {};
        if (raw.label !== undefined && raw.label !== null) {
            var label = String(raw.label).replace(/[\u0000-\u001f]/g, "").trim();
            if (label.length > 0 && label.length <= 48)
                patch.label = label;
        }
        if (raw.icon !== undefined && raw.icon !== null) {
            var icon = cleanId(raw.icon);
            if (icon.length > 0 && icon.length <= 64)
                patch.icon = icon;
        }
        var id = cleanId(key);
        if (id.length > 0 && (patch.label !== undefined || patch.icon !== undefined))
            out[id] = patch;
    }
    return out;
}

function copyList(src, cap) {
    var out = [];
    if (!src || typeof src.length !== "number")
        return out;
    for (var i = 0; i < src.length && out.length < cap; ++i) {
        var id = cleanId(src[i]);
        if (id.length > 0 && out.indexOf(id) < 0)
            out.push(id);
    }
    return out;
}

function clone(state) {
    var s = state && typeof state === "object" ? state : {};
    return {
        version: VERSION,
        desktop: copyList(s.desktop, DESKTOP_MAX),
        dock: copyList(s.dock, DOCK_MAX),
        overrides: copyOverrides(s.overrides)
    };
}

function emptyState() {
    return { version: VERSION, desktop: [], dock: [], overrides: {} };
}

function normalize(state) {
    return clone(state);
}

function createState(desktopIds, dockIds, overrides) {
    return clone({ desktop: desktopIds, dock: dockIds, overrides: overrides });
}

// null when the text is not a layout document the shell can trust (missing file,
// truncated write, hand-edited garbage); DesktopLayout then falls back to the seed.
function tryParse(raw) {
    var doc = raw;
    if (typeof raw === "string") {
        var text = raw.trim();
        if (text.length === 0)
            return null;
        try {
            doc = JSON.parse(text);
        } catch (e) {
            return null;
        }
    }
    if (!doc || typeof doc !== "object")
        return null;
    if (typeof doc.desktop === "undefined" && typeof doc.dock === "undefined")
        return null;
    return normalize(doc);
}

function parse(raw) {
    var state = tryParse(raw);
    return state === null ? emptyState() : state;
}

function serialize(state) {
    return JSON.stringify(normalize(state), null, 2) + "\n";
}

function list(state, surface) {
    return (surfaceOf(surface) === SURFACE_DOCK ? state.dock : state.desktop).slice();
}

function count(state, surface) {
    return (surfaceOf(surface) === SURFACE_DOCK ? state.dock : state.desktop).length;
}

function ids(state, surface) {
    return list(state, surface);
}

function has(state, surface, id) {
    var target = cleanId(id);
    return target.length > 0 && list(state, surface).indexOf(target) >= 0;
}

function indexOf(state, surface, id) {
    var target = cleanId(id);
    return target.length > 0 ? list(state, surface).indexOf(target) : -1;
}

function isFull(state, surface) {
    return count(state, surface) >= capacity(surface);
}

// Adds an app to a surface. Idempotent; a full dock is left untouched.
function add(state, surface, id, index) {
    var next = clone(state);
    var target = surfaceOf(surface);
    var entryId = cleanId(id);
    if (entryId.length === 0)
        return next;
    var entries = target === SURFACE_DOCK ? next.dock : next.desktop;
    if (entries.indexOf(entryId) >= 0)
        return next;
    if (entries.length >= capacity(target))
        return next;
    var at = (typeof index === "number" && index >= 0) ? Math.min(Math.floor(index), entries.length) : entries.length;
    entries.splice(at, 0, entryId);
    return next;
}

function remove(state, surface, id) {
    var next = clone(state);
    var entries = surfaceOf(surface) === SURFACE_DOCK ? next.dock : next.desktop;
    var at = entries.indexOf(cleanId(id));
    if (at >= 0)
        entries.splice(at, 1);
    return next;
}

// Moves within one surface; `toIndex` is the final index (post-removal), clamped.
function move(state, surface, fromIndex, toIndex) {
    var next = clone(state);
    var entries = surfaceOf(surface) === SURFACE_DOCK ? next.dock : next.desktop;
    var from = Math.floor(fromIndex);
    if (!(from >= 0) || from >= entries.length)
        return next;
    var to = Math.max(0, Math.min(entries.length - 1, Math.floor(toIndex)));
    if (from === to)
        return next;
    var moved = entries.splice(from, 1)[0];
    entries.splice(to, 0, moved);
    return next;
}

// Moves an app between ~/Desktop and the dock (or reorders when both surfaces match).
// Landing at `index` (or appended). Already present on the target surface: the source
// copy is dropped and the target keeps its position.
function transfer(state, fromSurface, id, toSurface, index) {
    var next = clone(state);
    var from = surfaceOf(fromSurface);
    var to = surfaceOf(toSurface);
    var entryId = cleanId(id);
    if (entryId.length === 0)
        return next;
    if (from === to)
        return move(next, to, indexOf(next, to, entryId), index);
    var source = from === SURFACE_DOCK ? next.dock : next.desktop;
    var target = to === SURFACE_DOCK ? next.dock : next.desktop;
    var at = source.indexOf(entryId);
    if (at < 0)
        return next;
    if (target.indexOf(entryId) >= 0) {
        source.splice(at, 1);
        return next;
    }
    if (target.length >= capacity(to))
        return next;
    source.splice(at, 1);
    var insertAt = (typeof index === "number" && index >= 0) ? Math.min(Math.floor(index), target.length) : target.length;
    target.splice(insertAt, 0, entryId);
    return next;
}

// Overrides are sparse merges: an empty string removes a key (back to the app's own
// name/icon), an absent key is left as it is.
function setOverride(state, id, patch) {
    var next = clone(state);
    var entryId = cleanId(id);
    if (entryId.length === 0 || !patch || typeof patch !== "object")
        return next;
    var current = next.overrides[entryId] || {};
    if (patch.label !== undefined) {
        var label = patch.label === null ? "" : String(patch.label).replace(/[\u0000-\u001f]/g, "").trim();
        if (label.length === 0 || label.length > 48)
            delete current.label;
        else
            current.label = label;
    }
    if (patch.icon !== undefined) {
        var icon = patch.icon === null ? "" : cleanId(patch.icon);
        if (icon.length === 0 || icon.length > 64)
            delete current.icon;
        else
            current.icon = icon;
    }
    if (current.label !== undefined || current.icon !== undefined)
        next.overrides[entryId] = current;
    else
        delete next.overrides[entryId];
    return next;
}

function clearOverride(state, id) {
    var next = clone(state);
    var entryId = cleanId(id);
    if (entryId.length > 0)
        delete next.overrides[entryId];
    return next;
}

function overrideFor(state, id) {
    var entryId = cleanId(id);
    var found = state && state.overrides ? state.overrides[entryId] : null;
    return found && typeof found === "object" ? found : null;
}

function labelFor(state, id, fallback) {
    var found = overrideFor(state, id);
    return found && found.label ? found.label : fallback;
}

function iconFor(state, id, fallback) {
    var found = overrideFor(state, id);
    return found && found.icon ? found.icon : fallback;
}

// Seed lookup by any id in its preference list (or by its placeholder id).
function seedFor(seedList, id) {
    var target = cleanId(id);
    if (target.length === 0)
        return null;
    for (var i = 0; i < seedList.length; ++i) {
        var seed = seedList[i];
        if (seed.id === target)
            return seed;
        if (seed.ids && seed.ids.indexOf(target) >= 0)
            return seed;
    }
    return null;
}

// "org.kde.some-app" -> "Some app": the last resort label for an id with no entry.
function prettyId(id) {
    var target = cleanId(id).replace(/\.desktop$/i, "");
    if (target.length === 0)
        return "";
    var tail = target.split(/[./]/).pop();
    tail = tail.replace(/[-_]+/g, " ").replace(/\s+/g, " ").trim();
    return tail.length > 0 ? tail.charAt(0).toUpperCase() + tail.slice(1) : target;
}

if (typeof module !== "undefined") module.exports = {
    VERSION: VERSION, DOCK_MAX: DOCK_MAX, DESKTOP_MAX: DESKTOP_MAX,
    SURFACE_DESKTOP: SURFACE_DESKTOP, SURFACE_DOCK: SURFACE_DOCK,
    DESKTOP_SEED: DESKTOP_SEED, DOCK_SEED: DOCK_SEED,
    emptyState: emptyState, normalize: normalize, createState: createState,
    tryParse: tryParse, parse: parse, serialize: serialize,
    list: list, ids: ids, count: count, has: has, indexOf: indexOf,
    capacity: capacity, isFull: isFull, add: add, remove: remove,
    move: move, transfer: transfer, setOverride: setOverride,
    clearOverride: clearOverride, overrideFor: overrideFor,
    labelFor: labelFor, iconFor: iconFor, seedFor: seedFor, prettyId: prettyId
};
