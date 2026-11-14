#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LINUX_REPO="https://github.com/samsungexynos850/linux.git"
LINUX_COMMIT="14a425b3a"
UNILOADER_REPO="https://github.com/samsungexynos850/uniLoader.git"
UNILOADER_COMMIT="06a1bba"

CROSS_COMPILE="${CROSS_COMPILE:-}"
LLVM="${LLVM:-1}"
MAKE_TOOLCHAIN=(ARCH=arm64)
if [[ -n "$CROSS_COMPILE" ]]; then
  MAKE_TOOLCHAIN+=(CROSS_COMPILE="$CROSS_COMPILE")
else
  MAKE_TOOLCHAIN+=(LLVM="$LLVM")
fi

mkdir -p "$ROOT_DIR/src" "$ROOT_DIR/build/linux"

if [[ ! -d "$ROOT_DIR/src/linux/.git" ]]; then
  echo "[*] Cloning samsungexynos850/linux (shallow fetch for $LINUX_COMMIT)..."
  git clone --depth 1 --branch staging "$LINUX_REPO" "$ROOT_DIR/src/linux"
  if ! git -C "$ROOT_DIR/src/linux" rev-parse --verify "${LINUX_COMMIT}^{commit}" >/dev/null 2>&1; then
    git -C "$ROOT_DIR/src/linux" fetch --depth 50 origin "$LINUX_COMMIT"
  fi
  git -C "$ROOT_DIR/src/linux" checkout "$LINUX_COMMIT"
else
  echo "[*] src/linux already exists ($(git -C "$ROOT_DIR/src/linux" rev-parse --short HEAD))."
fi

for patch in "$ROOT_DIR"/patches/linux/*.patch; do
  [[ -e "$patch" ]] || continue
  if git -C "$ROOT_DIR/src/linux" apply --reverse --check "$patch" >/dev/null 2>&1; then
    echo "[*] Patch already applied: $(basename "$patch")"
  else
    echo "[*] Applying patch: $(basename "$patch")"
    git -C "$ROOT_DIR/src/linux" apply "$patch"
  fi
done

if [[ ! -d "$ROOT_DIR/src/uniLoader/.git" ]]; then
  echo "[*] Cloning samsungexynos850/uniLoader ($UNILOADER_COMMIT)..."
  git clone --depth 1 "$UNILOADER_REPO" "$ROOT_DIR/src/uniLoader"
  git -C "$ROOT_DIR/src/uniLoader" fetch --depth 20 origin "$UNILOADER_COMMIT" || true
  git -C "$ROOT_DIR/src/uniLoader" checkout "$UNILOADER_COMMIT" || true
else
  echo "[*] src/uniLoader already exists ($(git -C "$ROOT_DIR/src/uniLoader" rev-parse --short HEAD))."
fi

if [[ ! -f "$ROOT_DIR/build/linux/.config" ]]; then
  echo "[*] Installing configs/a21s_linux_defconfig -> build/linux/.config"
  cp "$ROOT_DIR/configs/a21s_linux_defconfig" "$ROOT_DIR/build/linux/.config"
  make -C "$ROOT_DIR/src/linux" O=../../build/linux "${MAKE_TOOLCHAIN[@]}" olddefconfig
else
  echo "[*] build/linux/.config already exists (leaving untouched)."
fi

echo "[*] Preparing kernel build tree (modules_prepare)..."
make -C "$ROOT_DIR/src/linux" O=../../build/linux "${MAKE_TOOLCHAIN[@]}" -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu)" modules_prepare

echo "[✓] Build environment ready! See docs/BUILD.md for next steps."
