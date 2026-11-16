// node --test tests/calc.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const raw = readFileSync(new URL("../apps/calculator/Calc.js", import.meta.url), "utf8");
// `.pragma library` is a QML directive, not JavaScript.
const src = raw.split("\n").filter((l) => !l.trimStart().startsWith(".pragma")).join("\n");
const ctx = { console };
vm.createContext(ctx);
vm.runInContext(src + "\nthis.Calc = { evaluate, format };", ctx);
const Calc = ctx.Calc;

const val = (e) => {
  const r = Calc.evaluate(e);
  assert.equal(r.ok, true, e + " should parse");
  return r.value;
};

test("basic arithmetic and precedence", () => {
  assert.equal(val("1+2"), 3);
  assert.equal(val("2+3*4"), 14);
  assert.equal(val("(2+3)*4"), 20);
  assert.equal(val("10/4"), 2.5);
  assert.equal(val("-3+1"), -2);
  assert.equal(val("2*-3"), -6);
});

test("percent applies to the preceding value", () => {
  assert.equal(val("200*10%"), 20);
  assert.equal(val("50+10%"), 50.1); // postfix percent: 10% -> 0.1
});

test("decimals", () => {
  assert.equal(val("0.1+0.2"), 0.30000000000000004);
  assert.equal(Calc.format(val("0.1+0.2")), "0.3");
});

test("errors", () => {
  assert.equal(Calc.evaluate("1/0").ok, false);
  assert.equal(Calc.evaluate("1+").ok, false);
  assert.equal(Calc.evaluate("(1+2").ok, false);
  assert.equal(Calc.evaluate("abc").ok, false);
  assert.equal(Calc.evaluate("").ok, false);
  assert.equal(Calc.evaluate("1..2").ok, false);
});

test("format keeps things readable", () => {
  assert.equal(Calc.format(4), "4");
  assert.equal(Calc.format(2.5), "2.5");
  assert.equal(Calc.format(1e13), "1.00000e+13");
});
