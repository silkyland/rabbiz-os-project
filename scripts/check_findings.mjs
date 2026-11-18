// Validates finding front matter (docs/FINDINGS.md). No dependencies.
import { readdirSync, readFileSync, statSync, existsSync } from "node:fs";
import { join, relative } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const STATUS = ["hypothesis", "reproduced", "verified", "refuted"];
const LANES = ["de", "kernel", "userland", "docs"];
const REQUIRED = ["status", "lane", "device", "author", "ai_assisted", "date"];

function findingDirs() {
  const dirs = [join(root, "findings")];
  const dev = join(root, "devices");
  if (existsSync(dev)) {
    for (const v of readdirSync(dev)) {
      if (v === "_template") continue;
      const vp = join(dev, v);
      if (!statSync(vp).isDirectory()) continue;
      for (const c of readdirSync(vp)) dirs.push(join(vp, c, "findings"));
    }
  }
  return dirs.filter((d) => existsSync(d));
}

export function parse(text) {
  const m = text.match(/^---\n([\s\S]*?)\n---\n/);
  if (!m) return null;
  const out = {};
  for (const line of m[1].split("\n")) {
    const kv = line.match(/^([a-z_]+):\s*(.*?)(\s+#.*)?$/);
    if (kv) out[kv[1]] = kv[2];
  }
  out.__hasEvidence = /^evidence:\s*\n\s+- /m.test(m[1]);
  return out;
}

export function validate(fm, path) {
  const errs = [];
  if (!fm) return [`${path}: missing front matter`];
  for (const k of REQUIRED) if (!fm[k]) errs.push(`${path}: missing '${k}'`);
  if (fm.status && !STATUS.includes(fm.status)) errs.push(`${path}: bad status '${fm.status}'`);
  if (fm.lane && !LANES.includes(fm.lane)) errs.push(`${path}: bad lane '${fm.lane}'`);
  if (fm.status && fm.status !== "hypothesis" && !fm.__hasEvidence)
    errs.push(`${path}: status '${fm.status}' needs at least one evidence entry`);
  return errs;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const errs = [];
  let n = 0;
  for (const d of findingDirs()) {
    for (const f of readdirSync(d)) {
      if (!f.endsWith(".md") || f === "README.md") continue;
      n++;
      const p = join(d, f);
      errs.push(...validate(parse(readFileSync(p, "utf8")), relative(root, p)));
    }
  }
  if (errs.length) { console.error(errs.join("\n")); process.exit(1); }
  console.log(`findings ok (${n} checked)`);
}
