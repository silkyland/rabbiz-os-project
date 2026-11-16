.pragma library
// PhotosLogic — pure helpers for the in-house Photos app (no Qt; tested with node).
// The app keeps only the plain objects returned here, so path parsing, ordering, index
// wrap, zoom/pan limits and date copy are all testable without a device.
// Files are listed by `find`, so every path is absolute and every mtime comes from
// find's %T@ (unix seconds, fractional). Times are stored here as JS milliseconds.

var IMAGE_EXTENSIONS = ["jpg", "jpeg", "png", "webp"];

// Double-tap zooms between these two levels; the viewer clamps anything else.
var ZOOM_MIN = 1.0;
var ZOOM_MAX = 2.5;

var MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
              "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

function _text(value) {
    return value === undefined || value === null ? "" : String(value);
}

// "a/b/Photo.JPG" -> "jpg"; a dot in a directory name or a dotfile is not an extension.
function extensionOf(path) {
    var s = _text(path);
    var slash = s.lastIndexOf("/");
    var dot = s.lastIndexOf(".");
    if (dot <= slash + 1) return "";
    return s.slice(dot + 1).toLowerCase();
}

function isImagePath(path) {
    return IMAGE_EXTENSIONS.indexOf(extensionOf(path)) >= 0;
}

function baseName(path) {
    var s = _text(path);
    var slash = s.lastIndexOf("/");
    return slash >= 0 ? s.slice(slash + 1) : s;
}

function dirName(path) {
    var s = _text(path);
    var slash = s.lastIndexOf("/");
    if (slash < 0) return "";
    if (slash === 0) return "/";
    return s.slice(0, slash);
}

// `find` argv for the photo roots (recursive, image extensions only) printing
// "<mtime>\t<path>" lines. Built as an argv array, never through a shell, so spaces and
// Thai characters in $HOME or in file names need no quoting. Empty roots -> no command.
function findArgs(roots) {
    var list = roots || [];
    var args = ["find"];
    var rooted = false;
    for (var i = 0; i < list.length; i++) {
        if (list[i]) {
            args.push(String(list[i]));
            rooted = true;
        }
    }
    if (!rooted) return [];
    args.push("-type", "f", "(");
    for (var j = 0; j < IMAGE_EXTENSIONS.length; j++) {
        if (j > 0) args.push("-o");
        args.push("-iname", "*." + IMAGE_EXTENSIONS[j]);
    }
    args.push(")");
    args.push("-printf", "%T@\\t%p\\n");
    return args;
}

// Parse find's output into photo objects, newest first. Defensive by design: a malformed
// line, a duplicate path, a CRLF ending or a non-image name is skipped instead of thrown,
// because this runs on whatever the user's home directory happens to contain.
function parseFindOutput(text) {
    var lines = _text(text).split("\n");
    var out = [];
    var seen = {};

    for (var i = 0; i < lines.length; i++) {
        var line = lines[i];
        if (line.length === 0) continue;
        if (line.charAt(line.length - 1) === "\r") line = line.slice(0, -1);
        if (line.length === 0) continue;

        var mtime = 0;
        var path = line;
        var tab = line.indexOf("\t");
        if (tab > 0) {
            mtime = parseFloat(line.slice(0, tab));
            path = line.slice(tab + 1);
            if (!isFinite(mtime)) mtime = 0;
        }
        if (path.length === 0 || !isImagePath(path)) continue;
        if (seen["p:" + path]) continue;
        seen["p:" + path] = true;

        out.push({
            path: path,
            name: baseName(path),
            dir: dirName(path),
            mtime: mtime * 1000
        });
    }

    out.sort(function (a, b) {
        if (b.mtime !== a.mtime) return b.mtime - a.mtime;      // newest first
        return a.path < b.path ? -1 : a.path > b.path ? 1 : 0;  // stable, readable ties
    });
    return out;
}

// Local path -> QML image source. Each segment is percent-encoded (UTF-8) so spaces,
// #, % and Thai names all load; "/home/g/a b.jpg" -> "file:///home/g/a%20b.jpg".
function fileUrl(path) {
    var s = _text(path);
    if (s.length === 0) return "";
    if (s.slice(0, 5) === "file:") return s;
    var parts = s.split("/");
    var out = [];
    for (var i = 0; i < parts.length; i++) out.push(encodeURIComponent(parts[i]));
    return "file://" + out.join("/");
}

function _two(n) {
    return n < 10 ? "0" + n : String(n);
}

// "Today, 14:41" / "Yesterday, 09:05" / "2 Jan, 03:04" / "2 Jan 2025, 03:04".
// `nowMs` exists for tests; the app omits it and gets the device clock.
function formatDate(ms, nowMs) {
    var t = Number(ms);
    if (!isFinite(t) || t <= 0) return "";
    var when = new Date(t);
    var now = new Date(isFinite(Number(nowMs)) ? Number(nowMs) : Date.now());

    var time = _two(when.getHours()) + ":" + _two(when.getMinutes());
    var day = new Date(when.getFullYear(), when.getMonth(), when.getDate()).getTime();
    var today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    var days = Math.round((today - day) / 86400000);

    if (days === 0) return "Today, " + time;
    if (days === 1) return "Yesterday, " + time;

    var dayPart = when.getDate() + " " + MONTHS[when.getMonth()];
    if (when.getFullYear() !== now.getFullYear()) dayPart += " " + when.getFullYear();
    return dayPart + ", " + time;
}

function formatCount(count) {
    var n = Number(count);
    if (!isFinite(n) || n <= 0) return "No photos";
    if (n < 2) return "1 photo";
    return Math.floor(n) + " photos";
}

// Shorten a folder for the viewer caption: "/home/gamer/Pictures/x" -> "~/Pictures/x".
function displayFolder(dir, home) {
    var d = _text(dir);
    var h = _text(home);
    if (h.length === 0) return d;
    if (d === h) return "~";
    if (d.indexOf(h + "/") === 0) return "~" + d.slice(h.length);
    return d;
}

// Wrap around both ends so the arrows and swipes never dead-end. -1 means "no photo".
function wrapIndex(index, count) {
    var n = Math.floor(Number(count));
    if (!isFinite(n) || n <= 0) return -1;
    var i = Math.floor(Number(index));
    if (!isFinite(i)) i = 0;
    return ((i % n) + n) % n;
}

function nextIndex(index, count) {
    return wrapIndex((isFinite(Number(index)) ? Math.floor(Number(index)) : 0) + 1, count);
}

function prevIndex(index, count) {
    return wrapIndex((isFinite(Number(index)) ? Math.floor(Number(index)) : 0) - 1, count);
}

function clampZoom(zoom, min, max) {
    var lo = isFinite(Number(min)) ? Number(min) : ZOOM_MIN;
    var hi = isFinite(Number(max)) ? Number(max) : ZOOM_MAX;
    if (hi < lo) hi = lo;
    var z = Number(zoom);
    if (!isFinite(z)) return lo;
    return z < lo ? lo : z > hi ? hi : z;
}

function zoomTarget(current) {
    return clampZoom(current, ZOOM_MIN, ZOOM_MAX) > ZOOM_MIN + 0.01 ? ZOOM_MIN : ZOOM_MAX;
}

function isZoomed(zoom) {
    var z = Number(zoom);
    return isFinite(z) && z > ZOOM_MIN + 0.01;
}

// How far the image may be dragged: only the part that overflowed the viewport.
function maxPan(paintedSize, viewSize, zoom) {
    var painted = Number(paintedSize);
    var view = Number(viewSize);
    var z = Number(zoom);
    if (!isFinite(painted) || !isFinite(view) || painted <= 0 || view <= 0) return 0;
    if (!isFinite(z) || z < 1) z = 1;
    var limit = (painted * z - view) / 2;
    return limit > 0 ? limit : 0;
}

function clampPan(offset, limit) {
    var room = Math.abs(Number(limit));
    if (!isFinite(room)) room = 0;
    var value = Number(offset);
    if (!isFinite(value)) return 0;
    return value < -room ? -room : value > room ? room : value;
}

// A left/right swipe: long enough horizontally and clearly not a vertical drag.
function isSwipe(dx, dy, threshold) {
    var x = Number(dx);
    var y = Number(dy);
    var t = Number(threshold);
    if (!isFinite(x)) x = 0;
    if (!isFinite(y)) y = 0;
    if (!isFinite(t)) t = 64;
    return Math.abs(x) > t && Math.abs(x) > Math.abs(y) * 1.2;
}
