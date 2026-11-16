.pragma library
// MusicLogic — pure helpers for the in-house Music app (no Qt; tested with node).
//
// The library comes from
//   find "$HOME/Music" "$HOME/Downloads" ... -printf '%T@\t%p\n' | sort -rn
// so every input line is "<mtime>\t<absolute path>". Nothing here touches the
// filesystem, mpv or Quickshell, which keeps the list parsing, wraparound and time
// formatting testable in node (tests/music.test.mjs).

var AUDIO_EXTENSIONS = ["mp3", "flac", "ogg", "m4a", "wav", "opus"];

function baseName(path) {
    var s = String(path === undefined || path === null ? "" : path);
    var i = s.lastIndexOf("/");
    return i >= 0 ? s.slice(i + 1) : s;
}

function extensionOf(path) {
    var base = baseName(path);
    var dot = base.lastIndexOf(".");
    if (dot <= 0 || dot === base.length - 1) return "";
    return base.slice(dot + 1).toLowerCase();
}

// "01_Dark_Matter.flac" -> "01 Dark Matter". The extension is dropped, underscores
// become spaces, runs of whitespace collapse (Thai text passes through untouched).
function titleFromPath(path) {
    var base = baseName(path);
    var dot = base.lastIndexOf(".");
    if (dot > 0) base = base.slice(0, dot);
    base = base.replace(/_+/g, " ").replace(/\s+/g, " ").trim();
    return base.length > 0 ? base : "Untitled";
}

// Parses the find output into track objects, newest first.
// Each track: { path, title, ext, folder, subfolder, caption, mtime }
//   folder   "Music" | "Downloads" | "Other"
//   caption  "Music · Albums/Blue" — what the list row shows as its second line.
function parseFind(raw, homeDir) {
    var home = String(homeDir === undefined || homeDir === null ? "" : homeDir).replace(/\/+$/, "");
    var musicRoot = home + "/Music/";
    var downloadsRoot = home + "/Downloads/";
    var seen = {};
    var out = [];
    var lines = String(raw === undefined || raw === null ? "" : raw).split("\n");

    for (var i = 0; i < lines.length; i++) {
        var line = lines[i].replace(/\r$/, "");
        if (line.length === 0) continue;

        var tab = line.indexOf("\t");
        var mtime = 0;
        var path = line;
        if (tab > 0) {
            mtime = parseFloat(line.slice(0, tab));
            if (!isFinite(mtime)) mtime = 0;
            path = line.slice(tab + 1);
        }
        if (path.length === 0 || seen[path] === true) continue;

        var ext = extensionOf(path);
        if (AUDIO_EXTENSIONS.indexOf(ext) < 0) continue;
        seen[path] = true;

        var folder = "Other";
        var rel = "";
        if (path.indexOf(musicRoot) === 0) {
            folder = "Music";
            rel = path.slice(musicRoot.length);
        } else if (path.indexOf(downloadsRoot) === 0) {
            folder = "Downloads";
            rel = path.slice(downloadsRoot.length);
        }
        var slash = rel.lastIndexOf("/");
        var subfolder = slash > 0 ? rel.slice(0, slash) : "";

        out.push({
            path: path,
            title: titleFromPath(path),
            ext: ext,
            folder: folder,
            subfolder: subfolder,
            caption: subfolder.length > 0 ? folder + " · " + subfolder : folder,
            mtime: mtime
        });
    }

    out.sort(function (a, b) {
        if (b.mtime !== a.mtime) return b.mtime - a.mtime;
        return a.path < b.path ? -1 : a.path > b.path ? 1 : 0;
    });
    return out;
}

// ffprobe -show_entries format=filename,duration -of default=nw=1 prints
//   filename=/home/gamer/Music/a.flac
//   duration=214.75
// Returns { path, duration } with duration 0 when the value is "N/A" or absent.
function parseProbe(raw) {
    var out = { path: "", duration: 0 };
    var lines = String(raw === undefined || raw === null ? "" : raw).split("\n");
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i].replace(/\r$/, "");
        var eq = line.indexOf("=");
        if (eq <= 0) continue;
        var key = line.slice(0, eq);
        var value = line.slice(eq + 1);
        if (key === "filename") out.path = value;
        else if (key === "duration") out.duration = parseDuration(value);
    }
    return out;
}

function parseDuration(raw) {
    var value = typeof raw === "number"
        ? raw
        : parseFloat(String(raw === undefined || raw === null ? "" : raw));
    if (!isFinite(value) || value <= 0) return 0;
    return value;
}

// "0:05", "3:07", "1:02:03"; unknown lengths print "--:--" instead of a fake 0.
function formatTime(seconds) {
    var value = typeof seconds === "number"
        ? seconds
        : parseFloat(String(seconds === undefined || seconds === null ? "" : seconds));
    if (!isFinite(value) || value < 0) return "--:--";
    var total = Math.floor(value);
    var h = Math.floor(total / 3600);
    var m = Math.floor((total % 3600) / 60);
    var s = total % 60;
    var ss = s < 10 ? "0" + s : String(s);
    if (h > 0) {
        var mm = m < 10 ? "0" + m : String(m);
        return h + ":" + mm + ":" + ss;
    }
    return m + ":" + ss;
}

// Explicit next/previous wrap around the list (transport buttons).
function nextIndex(index, count) {
    if (count <= 0) return -1;
    if (index < 0) return 0;
    return (index + 1) % count;
}

function prevIndex(index, count) {
    if (count <= 0) return -1;
    if (index < 0) return 0;
    return (index - 1 + count) % count;
}

// End-of-track advance: stop at the end of the list instead of looping forever.
function autoNextIndex(index, count) {
    if (count <= 0) return -1;
    if (index < 0) return 0;
    return index + 1 < count ? index + 1 : -1;
}

function indexOfPath(tracks, path) {
    var list = tracks || [];
    for (var i = 0; i < list.length; i++) {
        if (list[i] && list[i].path === path) return i;
    }
    return -1;
}

function countFolder(tracks, folder) {
    var list = tracks || [];
    var n = 0;
    for (var i = 0; i < list.length; i++) {
        if (list[i] && list[i].folder === folder) n++;
    }
    return n;
}

function filterTracks(tracks, scope) {
    var list = tracks || [];
    if (!scope || scope === "all") return list.slice();
    var want = scope === "music" ? "Music" : scope === "downloads" ? "Downloads" : "";
    if (want.length === 0) return list.slice();
    var out = [];
    for (var i = 0; i < list.length; i++) {
        if (list[i] && list[i].folder === want) out.push(list[i]);
    }
    return out;
}

function progressFraction(elapsedSec, durationSec) {
    if (!(durationSec > 0)) return 0;
    var fraction = elapsedSec / durationSec;
    if (!isFinite(fraction)) return 0;
    return fraction < 0 ? 0 : fraction > 1 ? 1 : fraction;
}

function seekSeconds(fraction, durationSec) {
    if (!(durationSec > 0)) return 0;
    var f = Number(fraction);
    if (!isFinite(f)) f = 0;
    f = f < 0 ? 0 : f > 1 ? 1 : f;
    return f * durationSec;
}
