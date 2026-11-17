#!/usr/bin/env python3
"""Enforce the `rabbiz-de` crate dependency direction.

Rust's privacy does not stop one workspace crate from importing another, so
this is the seam check. It reads `cargo metadata` (no build) and fails when a
crate depends on something its row does not allow, or when the graph cycles.

Usage: python3 tools/rabbiz-de/scripts/check_seams.py [--manifest-path PATH]
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

# Lower layers first. A crate may only depend on the crates listed for it.
# Host-only crates (top block) build on macOS; the Linux/smithay crates
# (bottom block) build only in the arm64 container — same one-way rule.
ALLOWED: dict[str, set[str]] = {
    "de-api": set(),
    "de-design": {"de-api"},
    "de-paint": {"de-api"},
    "de-ui": {"de-api", "de-design"},
    "de-sim": {"de-api", "de-design", "de-paint", "de-ui"},
    "de-config": set(),
    "de-system": set(),
    "de-apps": {"de-system"},
    "de-backend": {"de-api"},
    "de-protocols": set(),
    "de-render": {"de-api", "de-paint"},
    "de-core": {"de-api", "de-backend", "de-protocols", "de-render", "de-paint", "de-ui", "de-config"},
    "de-bin": {"de-api", "de-apps", "de-backend", "de-core", "de-render", "de-system"},
}

# Non-workspace dependencies are ignored: only the seams between our crates
# are ours to police.
def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--manifest-path",
        default=str(Path(__file__).resolve().parents[1] / "Cargo.toml"),
    )
    args = parser.parse_args()

    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1",
         "--manifest-path", args.manifest_path],
        check=True, capture_output=True, text=True,
    ).stdout
    meta = json.loads(out)
    workspace = {pkg["name"] for pkg in meta["packages"]}

    graph: dict[str, set[str]] = {}
    for pkg in meta["packages"]:
        name = pkg["name"]
        if name not in ALLOWED:
            print(f"seams: unknown crate '{name}' - add a row to ALLOWED", file=sys.stderr)
            return 1
        deps = {
            dep["name"]
            for dep in pkg["dependencies"]
            if dep["name"] in workspace and dep.get("kind") is None
        }
        forbidden = deps - ALLOWED[name]
        if forbidden:
            print(
                f"seams: {name} may not import {sorted(forbidden)}; "
                f"allowed: {sorted(ALLOWED[name])}",
                file=sys.stderr,
            )
            return 1
        graph[name] = deps

    # Cycle check: a dependency graph that points backwards is a seam bug even
    # when both edges are individually allowed.
    visiting: set[str] = set()
    done: set[str] = set()

    def visit(node: str, trail: list[str]) -> None:
        if node in done:
            return
        if node in visiting:
            cycle = " -> ".join(trail + [node])
            raise SystemExit(f"seams: dependency cycle: {cycle}")
        visiting.add(node)
        for dep in graph.get(node, ()):
            visit(dep, trail + [node])
        visiting.remove(node)
        done.add(node)

    for crate in graph:
        visit(crate, [])

    print(f"seams ok: {len(graph)} crates, one-way")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
