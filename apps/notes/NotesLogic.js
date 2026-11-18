.pragma library
// NotesLogic — pure helpers for the in-house Notes app (no Qt imports; tested with node).
//
// A note is { id, title, body, createdAt, updatedAt }:
//   id        decimal string of the creation timestamp (also the file name, never a path)
//   title     user-typed title; may be empty, then the first line of the body is the title
//   body      plain text (TextEdit.PlainText), may contain newlines and Thai
//   createdAt / updatedAt  milliseconds since epoch
//
// One note per file, serialised on a single line: JSON.stringify escapes newlines, so the
// loader can read the whole directory as newline-separated records and skip corrupt ones.

var TITLE_MAX = 120;
var PREVIEW_MAX = 160;
var MINUTE = 60 * 1000;
var HOUR = 60 * MINUTE;
var DAY = 24 * HOUR;
var MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
              "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

// File names are the note ids; keep them to digits so a corrupt file can never point
// outside ~/.local/share/rabbiz-notes.
function isValidId(id) {
    return typeof id === "string" && /^[0-9]{1,16}$/.test(id);
}

// Id = creation timestamp, bumped when two notes would share one.
function nextId(notes, now) {
    var base = Math.floor(Number(now));
    if (!isFinite(base) || base <= 0) base = Date.now();
    var taken = {};
    var list = notes || [];
    for (var i = 0; i < list.length; i++) {
        if (list[i] && list[i].id !== undefined && list[i].id !== null)
            taken[String(list[i].id)] = true;
    }
    var id = base;
    while (taken[String(id)]) id++;
    return String(id);
}

// Collapse all whitespace (including newlines) to single spaces; used for titles/previews.
function oneLine(text) {
    if (text === undefined || text === null) return "";
    return String(text).replace(/\s+/g, " ").trim();
}

function _string(value) {
    if (typeof value === "string") return value;
    if (value === undefined || value === null) return "";
    return String(value);
}

function _positive(value, fallback) {
    var n = Number(value);
    return isFinite(n) && n > 0 ? n : fallback;
}

function _truncate(text, max) {
    if (text.length <= max) return text;
    return text.slice(0, max - 1) + "\u2026";
}

// First non-empty line of a text block, already collapsed to one line.
function firstLine(text) {
    var lines = _string(text).split("\n");
    for (var i = 0; i < lines.length; i++) {
        var line = oneLine(lines[i]);
        if (line.length > 0) return line;
    }
    return "";
}

// Display title: the typed title, else the first line of the body, else "Untitled note".
function titleOf(note) {
    var typed = note ? oneLine(_string(note.title)) : "";
    var text = typed.length > 0 ? typed : firstLine(note ? note.body : "");
    if (text.length === 0) return "Untitled note";
    return _truncate(text, TITLE_MAX);
}

// Display preview: the body without the line already used as the derived title.
function previewOf(note) {
    var body = note ? _string(note.body) : "";
    var typed = note ? oneLine(_string(note.title)) : "";
    var rest = body;
    if (typed.length === 0) {
        var lines = body.split("\n");
        var kept = [];
        var dropped = false;
        for (var i = 0; i < lines.length; i++) {
            if (!dropped && oneLine(lines[i]).length > 0) { dropped = true; continue; }
            kept.push(lines[i]);
        }
        rest = kept.join("\n");
    }
    return _truncate(oneLine(rest), PREVIEW_MAX);
}

// Case-insensitive search over the displayed title and the body. Thai has no case, so
// toLowerCase() is a no-op there and substring matching still works.
function searchNotes(notes, query) {
    var list = notes || [];
    var q = oneLine(query).toLowerCase();
    if (q.length === 0) return list.slice();
    var out = [];
    for (var i = 0; i < list.length; i++) {
        var note = list[i];
        if (!note) continue;
        var hay = (titleOf(note) + "\n" + _string(note.body)).toLowerCase();
        if (hay.indexOf(q) >= 0) out.push(note);
    }
    return out;
}

// Newest edit first; ties go to the newer id. Returns a copy, never mutates the input.
function sortNotes(notes) {
    var copy = (notes || []).slice();
    copy.sort(function (a, b) {
        var au = a ? Number(a.updatedAt) : 0;
        var bu = b ? Number(b.updatedAt) : 0;
        if (!isFinite(au)) au = 0;
        if (!isFinite(bu)) bu = 0;
        if (bu !== au) return bu - au;
        var aid = a && a.id !== undefined && a.id !== null ? String(a.id) : "";
        var bid = b && b.id !== undefined && b.id !== null ? String(b.id) : "";
        if (aid === bid) return 0;
        return aid < bid ? 1 : -1;
    });
    return copy;
}

// JSON for one on-disk note. Unknown fields are dropped; ids are always strings.
function serialize(note) {
    var id = _string(note && note.id);
    var createdAt = _positive(note && note.createdAt, _positive(id, 0));
    var updatedAt = _positive(note && note.updatedAt, createdAt);
    return JSON.stringify({
        v: 1,
        id: id,
        title: _string(note && note.title),
        body: _string(note && note.body),
        createdAt: createdAt,
        updatedAt: updatedAt
    });
}

// Validate one parsed record. Returns a clean note or null; never throws.
function normalizeNote(raw) {
    if (!raw || typeof raw !== "object") return null;
    var id = _string(raw.id);
    if (!isValidId(id)) return null;
    var createdAt = _positive(raw.createdAt, Number(id));
    if (!isFinite(createdAt) || createdAt <= 0) createdAt = Date.now();
    var updatedAt = _positive(raw.updatedAt, createdAt);
    return {
        id: id,
        title: _string(raw.title),
        body: _string(raw.body),
        createdAt: createdAt,
        updatedAt: updatedAt
    };
}

// Parse one line of the loader dump. Corrupt records are skipped, not thrown.
function parseNote(line) {
    if (typeof line !== "string") return null;
    var trimmed = line.trim();
    if (trimmed.length === 0) return null;
    var raw;
    try { raw = JSON.parse(trimmed); } catch (e) { return null; }
    return normalizeNote(raw);
}

// Whole-directory dump (one note per line, blank lines allowed). Duplicate ids keep the
// newest copy, and the result is sorted newest-first.
function parseDump(text) {
    var byId = {};
    var order = [];
    if (typeof text === "string" && text.length > 0) {
        var lines = text.split("\n");
        for (var i = 0; i < lines.length; i++) {
            var note = parseNote(lines[i]);
            if (!note) continue;
            var seen = byId[note.id];
            if (!seen) {
                byId[note.id] = note;
                order.push(note.id);
            } else if (note.updatedAt >= seen.updatedAt) {
                byId[note.id] = note;
            }
        }
    }
    var out = [];
    for (var j = 0; j < order.length; j++) out.push(byId[order[j]]);
    return sortNotes(out);
}

function _midnight(ms) {
    var d = new Date(ms);
    return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

// "just now" / "5 mins ago" / "2 hours ago" / "Yesterday" / "3 days ago" / "16 Nov" /
// "16 Nov 2025". Empty string when the timestamp is unusable, so callers can hide it.
function formatEdited(ts, now) {
    var t = Number(ts);
    if (!isFinite(t) || t <= 0) return "";
    var n = Number(now);
    if (!isFinite(n) || n <= 0) n = Date.now();
    var diff = n - t;
    if (diff < 0) diff = 0;
    if (diff < MINUTE) return "just now";
    if (diff < HOUR) {
        var mins = Math.floor(diff / MINUTE);
        return mins + (mins === 1 ? " min ago" : " mins ago");
    }
    var d = new Date(t);
    var days = Math.round((_midnight(n) - _midnight(t)) / DAY);
    if (days <= 0) {
        var hours = Math.floor(diff / HOUR);
        return hours + (hours === 1 ? " hour ago" : " hours ago");
    }
    if (days === 1) return "Yesterday";
    if (days < 7) return days + " days ago";
    var label = d.getDate() + " " + MONTHS[d.getMonth()];
    if (d.getFullYear() !== new Date(n).getFullYear()) label += " " + d.getFullYear();
    return label;
}
