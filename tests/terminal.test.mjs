// node --test tests/terminal.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const raw = readFileSync(new URL("../apps/terminal/TerminalLogic.js", import.meta.url), "utf8");
// `.pragma library` is a QML directive, not JavaScript.
const src = raw.split("\n").filter((l) => !l.trimStart().startsWith(".pragma")).join("\n");
const ctx = { console };
vm.createContext(ctx);
vm.runInContext(
  src +
    "\nthis.Logic = { stripAnsi, cleanOutput, capOutput, addHistory, historyUp, historyDown," +
    " loadHistory, shortPath, formatPrompt, describeExit };",
  ctx
);
const L = ctx.Logic;

test("strips SGR colour and style codes", () => {
  assert.equal(L.stripAnsi("\u001b[31mred\u001b[0m"), "red");
  assert.equal(L.stripAnsi("\u001b[1;32mok\u001b[0m!"), "ok!");
  assert.equal(L.stripAnsi("\u001b[38;5;208morange\u001b[39m"), "orange");
  assert.equal(L.stripAnsi("plain"), "plain");
});

test("strips cursor moves, titles, hyperlinks and charset escapes", () => {
  assert.equal(L.stripAnsi("\u001b[2K\u001b[1Grebuild"), "rebuild");
  assert.equal(L.stripAnsi("\u001b]0;my title\u0007hello"), "hello");
  assert.equal(L.stripAnsi("\u001b]0;my title\u001b\\hello"), "hello");
  assert.equal(L.stripAnsi("\u001b]8;;file:///tmp\u001b\\link\u001b]8;;\u001b\\"), "link");
  assert.equal(L.stripAnsi("\u001b(B\u001b)0ascii"), "ascii");
  assert.equal(L.stripAnsi("\u001b7saved\u001b8"), "saved");
  assert.equal(L.stripAnsi("\u001b[?25lhidden\u001b[?25h"), "hidden");
});

test("carriage returns keep the last overwrite", () => {
  assert.equal(L.cleanOutput("10%\r20%\r100%\n"), "100%");
  assert.equal(L.cleanOutput("10%\r20%\r100%"), "100%");
  assert.equal(L.cleanOutput("done\r"), "done");
  assert.equal(L.cleanOutput("a\r\nb\r\n"), "a\nb");
});

test("backspaces delete like a terminal", () => {
  assert.equal(L.cleanOutput("abc\b\bX"), "aX");
  assert.equal(L.cleanOutput("abc\b"), "ab");
  assert.equal(L.cleanOutput("nope\b\b\b\b"), "");
});

test("cleanOutput keeps tabs and indentation, trims blank edges", () => {
  assert.equal(L.cleanOutput("\n\n  drwx\troot\n\n"), "  drwx\troot");
  assert.equal(L.cleanOutput("  hi  "), "  hi");
  assert.equal(L.cleanOutput("a\n\n\nb"), "a\n\n\nb");
  assert.equal(L.cleanOutput(""), "");
  assert.equal(L.cleanOutput(undefined), "");
});

test("cleanOutput drops other control characters", () => {
  assert.equal(L.cleanOutput("a\u0007b\u0001c"), "abc");
  assert.equal(L.cleanOutput("Thai: สวัสดี"), "Thai: สวัสดี");
});

test("capOutput leaves short output alone", () => {
  const r = L.capOutput("short", 100);
  assert.equal(r.text, "short");
  assert.equal(r.truncated, false);
  assert.equal(r.dropped, 0);
});

test("capOutput cuts long output", () => {
  const r = L.capOutput("x".repeat(200), 50);
  assert.equal(r.text.length, 50);
  assert.equal(r.truncated, true);
  assert.equal(r.dropped, 150);
  assert.equal(L.capOutput("abc", 0).truncated, false);
});

test("history ignores blanks and consecutive duplicates", () => {
  let h = [];
  h = L.addHistory(h, "ls", 10);
  h = L.addHistory(h, "   ", 10);
  h = L.addHistory(h, "ls", 10);
  h = L.addHistory(h, "pwd", 10);
  assert.deepEqual([...h], ["ls", "pwd"]);
  assert.deepEqual([...L.addHistory(h, "  df -h  ", 10)], ["ls", "pwd", "df -h"]);
});

test("history keeps only the newest limit entries", () => {
  let h = [];
  for (const c of ["a", "b", "c", "d"]) h = L.addHistory(h, c, 2);
  assert.deepEqual([...h], ["c", "d"]);
});

test("historyUp walks from newest to oldest and stops at the top", () => {
  const h = ["ls", "pwd", "uname -a"];
  let r = L.historyUp(h, -1);
  assert.equal(r.text, "uname -a");
  assert.equal(r.index, 2);
  r = L.historyUp(h, r.index);
  assert.equal(r.text, "pwd");
  r = L.historyUp(h, r.index);
  assert.equal(r.text, "ls");
  r = L.historyUp(h, r.index);
  assert.equal(r.text, "ls");
  assert.equal(r.moved, false);
  assert.deepEqual({ ...L.historyUp([], -1) }, { index: -1, text: null, moved: false });
});

test("historyDown walks back down and restores the draft", () => {
  const h = ["ls", "pwd"];
  let r = L.historyDown(h, 0, "draft");
  assert.equal(r.text, "pwd");
  assert.equal(r.index, 1);
  r = L.historyDown(h, 1, "draft");
  assert.equal(r.text, "draft");
  assert.equal(r.index, -1);
  assert.equal(r.moved, true);
  r = L.historyDown(h, -1, "draft");
  assert.equal(r.text, "draft");
  assert.equal(r.moved, false);
});

test("loadHistory tolerates corrupt files and keeps only useful strings", () => {
  assert.deepEqual([...L.loadHistory("not json")], []);
  assert.deepEqual([...L.loadHistory("{}")], []);
  assert.deepEqual([...L.loadHistory("")], []);
  assert.deepEqual([...L.loadHistory('["ls", "  pwd  ", 3, null, ""]')], ["ls", "pwd"]);
});

test("loadHistory keeps only the newest entries", () => {
  const many = JSON.stringify(Array.from({ length: 50 }, (_, i) => "cmd" + i));
  const h = L.loadHistory(many, 40);
  assert.equal(h.length, 40);
  assert.equal(h[0], "cmd10");
  assert.equal(h[39], "cmd49");
});

test("shortPath collapses the home directory", () => {
  assert.equal(L.shortPath("/home/rabbiz", "/home/rabbiz"), "~");
  assert.equal(L.shortPath("/home/rabbiz/", "/home/rabbiz"), "~");
  assert.equal(L.shortPath("/home/rabbiz/Music", "/home/rabbiz"), "~/Music");
  assert.equal(L.shortPath("/etc", "/home/rabbiz"), "/etc");
  assert.equal(L.shortPath("", "/home/rabbiz"), "~");
  assert.equal(L.shortPath("/home/rabbiz2", "/home/rabbiz"), "/home/rabbiz2");
});

test("formatPrompt is compact and stable", () => {
  assert.equal(L.formatPrompt("/home/rabbiz", "/home/rabbiz"), "~ $");
  assert.equal(L.formatPrompt("/home/rabbiz/Downloads", "/home/rabbiz"), "~/Downloads $");
});

test("describeExit explains shell codes", () => {
  assert.equal(L.describeExit(0, 20), "");
  assert.equal(L.describeExit(1, 20), "Exit 1");
  assert.equal(L.describeExit(124, 20), "Timed out after 20 s");
  assert.equal(L.describeExit(124, 0), "Timed out");
  assert.equal(L.describeExit(127, 20), "Command not found");
  assert.equal(L.describeExit(126, 20), "Not executable");
  assert.equal(L.describeExit(130, 20), "Stopped");
  assert.equal(L.describeExit(137, 20), "Killed by signal 9");
  assert.equal(L.describeExit(-1, 20), "Stopped");
  assert.equal(L.describeExit(undefined, 20), "");
});
