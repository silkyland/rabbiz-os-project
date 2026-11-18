import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
// QML JS libraries start with `.pragma library`, which is not valid JS: strip it, run the rest.
const src = readFileSync(new URL("../Rabbiz/Windowing/LaunchFlow.js", import.meta.url), "utf8")
  .replace(/^\.pragma library\s*$/m, "");
const box = { module: { exports: {} } };
vm.runInNewContext(src, box);
const f = box.module.exports;

let s = f.create();
s = f.launch(s, "kalk", { x: 1, y: 2, w: 88, h: 88, radius: 24 }, 0);
assert.equal(s.state, "launching"); assert.ok(f.overlayVisible(s));
assert.equal(f.launch(s, "other", null, 5).appId, "kalk", "double tap ignored");
assert.equal(f.toplevelMapped(s, "other", 10).state, "launching", "wrong app does not map");
s = f.toplevelMapped(s, "kalk", 400); assert.equal(s.state, "mapped");
assert.equal(f.tick(s, 400 + f.MAPPED_HOLD_MS - 1).state, "mapped");
assert.equal(f.tick(s, 400 + f.MAPPED_HOLD_MS).state, "idle", "overlay released after first frame");
// crashed app: overlay must not stay forever
let c = f.launch(f.create(), "bad", null, 0);
assert.equal(f.tick(c, f.LAUNCH_TIMEOUT_MS - 1).state, "launching");
assert.equal(f.tick(c, f.LAUNCH_TIMEOUT_MS).state, "idle");
// close path
assert.equal(f.close(f.create(), 0).state, "idle", "nothing to close");
console.log("launchflow: ok");
// matching real desktop entries to toplevel appIds, no per-app table
assert.ok(f.matches("org.kde.kalk", "org.kde.kalk.desktop", ""));
assert.ok(f.matches("kalk", "org.kde.kalk", ""), "short id vs reverse-DNS");
assert.ok(f.matches("dolphin", "org.kde.dolphin", ""));
assert.ok(f.matches("Foo", "x.desktop", "foo"), "startup class");
assert.ok(!f.matches("org.kde.kclock", "org.kde.kalk", ""));
assert.ok(!f.matches("", "kalk", ""));
console.log("launchflow matches: ok");
// closing must time out too (review finding: it used to stay forever)
let k = f.close(f.toplevelMapped(f.launch(f.create(), "a", null, 0), "a", 1), 10);
assert.equal(k.state, "closing");
assert.equal(f.tick(k, 10 + f.CLOSE_MS - 1).state, "closing");
assert.equal(f.tick(k, 10 + f.CLOSE_MS).state, "idle", "closing releases the overlay");
console.log("launchflow closing: ok");
