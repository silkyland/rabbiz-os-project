import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
const src = readFileSync(new URL("../A21s/Windowing/Gesture.js", import.meta.url), "utf8").replace(/^\.pragma library\s*$/m, "");
const box = { module: { exports: {} } }; vm.runInNewContext(src, box); const g = box.module.exports;

// home: long swipe, fast short flick, plain tap; not a tiny drag
assert.equal(g.endBottom(5, -150, -300, false), "home");
assert.equal(g.endBottom(0, -50, -1200, false), "home", "short fast flick");
assert.equal(g.endBottom(0, -50, -200, false), "none", "short slow drag does nothing");
assert.equal(g.endBottom(2, -3, 0, false), "home", "tap");
// hold = recents (fires during the drag), and release afterwards does not also go home
assert.equal(g.duringBottom(0, -120, g.HOLD_MS - 1), "none");
assert.equal(g.duringBottom(0, -120, g.HOLD_MS), "recents");
assert.equal(g.duringBottom(100, -100, 500), "none", "diagonal drag is not a hold");
assert.equal(g.endBottom(0, -150, -100, true), "none");
// horizontal app switch along the bar
assert.equal(g.endBottom(160, -10, 0, false), "prev");
assert.equal(g.endBottom(-160, 5, 0, false), "next");
assert.equal(g.endBottom(100, -90, 0, false), "none", "diagonal is neither switch nor home");
// back from the side edges
assert.equal(g.endEdge("left", 80), "back");
assert.equal(g.endEdge("right", -80), "back");
assert.equal(g.endEdge("left", 40), "none");
assert.equal(g.endEdge("left", -80), "none", "outward swipe is not back");
assert.ok(g.bottomProgress(-220) === 1 && g.bottomProgress(10) === 0);
console.log("gesture: ok");
