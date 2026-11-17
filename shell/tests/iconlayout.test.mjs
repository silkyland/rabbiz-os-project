// IconLayout: the ~/Desktop + dock icon model (membership, order, overrides).
// Runs the real JS on node — no Qt, no device. The QML side (DesktopLayout.qml) only
// adds persistence + entry resolution, so ordering bugs are caught here.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const src = readFileSync(new URL("../A21s/Desktop/IconLayout.js", import.meta.url), "utf8")
    .replace(/^\.pragma library\s*$/m, "");
// Run in this realm (not vm): the model is plain data, and same-realm arrays keep
// deepEqual exact instead of failing on cross-realm prototypes.
const mod = {};
new Function("module", src)(mod);
const L = mod.exports;

// ---- seeds & defaults -------------------------------------------------------
const seedIds = (list) => list.flatMap((s) => s.ids);
assert.equal(L.DOCK_SEED.length, 4, "dock ships with four favourites");
// No two seeds may claim the same entry id, or the default layout would be ambiguous.
const claimed = new Set();
for (const seed of L.DESKTOP_SEED.concat(L.DOCK_SEED)) {
    assert.ok(seed.ids.length > 0 && seed.name && seed.icon, `seed ${seed.id} is complete`);
    for (const id of seed.ids) {
        assert.equal(claimed.has(id), false, `${id} claimed by two seeds`);
        claimed.add(id);
    }
}
assert.equal(L.seedFor(L.DESKTOP_SEED, "org.kde.index").id, "a21s-files", "alias finds its seed");
assert.equal(L.seedFor(L.DESKTOP_SEED, "a21s-files").id, "a21s-files", "placeholder id finds its seed");
assert.equal(L.seedFor(L.DESKTOP_SEED, "nope"), null);

// ---- parse: garbage in, sane state out --------------------------------------
assert.deepEqual(L.parse(""), L.emptyState());
assert.deepEqual(L.parse("   "), L.emptyState());
assert.deepEqual(L.parse(null), L.emptyState());
assert.deepEqual(L.parse("{broken"), L.emptyState());
assert.deepEqual(L.parse("[]"), L.emptyState(), "array is not a layout document");
assert.deepEqual(L.parse('{"foo":1}'), L.emptyState(), "no surface keys");
assert.equal(L.tryParse('{"desktop":[],"dock":[]}').desktop.length, 0, "explicit empty is valid");
assert.equal(L.tryParse("{broken"), null, "corrupt file is distinguishable from empty");

// dedupe + caps + id cleanup
const dirty = L.parse(JSON.stringify({
    desktop: ["a", "a", "  b  ", "", null, 42, "c"],
    dock: ["d1", "d2", "d3", "d4", "d5", "d6", "d7"],
    overrides: {
        a: { label: "  Nice  ", icon: " star " },
        b: { label: "" },                     // empty label -> no override left
        c: { label: "x".repeat(60) },         // too long -> ignored
        d: { icon: "42" },
        e: "not-an-object"
    }
}));
assert.deepEqual(dirty.desktop, ["a", "b", "42", "c"], "deduped, trimmed, coerced, ordered");
assert.equal(dirty.dock.length, L.DOCK_MAX, "dock capped");
assert.deepEqual(dirty.overrides.a, { label: "Nice", icon: "star" });
assert.equal(dirty.overrides.b, undefined, "empty label drops the override");
assert.equal(dirty.overrides.c, undefined, "over-long label is not trusted");
assert.deepEqual(dirty.overrides.d, { icon: "42" });
assert.equal(dirty.overrides.e, undefined);

// ---- add / remove ------------------------------------------------------------
let s = L.createState(["a", "b"], ["d"], {});
assert.deepEqual(L.add(s, "desktop", "c").desktop, ["a", "b", "c"], "append");
assert.deepEqual(L.add(s, "desktop", "c", 0).desktop, ["c", "a", "b"], "insert at index");
assert.deepEqual(L.add(s, "desktop", "a").desktop, ["a", "b"], "idempotent");
assert.deepEqual(L.add(s, "desktop", "").desktop, ["a", "b"], "empty id ignored");
assert.deepEqual(L.remove(s, "desktop", "a").desktop, ["b"]);
assert.deepEqual(L.remove(s, "desktop", "zz").desktop, ["a", "b"], "removing a stranger is a no-op");
assert.deepEqual(s.desktop, ["a", "b"], "operations never mutate the input state");

// dock capacity
let full = L.createState([], ["d1", "d2", "d3", "d4", "d5"], {});
assert.equal(L.isFull(full, "dock"), true);
assert.deepEqual(L.add(full, "dock", "d6").dock, ["d1", "d2", "d3", "d4", "d5"], "full dock refuses");
assert.deepEqual(L.transfer(full, "desktop", "x", "dock").dock.length, 5, "transfer into a full dock refuses");

// ---- move (final-index semantics) --------------------------------------------
const four = L.createState(["a", "b", "c", "d"], [], {});
assert.deepEqual(L.move(four, "desktop", 0, 3).desktop, ["b", "c", "d", "a"], "drag first onto last");
assert.deepEqual(L.move(four, "desktop", 3, 0).desktop, ["d", "a", "b", "c"], "drag last onto first");
assert.deepEqual(L.move(four, "desktop", 1, 2).desktop, ["a", "c", "b", "d"], "neighbour swap");
assert.deepEqual(L.move(four, "desktop", 2, 2).desktop, ["a", "b", "c", "d"], "same index is a no-op");
assert.deepEqual(L.move(four, "desktop", 1, 99).desktop, ["a", "c", "d", "b"], "target clamped to the end");
assert.deepEqual(L.move(four, "desktop", 1, -5).desktop, ["b", "a", "c", "d"], "target clamped to the start");
assert.deepEqual(L.move(four, "desktop", 9, 1).desktop, ["a", "b", "c", "d"], "bad source is ignored");
assert.deepEqual(L.move(four, "desktop", 0.7, 2.2).desktop, ["b", "c", "a", "d"], "fractional indices floored");

// ---- transfer between surfaces ----------------------------------------------
const both = L.createState(["a", "b", "c"], ["d"], {});
assert.deepEqual(L.transfer(both, "desktop", "a", "dock").dock, ["d", "a"], "append to dock");
assert.deepEqual(L.transfer(both, "desktop", "a", "dock").desktop, ["b", "c"], "removed from source");
assert.deepEqual(L.transfer(both, "desktop", "c", "dock", 0).dock, ["c", "d"], "insert at dock start");
assert.deepEqual(L.transfer(both, "dock", "d", "desktop", 1).desktop, ["a", "d", "b", "c"], "dock -> desktop slot");
assert.deepEqual(L.transfer(both, "dock", "d", "desktop").desktop, ["a", "b", "c", "d"], "append to desktop");
// already on the target: the source copy disappears, target keeps its place
const dup = L.createState(["a", "b"], ["a", "z"], {});
const deduped = L.transfer(dup, "desktop", "a", "dock", 0);
assert.deepEqual(deduped.desktop, ["b"]);
assert.deepEqual(deduped.dock, ["a", "z"], "existing dock order wins");
// same surface = reorder
assert.deepEqual(L.transfer(four, "desktop", "b", "desktop", 3).desktop, ["a", "c", "d", "b"]);

// ---- overrides ---------------------------------------------------------------
let o = L.createState(["a"], [], {});
o = L.setOverride(o, "a", { label: "Calc" });
assert.equal(L.labelFor(o, "a", "Calculator"), "Calc");
assert.equal(L.iconFor(o, "a", "calculate"), "calculate", "label-only override keeps the icon");
o = L.setOverride(o, "a", { icon: "functions" });
assert.equal(L.iconFor(o, "a", "calculate"), "functions");
o = L.setOverride(o, "a", { label: "" });
assert.equal(L.labelFor(o, "a", "Calculator"), "Calculator", "empty label restores the default");
assert.deepEqual(o.overrides.a, { icon: "functions" });
o = L.clearOverride(o, "a");
assert.equal(L.overrideFor(o, "a"), null);
assert.equal(L.labelFor(o, "unknown", "Fallback"), "Fallback");
// override of a removed icon survives (re-adding restores the user's naming)
let keep = L.setOverride(L.createState(["a"], [], {}), "a", { label: "Mine" });
keep = L.remove(keep, "desktop", "a");
assert.equal(L.labelFor(keep, "a", "x"), "Mine");

// ---- serialize round-trip ----------------------------------------------------
const doc = L.createState(["a", "b"], ["c"], { a: { label: "A", icon: "star" } });
const text = L.serialize(doc);
assert.ok(text.endsWith("\n"));
assert.deepEqual(L.parse(text), L.normalize(doc), "serialize -> parse is lossless");
assert.equal(L.parse(text).desktop[1], "b");
assert.deepEqual(JSON.parse(text).overrides.a, { label: "A", icon: "star" });
assert.equal(L.serialize(L.parse(text)), text, "stable serialization (no watcher churn)");

// ---- prettyId ----------------------------------------------------------------
assert.equal(L.prettyId("org.kde.some-app.desktop"), "Some app");
assert.equal(L.prettyId("a21s-sms"), "A21s sms");
assert.equal(L.prettyId(""), "");
assert.equal(L.prettyId("steam"), "Steam");
assert.equal(L.prettyId("org.kde.kweather"), "Kweather");

console.log("iconlayout: ok");
