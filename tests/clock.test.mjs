// node --test tests/clock.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const raw = readFileSync(new URL("../apps/clock/ClockLogic.js", import.meta.url), "utf8");
// `.pragma library` is a QML directive, not JavaScript.
const src = raw.split("\n").filter((l) => !l.trimStart().startsWith(".pragma")).join("\n");
const ctx = { console };
vm.createContext(ctx);
vm.runInContext(
  src +
    "\nthis.ClockLogic = { pad2, clampInt, stepClamped, stepWrapped, formatClock, formatDate," +
    " dayKey, formatDuration, formatStopwatch, countdownRemaining, elapsedMs, makeAlarm," +
    " normalizeAlarm, parseAlarms, sortAlarms, addAlarm, removeAlarm, setEnabled, markFired," +
    " alarmTimeText, alarmDue, nextAlarm, nextAlarmText, nextAlarmLine };",
  ctx,
);
const L = ctx.ClockLogic;
// Values produced inside the vm belong to another realm, so strict deep equality would
// compare prototypes; copy them into this realm first.
const plain = (v) => JSON.parse(JSON.stringify(v));

// Local dates only — the app never looks at UTC.
const at = (y, mo, d, h = 0, mi = 0, s = 0) => new Date(y, mo - 1, d, h, mi, s, 0);

test("pad2 keeps two digits", () => {
  assert.equal(L.pad2(0), "00");
  assert.equal(L.pad2(7), "07");
  assert.equal(L.pad2(23), "23");
  assert.equal(L.pad2(-4), "04");
  assert.equal(L.pad2("9"), "09");
  assert.equal(L.pad2(NaN), "00");
  assert.equal(L.pad2(120), "99");
});

test("clampInt rejects junk and stays in range", () => {
  assert.equal(L.clampInt(7, 0, 23, 7), 7);
  assert.equal(L.clampInt(-2, 0, 23, 7), 0);
  assert.equal(L.clampInt(99, 0, 23, 7), 23);
  assert.equal(L.clampInt("12", 0, 23, 7), 12);
  assert.equal(L.clampInt("abc", 5, 9, 6), 6);
  assert.equal(L.clampInt(undefined, 5, 9), 5);
});

test("steppers: clamp for the timer, wrap for the alarm", () => {
  assert.equal(L.stepClamped(0, -1, 0, 180), 0);
  assert.equal(L.stepClamped(180, 1, 0, 180), 180);
  assert.equal(L.stepClamped(5, 5, 0, 55), 10);
  assert.equal(L.stepWrapped(23, 1, 0, 23), 0);
  assert.equal(L.stepWrapped(0, -1, 0, 23), 23);
  assert.equal(L.stepWrapped(59, 1, 0, 59), 0);
  assert.equal(L.stepWrapped(0, -1, 0, 59), 59);
  assert.equal(L.stepWrapped(55, 5, 0, 59), 0); // 60 wraps to 0 over a 60-value span
});

test("clock and date formatting", () => {
  const t = at(2026, 11, 16, 14, 5);
  assert.equal(L.formatClock(t, true), "14:05");
  assert.equal(L.formatClock(t, false), "2:05 PM");
  assert.equal(L.formatClock(at(2026, 11, 16, 0, 0), true), "00:00");
  assert.equal(L.formatClock(at(2026, 11, 16, 0, 0), false), "12:00 AM");
  assert.equal(L.formatClock(at(2026, 11, 16, 12, 30), false), "12:30 PM");
  assert.equal(L.formatClock(at(2026, 11, 16, 9, 5), false), "9:05 AM");
  assert.equal(L.formatClock(null, true), "--:--");
  assert.equal(L.formatDate(t), "Monday 16 November");
  assert.equal(L.formatDate(null), "");
  assert.equal(L.dayKey(t), "2026-11-16");
  assert.equal(L.dayKey(at(2027, 1, 2)), "2027-01-02");
});

test("duration formatting rounds up, stopwatch shows centiseconds", () => {
  assert.equal(L.formatDuration(0), "00:00");
  assert.equal(L.formatDuration(1000), "00:01");
  assert.equal(L.formatDuration(1500), "00:02");
  assert.equal(L.formatDuration(61000), "01:01");
  assert.equal(L.formatDuration(25 * 60000), "25:00");
  assert.equal(L.formatDuration(3600000), "1:00:00");
  assert.equal(L.formatDuration(-500), "00:00");

  assert.equal(L.formatStopwatch(0), "00:00.00");
  assert.equal(L.formatStopwatch(1234), "00:01.23");
  assert.equal(L.formatStopwatch(61234), "01:01.23");
  assert.equal(L.formatStopwatch(3600000 + 61234), "1:01:01.23");
  assert.equal(L.formatStopwatch(-100), "00:00.00");
});

test("countdown math crosses zero exactly once", () => {
  assert.deepEqual(plain(L.countdownRemaining(10000, 4000)), { remaining: 6000, done: false });
  assert.deepEqual(plain(L.countdownRemaining(10000, 10000)), { remaining: 0, done: true });
  assert.deepEqual(plain(L.countdownRemaining(10000, 12000)), { remaining: 0, done: true });
  assert.deepEqual(plain(L.countdownRemaining(0, 5000)), { remaining: 0, done: false });
  assert.deepEqual(plain(L.countdownRemaining(NaN, 10)), { remaining: 0, done: false });
});

test("stopwatch elapsed banks paused time", () => {
  assert.equal(L.elapsedMs(0, 1000, 4500), 3500);
  assert.equal(L.elapsedMs(2000, 1000, 4500), 5500);
  assert.equal(L.elapsedMs(2000, null, 4500), 2000);
  assert.equal(L.elapsedMs(2000, undefined, 4500), 2000);
  assert.equal(L.elapsedMs(-5, 500, 400), 0); // garbage in, zero out
});

test("alarms normalize, sort and survive bad files", () => {
  const a = L.makeAlarm(7, 30, "a");
  assert.deepEqual(plain(a), { id: "a", hour: 7, minute: 30, enabled: true, firedDay: "" });
  assert.equal(L.makeAlarm(99, 99, "b").hour, 23);
  assert.equal(L.makeAlarm("x", "y", "c").hour, 7);

  assert.equal(L.normalizeAlarm(null), null);
  assert.equal(L.normalizeAlarm({ hour: 24, minute: 0 }), null);
  assert.equal(L.normalizeAlarm({ hour: 7, minute: 60 }), null);
  assert.equal(L.normalizeAlarm({ hour: "7", minute: "5" }).minute, 5);
  assert.equal(L.normalizeAlarm({ hour: 7, minute: 5 }).enabled, true);
  assert.equal(L.normalizeAlarm({ hour: 7, minute: 5, enabled: false }).enabled, false);
  assert.equal(L.normalizeAlarm({ hour: 7, minute: 5, firedDay: 5 }).firedDay, "");

  assert.equal(L.parseAlarms("[{\"hour\":9,\"minute\":0,\"id\":\"x\"},{\"hour\":1}]").length, 1);
  assert.equal(L.parseAlarms("not json").length, 0);
  assert.equal(L.parseAlarms("{}").length, 0);
  assert.equal(L.parseAlarms("").length, 0);

  const sorted = L.sortAlarms([{ hour: 9, minute: 5 }, { hour: 9, minute: 1 }, { hour: 6, minute: 0 }]);
  assert.deepEqual([...sorted].map((x) => L.alarmTimeText(x)), ["06:00", "09:01", "09:05"]);

  const list = L.addAlarm([{ id: "x", hour: 9, minute: 0, enabled: true, firedDay: "" }], L.makeAlarm(6, 15, "y"));
  assert.deepEqual([...list].map((x) => x.id), ["y", "x"]);
  assert.equal(L.addAlarm([], null).length, 0);

  assert.deepEqual([...L.removeAlarm(list, "y")].map((x) => x.id), ["x"]);
  assert.equal(L.setEnabled(list, "x", false)[1].enabled, false);
  assert.equal(list[1].enabled, true, "setEnabled must not mutate its input");
});

test("alarm rings once per day, only when enabled and matching the minute", () => {
  const alarm = L.makeAlarm(7, 30, "a");
  const now = at(2026, 11, 16, 7, 30, 5);
  assert.equal(L.alarmDue(alarm, now), true);
  assert.equal(L.alarmDue(alarm, at(2026, 11, 16, 7, 31)), false);
  assert.equal(L.alarmDue(alarm, at(2026, 11, 16, 19, 30)), false);
  assert.equal(L.alarmDue(L.setEnabled([alarm], "a", false)[0], now), false);
  assert.equal(L.alarmDue(null, now), false);

  const fired = L.markFired([alarm], "a", now);
  assert.equal(fired[0].firedDay, "2026-11-16");
  assert.equal(alarm.firedDay, "", "markFired must not mutate its input");
  assert.equal(L.alarmDue(fired[0], at(2026, 11, 16, 7, 30, 50)), false);
  assert.equal(L.alarmDue(fired[0], at(2026, 11, 17, 7, 30)), true);
});

test("next alarm picks the nearest enabled slot", () => {
  const alarms = [
    L.makeAlarm(7, 30, "morning"),
    L.makeAlarm(18, 0, "evening"),
    L.setEnabled([L.makeAlarm(9, 0, "off")], "off", false)[0],
  ];
  // 08:00 -> tonight 18:00
  const next = L.nextAlarm(alarms, at(2026, 11, 16, 8, 0));
  assert.equal(L.alarmTimeText(next), "18:00");
  assert.equal(L.nextAlarmText(alarms, at(2026, 11, 16, 8, 0)), "in 10 h");
  // 18:00:30 is still the 18:00 slot, not tomorrow's 07:30
  assert.equal(L.alarmTimeText(L.nextAlarm(alarms, at(2026, 11, 16, 18, 0, 30))), "18:00");
  assert.equal(L.nextAlarmText(alarms, at(2026, 11, 16, 18, 0, 30)), "in less than a minute");
  // after tonight's slot the morning alarm is next
  assert.equal(L.nextAlarmText(alarms, at(2026, 11, 16, 18, 2)), "in 13 h 28 min");
  assert.equal(L.nextAlarmLine(alarms, at(2026, 11, 16, 18, 2)),
    "Next alarm 07:30 · in 13 h 28 min");
  // once it rang today, the header counts down to tomorrow
  const onlyMorning = L.markFired([alarms[0]], "morning", at(2026, 11, 16, 7, 30));
  assert.equal(L.nextAlarmLine(onlyMorning, at(2026, 11, 16, 7, 30, 30)),
    "Next alarm 07:30 · in 23 h 59 min");
  // a fired alarm whose time is still ahead today keeps today's slot
  assert.equal(L.nextAlarmText(onlyMorning, at(2026, 11, 16, 6, 0)), "in 1 h 30 min");
  // disabled alarms are ignored entirely
  assert.equal(L.nextAlarm([alarms[2]], at(2026, 11, 16, 8, 0)), null);
  assert.equal(L.nextAlarmLine([alarms[2]], at(2026, 11, 16, 8, 0)), "");
  assert.equal(L.nextAlarm([], at(2026, 11, 16, 8, 0)), null);
});

test("array-like sequences are accepted (a QML var signal turns arrays into these)", () => {
  const alarm = L.makeAlarm(7, 30, "a");
  const arrayLike = { length: 1, 0: alarm };
  assert.equal(L.nextAlarmText(arrayLike, at(2026, 11, 16, 8, 0)), "in 23 h 30 min");
  assert.deepEqual(
    [...L.markFired(arrayLike, "a", at(2026, 11, 16, 7, 30))].map((x) => x.firedDay),
    ["2026-11-16"],
  );
  assert.equal(L.setEnabled(arrayLike, "a", false)[0].enabled, false);
  assert.equal(L.addAlarm(arrayLike, L.makeAlarm(6, 0, "b")).length, 2);
  assert.equal(L.removeAlarm(arrayLike, "a").length, 0);
  assert.equal(L.parseAlarms({ length: 1, 0: { hour: 9, minute: 0 } }).length, 1);
});
