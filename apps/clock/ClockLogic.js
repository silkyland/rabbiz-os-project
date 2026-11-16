.pragma library
// ClockLogic — pure time, alarm and countdown math for the in-house Clock app.
// No Qt here: the QML files only paint the result, so every rule below is testable with
// `node --test tests/clock.test.mjs` (no device, no Quickshell).
//
// Conventions:
//   * durations are milliseconds (numbers), times are JS `Date`
//   * alarms are plain objects: { id, hour, minute, enabled, firedDay }
//     `firedDay` is the local day key ("2026-11-16") the alarm last rang on, so one alarm
//     rings at most once per day even though the app ticks every second.
//   * functions never mutate their arguments; they return new arrays/objects.

var DAY_NAMES = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
var MONTH_NAMES = ["January", "February", "March", "April", "May", "June",
                   "July", "August", "September", "October", "November", "December"];

function _validDate(date) {
    return !!date && typeof date.getTime === "function" && !isNaN(date.getTime());
}

// Two digits, never negative, never NaN (01, 09, 23).
function pad2(n) {
    var v = Math.floor(Math.abs(Number(n)));
    if (!isFinite(v)) v = 0;
    if (v > 99) v = 99;
    return (v < 10 ? "0" : "") + v;
}

function clampInt(value, min, max, fallback) {
    var v = parseInt(value, 10);
    if (isNaN(v)) v = (fallback === undefined ? min : parseInt(fallback, 10));
    if (isNaN(v)) v = min;
    return Math.max(min, Math.min(max, v));
}

// Stepper that stops at the ends (timer minute / second).
function stepClamped(value, delta, min, max) {
    var v = clampInt(value, min, max, min) + Math.round(Number(delta) || 0);
    return Math.max(min, Math.min(max, v));
}

// Stepper that wraps around (alarm hour / minute feel like a clock face).
function stepWrapped(value, delta, min, max) {
    var span = max - min + 1;
    if (span <= 0) return min;
    var v = (clampInt(value, min, max, min) - min + Math.round(Number(delta) || 0)) % span;
    if (v < 0) v += span;
    return min + v;
}

// "14:41" in 24 h mode, "2:41 PM" in 12 h mode.
function formatClock(date, use24h) {
    if (!_validDate(date)) return "--:--";
    var h = date.getHours();
    var m = date.getMinutes();
    if (use24h) return pad2(h) + ":" + pad2(m);
    var suffix = h >= 12 ? "PM" : "AM";
    var h12 = h % 12;
    if (h12 === 0) h12 = 12;
    return h12 + ":" + pad2(m) + " " + suffix;
}

// "Sunday 16 November" — built from fixed English names, not the C locale, so the header
// reads the same on every device (the UI copy is English per docs/BASE-APPS.md).
function formatDate(date) {
    if (!_validDate(date)) return "";
    return DAY_NAMES[date.getDay()] + " " + date.getDate() + " " + MONTH_NAMES[date.getMonth()];
}

// Local calendar day, used to stop an alarm from ringing twice.
function dayKey(date) {
    if (!_validDate(date)) return "";
    return date.getFullYear() + "-" + pad2(date.getMonth() + 1) + "-" + pad2(date.getDate());
}

// Countdown display: 00:59, 05:00, 1:05:00. Rounds up so a running timer never shows
// 00:00 while time is left.
function formatDuration(ms) {
    var seconds = Math.ceil(Math.max(0, Number(ms) || 0) / 1000);
    var h = Math.floor(seconds / 3600);
    var m = Math.floor((seconds % 3600) / 60);
    var s = seconds % 60;
    if (h > 0) return h + ":" + pad2(m) + ":" + pad2(s);
    return pad2(m) + ":" + pad2(s);
}

// Stopwatch display with centiseconds: 00:12.34, 1:02:03.45.
function formatStopwatch(ms) {
    var total = Math.floor(Math.max(0, Number(ms) || 0));
    var cs = Math.floor(total / 10) % 100;
    var seconds = Math.floor(total / 1000);
    var s = seconds % 60;
    var m = Math.floor(seconds / 60) % 60;
    var h = Math.floor(seconds / 3600);
    var head = h > 0 ? (h + ":" + pad2(m) + ":" + pad2(s)) : (pad2(m) + ":" + pad2(s));
    return head + "." + pad2(cs);
}

// Countdown remaining from an absolute deadline. `done` is true only once the deadline has
// passed, so the caller rings exactly once when it flips.
function countdownRemaining(deadlineMs, nowMs) {
    var deadline = Number(deadlineMs);
    var now = Number(nowMs);
    if (!isFinite(deadline)) deadline = 0;
    if (!isFinite(now)) now = deadline;
    var remaining = deadline - now;
    return { remaining: remaining > 0 ? remaining : 0, done: deadline > 0 && remaining <= 0 };
}

// Stopwatch elapsed = time banked before the last pause + the run that is still going.
// Pass startedAt = null while paused.
function elapsedMs(accumMs, startedAtMs, nowMs) {
    var banked = Math.max(0, Number(accumMs) || 0);
    if (startedAtMs === null || startedAtMs === undefined) return banked;
    var started = Number(startedAtMs);
    var now = Number(nowMs);
    if (!isFinite(started) || !isFinite(now)) return banked;
    return banked + Math.max(0, now - started);
}

// ---- alarms ---------------------------------------------------------------

function makeAlarm(hour, minute, id) {
    return {
        id: id === undefined || id === null ? String(Date.now()) : String(id),
        hour: clampInt(hour, 0, 23, 7),
        minute: clampInt(minute, 0, 59, 0),
        enabled: true,
        firedDay: ""
    };
}

// Anything read back from disk may be stale or hand-edited: drop what cannot be an alarm
// instead of letting one bad entry break the whole list.
function normalizeAlarm(raw) {
    if (!raw || typeof raw !== "object") return null;
    var h = parseInt(raw.hour, 10);
    var m = parseInt(raw.minute, 10);
    if (isNaN(h) || isNaN(m) || h < 0 || h > 23 || m < 0 || m > 59) return null;
    var id = raw.id === undefined || raw.id === null || String(raw.id).length === 0
        ? ("alarm-" + h + "-" + m) : String(raw.id);
    return {
        id: id,
        hour: h,
        minute: m,
        enabled: raw.enabled !== false,
        firedDay: typeof raw.firedDay === "string" ? raw.firedDay : ""
    };
}

// QML can hand us a QVariantList-backed sequence (for example through a `var` signal
// parameter) that is array-like — it has .length and indexes — but is not a real Array, so
// Array.isArray() is false and .sort() may not exist. Copy it into a real array first.
function _list(value) {
    if (!value) return [];
    if (Array.isArray(value)) return value;
    if (typeof value.length === "number") {
        var out = [];
        for (var i = 0; i < value.length; ++i) out.push(value[i]);
        return out;
    }
    return [];
}

// Accepts the raw file text, a parsed array, or garbage; always returns a sorted array.
function parseAlarms(raw) {
    var data = raw;
    if (typeof raw === "string") {
        try { data = JSON.parse(raw); } catch (e) { return []; }
    }
    var items = _list(data);
    var out = [];
    for (var i = 0; i < items.length; ++i) {
        var alarm = normalizeAlarm(items[i]);
        if (alarm) out.push(alarm);
    }
    return sortAlarms(out);
}

function sortAlarms(alarms) {
    var list = _list(alarms).slice();
    list.sort(function (a, b) {
        return (a.hour - b.hour) || (a.minute - b.minute);
    });
    return list;
}

function addAlarm(alarms, alarm) {
    var list = _list(alarms).slice();
    var normal = normalizeAlarm(alarm);
    if (!normal) return sortAlarms(list);
    list.push(normal);
    return sortAlarms(list);
}

function removeAlarm(alarms, id) {
    var list = _list(alarms).slice();
    var out = [];
    for (var i = 0; i < list.length; ++i) {
        if (list[i].id !== id) out.push(list[i]);
    }
    return out;
}

function setEnabled(alarms, id, enabled) {
    var list = _list(alarms).slice();
    for (var i = 0; i < list.length; ++i) {
        if (list[i].id === id) {
            list[i] = {
                id: list[i].id,
                hour: list[i].hour,
                minute: list[i].minute,
                enabled: Boolean(enabled),
                firedDay: list[i].firedDay || ""
            };
        }
    }
    return list;
}

// Record that an alarm rang today. Returns a new list.
function markFired(alarms, id, date) {
    var list = _list(alarms).slice();
    var key = dayKey(date);
    for (var i = 0; i < list.length; ++i) {
        if (list[i].id === id) {
            list[i] = {
                id: list[i].id,
                hour: list[i].hour,
                minute: list[i].minute,
                enabled: list[i].enabled,
                firedDay: key
            };
        }
    }
    return list;
}

function alarmTimeText(alarm) {
    if (!alarm) return "--:--";
    return pad2(alarm.hour) + ":" + pad2(alarm.minute);
}

// Should this alarm ring right now? True during the matching minute of a day it has not
// rung on yet — the 1 s tick can call this as often as it likes.
function alarmDue(alarm, now) {
    if (!alarm || !alarm.enabled) return false;
    if (!_validDate(now)) return false;
    if (alarm.hour !== now.getHours() || alarm.minute !== now.getMinutes()) return false;
    return alarm.firedDay !== dayKey(now);
}

// The next enabled alarm occurrence at or after `now` (the current minute still counts while
// it is ringing). Returns { id, hour, minute, at, inMs } or null when nothing is enabled.
function nextAlarm(alarms, now) {
    var list = _list(alarms);
    if (list.length === 0) return null;
    var base = _validDate(now) ? now : new Date();
    var best = null;
    for (var i = 0; i < list.length; ++i) {
        var alarm = list[i];
        if (!alarm || !alarm.enabled) continue;
        var at = new Date(base.getFullYear(), base.getMonth(), base.getDate(),
                          alarm.hour, alarm.minute, 0, 0);
        // A slot that passed more than a minute ago is tomorrow's; so is the slot that is
        // ringing right now but already rang today (the header should count down to the next
        // day instead of saying "in less than a minute" right after the alarm fired).
        var slotStarted = at.getTime() <= base.getTime();
        var firedToday = alarm.firedDay === dayKey(base);
        if (at.getTime() + 60000 < base.getTime() || (slotStarted && firedToday)) {
            at = new Date(base.getFullYear(), base.getMonth(), base.getDate() + 1,
                          alarm.hour, alarm.minute, 0, 0);
        }
        if (!best || at.getTime() < best.at.getTime()) {
            best = {
                id: alarm.id,
                hour: alarm.hour,
                minute: alarm.minute,
                at: at,
                inMs: Math.max(0, at.getTime() - base.getTime())
            };
        }
    }
    return best;
}

function nextAlarmText(alarms, now) {
    var next = nextAlarm(alarms, now);
    if (!next) return "";
    var minutes = Math.floor(next.inMs / 60000);
    if (minutes < 1) return "in less than a minute";
    if (minutes < 60) return "in " + minutes + " min";
    var h = Math.floor(minutes / 60);
    var m = minutes % 60;
    return m === 0 ? ("in " + h + " h") : ("in " + h + " h " + m + " min");
}

// One line for the header: "Next alarm 07:30 · in 7 h 12 m" (empty when none is enabled).
function nextAlarmLine(alarms, now) {
    var next = nextAlarm(alarms, now);
    if (!next) return "";
    return "Next alarm " + alarmTimeText(next) + " · " + nextAlarmText(alarms, now);
}
