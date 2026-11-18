import assert from "node:assert/strict";
import { parse, validate } from "../scripts/check_findings.mjs";

const good = `---
status: verified
lane: kernel
device: samsung/a21s
author: x
ai_assisted: false
date: 2026-11-18
evidence:
  - cmd: "dmesg"
---
# t
`;
assert.deepEqual(validate(parse(good), "good.md"), []);
assert.ok(validate(parse(good.replace(/evidence:\n  - cmd: "dmesg"\n/, "")), "x.md")[0].includes("evidence"));
assert.ok(validate(parse(good.replace("verified", "proven")), "x.md")[0].includes("bad status"));
assert.ok(validate(parse(good.replace("lane: kernel\n", "")), "x.md")[0].includes("lane"));
assert.deepEqual(validate(parse(good.replace("verified", "hypothesis").replace(/evidence:\n  - cmd: "dmesg"\n/, "")), "h.md"), []);
assert.deepEqual(validate(null, "n.md"), ["n.md: missing front matter"]);
console.log("findings test ok");
