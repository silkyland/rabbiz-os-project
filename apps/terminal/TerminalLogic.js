.pragma library
// TerminalLogic — pure helpers for the in-house Terminal app (docs/BASE-APPS.md).
// No Qt, no filesystem: tested with node (tests/terminal.test.mjs).
//
// The device UI runs ONE command per line through `script -qfec '<command>' /dev/null`,
// which allocates a PTY and therefore turns "\n" into "\r\n" and may emit ANSI colour /
// cursor codes from CLI tools. Everything the app shows goes through cleanOutput().

// ---------------------------------------------------------------------------
// ANSI escaping
// ---------------------------------------------------------------------------

var ESC = "\u001b";

// OSC: ESC ] ... BEL | ST  (window title, hyperlinks, clipboard)
var RE_OSC = /\u001b\][^\u0007\u001b]*(?:\u0007|\u001b\\)/g;
// DCS / SOS / PM / APC strings: ESC P|X|^|_ ... ST
var RE_STRING = /\u001b[PX^_][\s\S]*?\u001b\\/g;
// CSI: ESC [ params intermediates final
var RE_CSI = /\u001b\[[0-?]*[ -\/]*[@-~]/g;
// Character-set designation: ESC ( B, ESC ) 0, ESC # 8
var RE_CHARSET = /\u001b[()#][0-9A-Za-z]/g;
// Two-byte escapes: ESC 7 / ESC 8 / ESC M / ESC = ...
var RE_ESC2 = /\u001b[@-Z\\-_0-9]/g;
// C1 CSI (single-byte 0x9B) and other C1 controls
var RE_C1_CSI = /\u009b[0-?]*[ -\/]*[@-~]/g;

// Remove ANSI escape sequences, then resolve carriage returns (progress bars overwrite
// their line) and backspaces the way a real terminal would.
function stripAnsi(input) {
    var s = String(input === undefined || input === null ? "" : input);
    s = s.replace(RE_OSC, "");
    s = s.replace(RE_STRING, "");
    s = s.replace(RE_CSI, "");
    s = s.replace(RE_C1_CSI, "");
    s = s.replace(RE_CHARSET, "");
    s = s.replace(RE_ESC2, "");
    s = s.replace(/\u001b/g, "");
    return s;
}

function _collapseCarriageReturns(line) {
    if (line.indexOf("\r") < 0) return line;
    var parts = line.split("\r");
    // "50%\r100%\r" -> "100%"; a bare trailing "\r" keeps the text before it.
    for (var i = parts.length - 1; i >= 0; i--) {
        if (parts[i].length > 0) return parts[i];
    }
    return "";
}

function _applyBackspaces(line) {
    if (line.indexOf("\b") < 0) return line;
    var chars = Array.from(line);
    var out = [];
    for (var i = 0; i < chars.length; i++) {
        if (chars[i] === "\b") out.pop();
        else out.push(chars[i]);
    }
    return out.join("");
}

// Turn raw PTY output into display text: no escapes, no CR overstrike, no control
// characters, right-trimmed lines and no leading/trailing blank lines. Tabs and
// indentation are preserved because CLI output (df, ls -l, ip a) aligns with them.
function cleanOutput(input) {
    var s = stripAnsi(input).replace(/\r\n/g, "\n");
    var lines = s.split("\n");
    for (var i = 0; i < lines.length; i++) {
        lines[i] = _applyBackspaces(_collapseCarriageReturns(lines[i]))
            .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f\u0080-\u009f]/g, "")
            .replace(/[ \t]+$/, "");
    }
    s = lines.join("\n");
    s = s.replace(/^\n+/, "").replace(/\n+$/, "");
    return s;
}

// Keep one command's output from freezing the software renderer. The caller adds the
// "output cut" note; the returned text never grows past maxChars.
function capOutput(text, maxChars) {
    var s = String(text === undefined || text === null ? "" : text);
    var limit = Math.floor(Number(maxChars) || 0);
    if (limit <= 0 || s.length <= limit) {
        return { text: s, truncated: false, dropped: 0 };
    }
    return { text: s.slice(0, limit), truncated: true, dropped: s.length - limit };
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

// Oldest first. Blank input is ignored; running the same command twice in a row does
// not duplicate it; the list is trimmed to `limit` entries.
function addHistory(history, command, limit) {
    var list = Array.isArray(history) ? history.slice() : [];
    var cmd = String(command === undefined || command === null ? "" : command).trim();
    if (cmd.length === 0) return list;
    if (list.length > 0 && list[list.length - 1] === cmd) return list;
    list.push(cmd);
    var max = Math.floor(Number(limit) || 0);
    while (max > 0 && list.length > max) list.shift();
    return list;
}

// index is the position currently being browsed, or -1 when typing a fresh command.
// Returns { index, text, moved }. `text` is null only when there is nothing to show
// (empty history), so the UI can keep the user's draft untouched.
function historyUp(history, index) {
    var list = Array.isArray(history) ? history : [];
    if (list.length === 0) return { index: -1, text: null, moved: false };
    if (index < 0 || index >= list.length) {
        return { index: list.length - 1, text: list[list.length - 1], moved: true };
    }
    if (index === 0) {
        return { index: 0, text: list[0], moved: false };
    }
    return { index: index - 1, text: list[index - 1], moved: true };
}

// Walking past the newest entry restores the draft that was being typed.
function historyDown(history, index, draft) {
    var list = Array.isArray(history) ? history : [];
    var text = String(draft === undefined || draft === null ? "" : draft);
    if (index < 0 || list.length === 0) return { index: -1, text: text, moved: false };
    if (index >= list.length - 1) return { index: -1, text: text, moved: true };
    return { index: index + 1, text: list[index + 1], moved: true };
}

// Parse ~/.local/share/a21s-terminal/history.json. Bad JSON, a non-array or junk
// entries all fall back to an empty history instead of breaking the app.
function loadHistory(raw, limit) {
    var data = null;
    try {
        data = JSON.parse(String(raw === undefined || raw === null ? "" : raw));
    } catch (e) {
        return [];
    }
    if (!Array.isArray(data)) return [];
    var out = [];
    for (var i = 0; i < data.length; i++) {
        var cmd = typeof data[i] === "string" ? data[i].trim() : "";
        if (cmd.length > 0) out.push(cmd);
    }
    var max = Math.floor(Number(limit) || 0);
    if (max > 0 && out.length > max) out = out.slice(out.length - max);
    return out;
}

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

function _stripTrailingSlash(path) {
    var p = String(path === undefined || path === null ? "" : path);
    while (p.length > 1 && p.charAt(p.length - 1) === "/") p = p.slice(0, -1);
    return p;
}

// "~" for home, "~/Music" below it, otherwise the path itself.
function shortPath(path, home) {
    var p = _stripTrailingSlash(path);
    var h = _stripTrailingSlash(home);
    if (p.length === 0) return "~";
    if (h.length > 0 && p === h) return "~";
    if (h.length > 0 && p.indexOf(h + "/") === 0) return "~" + p.slice(h.length);
    return p;
}

// The prompt drawn before every command in the scrollback. This app has no persistent
// working directory (each line starts in ~ again), so the prompt is stable.
function formatPrompt(path, home) {
    return shortPath(path, home) + " $";
}

// ---------------------------------------------------------------------------
// Exit status
// ---------------------------------------------------------------------------

// Empty string for success. 124 is `timeout`, 126/127 are the shell's "cannot run" codes.
function describeExit(code, timeoutSec) {
    var c = Number(code);
    if (!isFinite(c) || c === 0) return "";
    if (c === 124) {
        return timeoutSec > 0 ? "Timed out after " + timeoutSec + " s" : "Timed out";
    }
    if (c === 126) return "Not executable";
    if (c === 127) return "Command not found";
    if (c === 130 || c === 143) return "Stopped";
    if (c > 128) return "Killed by signal " + (c - 128);
    if (c < 0) return "Stopped";
    return "Exit " + c;
}
