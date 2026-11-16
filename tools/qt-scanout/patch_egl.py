#!/usr/bin/env python3
"""Build a private, version-pinned Mesa EGL scanout experiment; never edit Mesa in place."""

import argparse
import hashlib
import json
from pathlib import Path

MESA_SHA256 = "539e44a926ddfc5ba562a11e07d18205fec08a08c57c3cebe5f3070e6082b789"
# Mesa 26.2.6-1 aarch64 get_back_bo(): SHARE|BACKBUFFER -> add SCANOUT.
# ELF executable LOAD segment has equal file offsets and virtual addresses.
PATCHES = ((0x2D518, 0x52800236, 0x52800276), (0x2D864, 0x52800236, 0x52800276))


def patch_library(original: bytes) -> bytes:
    if hashlib.sha256(original).hexdigest() != MESA_SHA256:
        raise ValueError("Unsupported Mesa binary; re-verify disassembly before porting this patch")
    patched = bytearray(original)
    for offset, expected, replacement in PATCHES:
        if original[offset : offset + 4] != expected.to_bytes(4, "little"):
            raise ValueError(f"Unexpected instruction at {offset:#x}")
        patched[offset : offset + 4] = replacement.to_bytes(4, "little")
    return bytes(patched)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    patched = patch_library(args.source.read_bytes())
    destination = args.destination.resolve()
    if destination == args.source.resolve().parent:
        parser.error("Destination must be a private directory outside the system Mesa directory")
    destination.mkdir(parents=True, exist_ok=True)
    library = destination / "libEGL_mesa.so.0"
    library.write_bytes(patched)
    (destination / "mesa.json").write_text(json.dumps({
        "file_format_version": "1.0.0", "ICD": {"library_path": str(library)}
    }) + "\n")
    (destination / "mesa.sha256").write_text(MESA_SHA256 + "\n")
    (destination / "patched.sha256").write_text(hashlib.sha256(patched).hexdigest() + "\n")


if __name__ == "__main__":
    main()
