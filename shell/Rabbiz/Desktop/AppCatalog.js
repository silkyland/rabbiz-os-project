.pragma library

// One launcher entry per base-app role. Alternatives stay installed and remain
// available as a fallback when a preferred desktop entry is absent or hidden.
// Legacy `a21s-*` ids stay listed so not-yet-redeployed devices (old .desktop
// files) keep resolving to the same app role; the `rabbiz-*` entry wins when
// both are installed.
var preferredGroups = [
    ["google-chrome", "com.google.Chrome", "chromium", "org.kde.angelfish", "rabbiz-browser", "a21s-browser"],
    ["rabbiz-dialer", "a21s-dialer", "org.gnome.Calls"],
    ["rabbiz-sms", "a21s-sms", "sm.puri.Chatty"],
    ["rabbiz-settings", "a21s-settings", "org.kde.mobile.plasmasettings"],
    ["org.kde.index", "org.kde.dolphin", "rabbiz-files", "a21s-files"],
    ["org.kde.merkuro.calendar", "org.kde.merkuro", "org.kde.calindori", "rabbiz-calendar", "a21s-calendar"],
    ["rabbiz-calculator", "a21s-calculator", "org.kde.kalk"],
    ["rabbiz-clock", "a21s-clock", "org.kde.kclock"],
    ["rabbiz-notes", "a21s-notes", "org.kde.marknote", "org.kde.buho"],
    ["rabbiz-photos", "a21s-photos", "rabbiz-gallery", "a21s-gallery", "org.kde.pix", "org.kde.koko"],
    ["rabbiz-music", "a21s-music", "org.kde.elisa", "org.kde.vvave"],
    ["rabbiz-terminal", "a21s-terminal", "org.kde.qmlkonsole", "org.kde.konsole", "org.kde.station", "xterm", "uxterm"],
    ["org.gnome.Contacts", "org.kde.merkuro.contact", "org.kde.merkuro.contacts", "rabbiz-contacts", "a21s-contacts"]
]

function visibleEntries(entries) {
    var available = Object.create(null)
    var result = []
    for (var i = 0; i < entries.length; ++i) {
        var entry = entries[i]
        if (entry && !entry.noDisplay && !entry.hidden && entry.id && !available[entry.id])
            available[entry.id] = entry
    }
    var suppressed = Object.create(null)
    for (var g = 0; g < preferredGroups.length; ++g) {
        var group = preferredGroups[g]
        var chosen = ""
        for (var j = 0; j < group.length; ++j) {
            if (!available[group[j]]) continue
            if (!chosen) chosen = group[j]
            else suppressed[group[j]] = true
        }
    }
    var seen = Object.create(null)
    for (var k = 0; k < entries.length; ++k) {
        var candidate = entries[k]
        if (!candidate || available[candidate.id] !== candidate || suppressed[candidate.id] || seen[candidate.id])
            continue
        result.push(candidate)
        seen[candidate.id] = true
    }
    return result
}

// Entries that ship a desktop file but are not launchable apps: KDE daemons/helpers,
// control modules and developer tools. The drawer and the add-app picker share this list.
var HELPER_IDS = [
    "org.kde.kiod6",
    "ktelnetservice6",
    "org.kde.knewstuff-dialog6",
    "org.kde.ksecretd",
    "org.kde.kwalletd",
    "cmake-gui",
    "qv4l2",
    "qvidcap",
    "org.gnupg.pinentry-qt"
]

function idOf(entry) {
    return entry && entry.id !== undefined ? String(entry.id) : ""
}

function nameOf(entry) {
    return entry && entry.name ? String(entry.name) : ""
}

// A desktop file the user would recognise as an app (not a KCM, handler or helper).
function isLaunchable(entry) {
    var id = idOf(entry)
    if (id.length === 0)
        return false
    if (id.indexOf("kcm_") === 0)
        return false
    if (id.indexOf("-geo-handler") >= 0)
        return false
    return HELPER_IDS.indexOf(id) < 0
}

// Name search; empty query matches everything. Case- and whitespace-insensitive.
function matchesQuery(entry, query) {
    var q = String(query === undefined || query === null ? "" : query).trim().toLowerCase()
    if (q.length === 0)
        return true
    return nameOf(entry).toLowerCase().indexOf(q) >= 0
}

function sortByName(list) {
    list.sort(function(a, b) { return nameOf(a).localeCompare(nameOf(b)) })
    return list
}

// What a picker shows: one entry per preferred role, launchable only, search-filtered.
function launcherEntries(entries, query) {
    var visible = visibleEntries(entries ? entries : [])
    var result = []
    for (var i = 0; i < visible.length; ++i) {
        if (isLaunchable(visible[i]) && matchesQuery(visible[i], query))
            result.push(visible[i])
    }
    return sortByName(result)
}
