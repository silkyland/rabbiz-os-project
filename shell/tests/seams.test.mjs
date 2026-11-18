// Proves the seam checker fires: pass fixture exits 0, fail fixture exits non-zero with both violations.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
const run = (d) => spawnSync("node", [new URL("../../scripts/check_shell_seams.mjs", import.meta.url).pathname, new URL(`./seams/${d}`, import.meta.url).pathname], { encoding: "utf8" });
assert.equal(run("pass").status, 0, run("pass").stderr);
const bad = run("fail");
assert.notEqual(bad.status, 0);
assert.match(bad.stderr, /Windowing must not import Rabbiz\.Desktop/);
assert.match(bad.stderr, /relative\/path import/);
console.log("seams checker fires: ok");
