.pragma library

// One launcher entry per base-app role. Alternatives stay installed and remain
// available as a fallback when a preferred desktop entry is absent or hidden.
var preferredGroups = [
    ["google-chrome", "com.google.Chrome", "chromium", "org.kde.angelfish", "a21s-browser"],
    ["a21s-dialer", "org.gnome.Calls"],
    ["a21s-sms", "sm.puri.Chatty"],
    ["a21s-settings", "org.kde.mobile.plasmasettings"],
    ["org.kde.index", "org.kde.dolphin", "a21s-files"],
    ["org.kde.merkuro.calendar", "org.kde.merkuro", "org.kde.calindori", "a21s-calendar"],
    ["a21s-calculator", "org.kde.kalk"],
    ["a21s-clock", "org.kde.kclock"],
    ["a21s-notes", "org.kde.marknote", "org.kde.buho"],
    ["a21s-photos", "a21s-gallery", "org.kde.pix", "org.kde.koko"],
    ["a21s-music", "org.kde.elisa", "org.kde.vvave"],
    ["a21s-terminal", "org.kde.qmlkonsole", "org.kde.konsole", "org.kde.station", "xterm", "uxterm"],
    ["org.gnome.Contacts", "org.kde.merkuro.contact", "org.kde.merkuro.contacts", "a21s-contacts"]
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
