#!/usr/bin/env node
// Enforces the shell's module dependency direction (docs/ARCHITECTURE.md "Shell seams").
// QML imports are static (`import A21s.X`), so a text scan is exact for them.
// NOT covered (stated gap): Loader{source:}/Qt.createComponent paths, runtime string imports.
// Usage: check_shell_seams.mjs [shellRoot]   (default: <repo>/shell)
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, sep, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = process.argv[2] ?? join(dirname(fileURLToPath(import.meta.url)), "..", "shell");

// module -> A21s modules it may import. Anything else is a violation.
// `apps` is legacy (deleted by M7.4 as base apps become real Linux apps).
const ALLOWED = {
  Theme: [],
  Services: ["Theme"],
  Widgets: ["Theme", "Services"],
  Windowing: ["Theme", "Services", "Widgets"],
  Desktop: ["Theme", "Services", "Widgets", "Windowing"],
  Panels: ["Theme", "Services", "Widgets", "Windowing"],
  apps: ["Theme", "Services", "Widgets", "Windowing"],
};

function* qmlFiles(dir) {
  for (const e of readdirSync(dir)) {
    const p = join(dir, e);
    if (statSync(p).isDirectory()) yield* qmlFiles(p);
    else if (e.endsWith(".qml")) yield p;
  }
}

function moduleOf(file) {
  const parts = relative(root, file).split(sep);
  if (parts[0] === "apps") return "apps";
  if (parts[0] === "A21s") return parts[1];
  return null; // shell.qml: wiring, may import anything
}

const errors = [];
for (const f of qmlFiles(root)) {
  const mod = moduleOf(f);
  if (mod === null) continue;
  if (!(mod in ALLOWED)) { errors.push(`${relative(root, f)}: directory "${mod}" has no seam rule — add it to ALLOWED and ARCHITECTURE.md`); continue; }
  readFileSync(f, "utf8").split("\n").forEach((line, i) => {
    const rel = line.match(/^\s*import\s+"(\.|\/)/);
    if (rel) errors.push(`${relative(root, f)}:${i + 1}: relative/path import; use a module import`);
    const m = line.match(/^\s*import\s+A21s\.(\w+)/);
    if (m && m[1] !== mod && !ALLOWED[mod].includes(m[1]))
      errors.push(`${relative(root, f)}:${i + 1}: ${mod} must not import A21s.${m[1]} (allowed: ${ALLOWED[mod].join(", ") || "none"})`);
  });
}
if (errors.length) { console.error(errors.join("\n")); process.exit(1); }
console.log("shell seams: ok");
