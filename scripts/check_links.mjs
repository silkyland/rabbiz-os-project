// Checks relative Markdown links resolve. PATHS=1 also checks `repo/paths` in backticks (noisy: docs
// legitimately cite upstream kernel paths like drivers/gnss/, so it is a one-off migration aid).
import { readFileSync, existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const files = execFileSync("git", ["ls-files", "*.md"], { cwd: root, encoding: "utf8" }).split("\n").filter(Boolean);
const PREFIXES = ["docs/", "devices/", "socs/", "scripts/", "drivers/", "patches/", "configs/", "tools/", "shell/", "rootfs-overlay/"];
const bad = [];
for (const f of files) {
  const text = readFileSync(join(root, f), "utf8").replace(/```[\s\S]*?```/g, "");
  for (const m of text.matchAll(/\]\(([^)\s]+)\)/g)) {
    const t = m[1].split("#")[0];
    if (!t || /^[a-z]+:/i.test(t) || !/[./]/.test(t)) continue;
    if (!existsSync(resolve(root, dirname(f), t))) bad.push(`${f}: link ${m[1]}`);
  }
  if (process.env.PATHS) for (const m of text.matchAll(/`((?:docs|devices|socs|scripts|drivers|patches|configs|tools|shell|apps|rootfs-overlay)\/[A-Za-z0-9_./-]+)`/g)) {
    const p = m[1].replace(/[.:]+$/, "");
    if (!existsSync(join(root, p))) bad.push(`${f}: path ${m[1]}`);
  }
}
if (bad.length) { console.error(bad.join("\n")); console.error(`${bad.length} broken`); process.exit(1); }
console.log(`links ok (${files.length} md files)`);
