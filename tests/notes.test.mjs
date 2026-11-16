// node --test tests/notes.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const raw = readFileSync(new URL("../apps/notes/NotesLogic.js", import.meta.url), "utf8");
// `.pragma library` is a QML directive, not JavaScript.
const src = raw.split("\n").filter((l) => !l.trimStart().startsWith(".pragma")).join("\n");
// Run in this realm (wrapped in a function, so nothing leaks into the global object):
// deepStrictEqual compares prototypes, and objects built in a fresh vm context have a
// different Object.prototype than this module.
const Notes = vm.runInThisContext(
  "(function () {\n" +
    src +
    "\nreturn { TITLE_MAX, PREVIEW_MAX, isValidId, nextId, oneLine, firstLine, titleOf," +
    " previewOf, searchNotes, sortNotes, serialize, normalizeNote, parseNote, parseDump," +
    " formatEdited };\n})()",
  { filename: "NotesLogic.js" },
);

const ID = "1730000000000";
const note = (over = {}) => ({ id: ID, title: "", body: "", createdAt: 1730000000000, updatedAt: 1730000000000, ...over });

test("title falls back to the first non-empty body line", () => {
  assert.equal(Notes.titleOf(note({ title: "  Shopping  " })), "Shopping");
  assert.equal(Notes.titleOf(note({ title: "a\nb" })), "a b");
  assert.equal(Notes.titleOf(note({ body: "\n\n  Groceries for Saturday \n milk " })), "Groceries for Saturday");
  assert.equal(Notes.titleOf(note()), "Untitled note");
  assert.equal(Notes.titleOf(note({ title: "   ", body: "   \n  " })), "Untitled note");
  assert.equal(Notes.titleOf(note({ title: "รายการซื้อของ" })), "รายการซื้อของ");
  assert.equal(Notes.titleOf(null), "Untitled note");
});

test("long titles are capped", () => {
  const long = "x".repeat(400);
  const title = Notes.titleOf(note({ title: long }));
  assert.equal(title.length, Notes.TITLE_MAX);
  assert.ok(title.endsWith("\u2026"));
});

test("preview skips the line already used as the title", () => {
  assert.equal(Notes.previewOf(note({ body: "Title line\nSecond line\nThird" })), "Second line Third");
  assert.equal(Notes.previewOf(note({ body: "\n\nTitle line\n\nSecond" })), "Second");
  // With a typed title the whole body is available for the preview.
  assert.equal(Notes.previewOf(note({ title: "T", body: "Title line\nSecond" })), "Title line Second");
  assert.equal(Notes.previewOf(note({ body: "only one line" })), "");
  assert.equal(Notes.previewOf(note()), "");
});

test("long previews are capped", () => {
  const body = "first line\n" + "y".repeat(400);
  const preview = Notes.previewOf(note({ body }));
  assert.equal(preview.length, Notes.PREVIEW_MAX);
  assert.ok(preview.endsWith("\u2026"));
});

test("search matches title and body, case-insensitively", () => {
  const milk = note({ id: "1", title: "Groceries", body: "milk, eggs" });
  const work = note({ id: "2", title: "Work", body: "finish the Grocery list" });
  const thai = note({ id: "3", title: "รายการ", body: "ซื้อของ" });
  const list = [milk, work, thai];

  assert.deepEqual(Notes.searchNotes(list, "grocery").map((n) => n.id), ["2"]);
  assert.deepEqual(Notes.searchNotes(list, "  MiLk ").map((n) => n.id), ["1"]);
  assert.deepEqual(Notes.searchNotes(list, "ซื้อ").map((n) => n.id), ["3"]);
  assert.deepEqual(Notes.searchNotes(list, "zzz"), []);
  assert.deepEqual(Notes.searchNotes(list, "").map((n) => n.id), ["1", "2", "3"]);
  assert.deepEqual(Notes.searchNotes(list, "   ").map((n) => n.id), ["1", "2", "3"]);
});

test("search does not mutate or alias the model", () => {
  const list = [note({ id: "1", title: "A" })];
  const all = Notes.searchNotes(list, "");
  assert.notEqual(all, list);
  all.push(note({ id: "2", title: "B" }));
  assert.equal(list.length, 1);
});

test("serialise -> parse round-trips quotes, backslashes, newlines and Thai", () => {
  const original = note({
    title: 'He said "hi"',
    body: "line1\nline2\\end\n\tรายการ",
    createdAt: 1730000000000,
    updatedAt: 1730000123456,
  });
  const line = Notes.serialize(original);
  assert.equal(typeof line, "string");
  assert.ok(!line.includes("\n"), "a stored note must stay on one line");
  assert.deepEqual(Notes.parseNote(line), original);
});

test("parse rejects records that are not a valid note", () => {
  assert.equal(Notes.parseNote(""), null);
  assert.equal(Notes.parseNote("   "), null);
  assert.equal(Notes.parseNote("not json"), null);
  assert.equal(Notes.parseNote("{}"), null);
  assert.equal(Notes.parseNote("[1,2]"), null);
  assert.equal(Notes.parseNote("null"), null);
  assert.equal(Notes.parseNote('{"id":"../etc/passwd","body":"x"}'), null);
  assert.equal(Notes.parseNote('{"id":"12a","body":"x"}'), null);
  assert.equal(Notes.parseNote(null), null);
  assert.equal(Notes.parseNote('{"body":"x"}'), null);
});

test("parse normalises missing timestamps and ignores extra fields", () => {
  const parsed = Notes.parseNote('{"id":"1730000000000","body":"x","mood":"happy"}');
  assert.equal(parsed.id, "1730000000000");
  assert.equal(parsed.title, "");
  assert.equal(parsed.body, "x");
  assert.equal(parsed.createdAt, 1730000000000);
  assert.equal(parsed.updatedAt, parsed.createdAt);
  assert.equal(parsed.mood, undefined);

  const numeric = Notes.parseNote('{"id":1730000000000,"title":"T"}');
  assert.equal(numeric.id, "1730000000000");
});

test("dump parser skips corrupt lines, dedupes ids and sorts newest first", () => {
  const older = note({ id: "1000", updatedAt: 1000, body: "old" });
  const newer = note({ id: "1000", updatedAt: 2000, body: "new" });
  const other = note({ id: "3000", updatedAt: 3000, body: "other" });
  const dump = [
    Notes.serialize(older),
    "",
    "garbage",
    Notes.serialize(other),
    Notes.serialize(newer),
    "   ",
  ].join("\n");

  const parsed = Notes.parseDump(dump);
  assert.equal(parsed.length, 2);
  assert.deepEqual(parsed.map((n) => n.id), ["3000", "1000"]);
  assert.equal(parsed[1].body, "new");
});

test("dump parser tolerates an empty or missing directory dump", () => {
  assert.deepEqual(Notes.parseDump(""), []);
  assert.deepEqual(Notes.parseDump(undefined), []);
  assert.deepEqual(Notes.parseDump("\n\n"), []);
});

test("sortNotes is newest-first, copy-only and id-tiebroken", () => {
  const a = note({ id: "1", updatedAt: 10 });
  const b = note({ id: "3", updatedAt: 30 });
  const c = note({ id: "2", updatedAt: 30 });
  const input = [a, b, c];
  const sorted = Notes.sortNotes(input);
  assert.deepEqual(sorted.map((n) => n.id), ["3", "2", "1"]);
  assert.deepEqual(input.map((n) => n.id), ["1", "3", "2"], "input array untouched");
});

test("nextId is a timestamp string and never collides", () => {
  assert.equal(Notes.nextId([], 1730000000000), ID);
  assert.equal(typeof Notes.nextId([], 1730000000000), "string");
  const taken = [note({ id: ID }), note({ id: String(1730000000000 + 1) })];
  assert.equal(Notes.nextId(taken, 1730000000000), String(1730000000000 + 2));
  assert.equal(Notes.nextId(null, 1730000000000), ID);
});

test("isValidId only accepts digit strings", () => {
  assert.equal(Notes.isValidId(ID), true);
  assert.equal(Notes.isValidId("0"), true);
  assert.equal(Notes.isValidId(""), false);
  assert.equal(Notes.isValidId("12a"), false);
  assert.equal(Notes.isValidId("1/2"), false);
  assert.equal(Notes.isValidId("../x"), false);
  assert.equal(Notes.isValidId(Number(ID)), false);
  assert.equal(Notes.isValidId(null), false);
});

test("edited times read like a phone app", () => {
  const now = new Date(2026, 10, 16, 20, 0, 0).getTime();
  const at = (h, m = 0, d = 16, mo = 10, y = 2026) => new Date(y, mo, d, h, m, 0).getTime();

  assert.equal(Notes.formatEdited(now - 30 * 1000, now), "just now");
  assert.equal(Notes.formatEdited(now - 60 * 1000, now), "1 min ago");
  assert.equal(Notes.formatEdited(now - 5 * 60 * 1000, now), "5 mins ago");
  assert.equal(Notes.formatEdited(at(15), now), "5 hours ago");
  assert.equal(Notes.formatEdited(at(23, 0, 15), now), "Yesterday");
  assert.equal(Notes.formatEdited(at(9, 0, 13), now), "3 days ago");
  assert.equal(Notes.formatEdited(at(10, 0, 6), now), "6 Nov");
  assert.equal(Notes.formatEdited(at(10, 0, 6, 10, 2025), now), "6 Nov 2025");
  assert.equal(Notes.formatEdited(0, now), "");
  assert.equal(Notes.formatEdited("nope", now), "");
  assert.equal(Notes.formatEdited(now + 5 * 60 * 1000, now), "just now");
});

test("oneLine and firstLine behave", () => {
  assert.equal(Notes.oneLine("  a \n\t b  "), "a b");
  assert.equal(Notes.oneLine(null), "");
  assert.equal(Notes.firstLine("  \n\n  hello  \nbye"), "hello");
  assert.equal(Notes.firstLine(""), "");
});
