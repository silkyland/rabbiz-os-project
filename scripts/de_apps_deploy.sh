#!/usr/bin/env bash
# de_apps_deploy.sh — install the rabbiz-apps Rust binaries on the A21s.
#
#   ./scripts/de_apps_deploy.sh [options] [user@host]     (default root@172.16.42.1)
#
# The app family is pure-Rust Wayland now (tools/rabbiz-apps/crates/, one
# `rabbiz-<app>` binary per app crate — rabbiz-app-kit is a lib and produces
# no binary). The .desktop files Exec `rabbiz-<app>` and resolve these
# binaries through PATH, so they land at /usr/local/bin/ like the deleted
# quickshell wrappers did.
#
# Binary source (pick one — auto-detect is the default):
#   --build          build for the device first (arm64 container, see below)
#   --local          use the host's default target dir
#                    (tools/rabbiz-apps/target/release)
#   --bindir <dir>   use an explicit directory of prebuilt rabbiz-* binaries
#   (default)        first existing device build dir, in order:
#                      tools/rabbiz-apps/target-linux-arm64/aarch64-unknown-linux-gnu/release
#                      tools/rabbiz-apps/target-linux-arm64/release
#                    matching both the cross (`--target`) and native arm64
#                    container layouts that tools/rabbiz-sdk/emu tooling stages.
#
# --build uses the same arm64 container recipe as tools/rabbiz-de
# (docs/DE-PLAN.md): image `rabbiz-de-build:arm64` when present
# (docker build -f tools/rabbiz-de/Dockerfile -t rabbiz-de-build .), else
# rust:1.99-bookworm + an apt dep install; CARGO_TARGET_DIR is forced to
# tools/rabbiz-apps/target-linux-arm64 so it never collides with the host
# target/ fingerprint cache. Override the image with APPS_BUILD_IMAGE.
#
# Install mirrors de_deploy.sh: scp each binary to /tmp, then one remote
# script (fed through scripts/device_session.sh so deploys serialize on the
# shared device lock) does `install -m0755 -o root -g root`.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APPS_DIR="$REPO_ROOT/tools/rabbiz-apps"
HOST="root@172.16.42.1"
IMAGE="${APPS_BUILD_IMAGE:-rabbiz-de-build:arm64}"
FALLBACK_IMAGE="rust:1.99-bookworm"
mode="auto"
bindir=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --build)  mode="build"; shift ;;
    --local)  mode="local"; shift ;;
    --bindir) mode="bindir"; bindir="$2"; shift 2 ;;
    -h|--help) sed -n '2,33p' "$0"; exit 0 ;;
    *)        HOST="$1"; shift ;;
  esac
done

# --- resolve the binary directory -------------------------------------------
case "$mode" in
  build)
    if docker image inspect "$IMAGE" >/dev/null 2>&1; then
      build_image="$IMAGE"; pre=""
    else
      echo ">> $IMAGE not found — falling back to $FALLBACK_IMAGE + apt deps"
      build_image="$FALLBACK_IMAGE"
      pre="apt-get update -qq && apt-get install -y -qq \
        libseat-dev libinput-dev libgbm-dev libegl1-mesa-dev libdrm-dev \
        libpixman-1-dev libudev-dev libwayland-dev libxkbcommon-dev \
        pkg-config clang cmake && "
    fi
    echo ">> building tools/rabbiz-apps --workspace --release (linux/arm64, $build_image)"
    docker run --rm --platform linux/arm64 -v "$REPO_ROOT":/w -w /w/tools/rabbiz-apps \
      -e CARGO_TARGET_DIR=/w/tools/rabbiz-apps/target-linux-arm64 \
      "$build_image" bash -c \
      "${pre}cargo build --release --workspace && \
       chown -R $(id -u):$(id -g) /w/tools/rabbiz-apps/target-linux-arm64"
    bindir="$APPS_DIR/target-linux-arm64/release"
    ;;
  local)
    bindir="$APPS_DIR/target/release"
    ;;
  bindir)
    ;;
  auto)
    for d in "$APPS_DIR/target-linux-arm64/aarch64-unknown-linux-gnu/release" \
             "$APPS_DIR/target-linux-arm64/release"; do
      if compgen -G "$d/rabbiz-*" > /dev/null; then bindir="$d"; break; fi
    done
    ;;
esac

[[ -n "$bindir" && -d "$bindir" ]] || {
  echo "no prebuilt rabbiz-apps binaries found" >&2
  echo "  looked: tools/rabbiz-apps/target-linux-arm64/{aarch64-unknown-linux-gnu/,}release" >&2
  echo "  build:  ./scripts/de_apps_deploy.sh --build $HOST" >&2
  echo "  or:     cargo build --release --manifest-path tools/rabbiz-apps/Cargo.toml --workspace" >&2
  echo "          then ./scripts/de_apps_deploy.sh --local $HOST" >&2
  exit 2
}

# Collect app binaries only: drop dep-info *.d files and the app-kit lib.
bins=()
for f in "$bindir"/rabbiz-*; do
  [[ -f "$f" && -x "$f" ]] || continue
  case "$f" in *.d) continue ;; esac
  bins+=("$f")
done
[[ ${#bins[@]} -gt 0 ]] || { echo "no rabbiz-* binaries in $bindir" >&2; exit 2; }

# Sanity: the device is aarch64; refuse to ship host-arch binaries.
if [[ "${APPS_DEPLOY_SKIP_ARCH_CHECK:-0}" != "1" ]] && command -v file >/dev/null 2>&1; then
  desc="$(file -b "${bins[0]}")"
  case "$desc" in
    *ELF*aarch64*) ;;
    *) echo "ERROR: ${bins[0]} is not an aarch64 ELF ($desc)" >&2
       echo "       build for the device (--build) or set APPS_DEPLOY_SKIP_ARCH_CHECK=1" >&2
       exit 2 ;;
  esac
fi

echo ">> deploying ${#bins[@]} rabbiz-apps binaries from $bindir to $HOST:/usr/local/bin/"
tmp_remote=$(ssh "$HOST" 'umask 077 && mktemp -d /root/.deploy.XXXXXX')
REMOTE_SCRIPT=$(mktemp)
trap 'rm -f "$REMOTE_SCRIPT"; ssh -q "$HOST" "rm -rf -- \"\$1\"" _ "$tmp_remote" 2>/dev/null || true' EXIT
for f in "${bins[@]}"; do
  scp -q "$f" "$HOST:$tmp_remote/$(basename "$f").new"
  echo "   $(basename "$f")"
done

cat > "$REMOTE_SCRIPT" <<REMOTE
set -eu
STAGE='$tmp_remote'
trap 'rm -rf -- "\$STAGE"' EXIT
for new in "\$STAGE"/*.new; do
  name=\$(basename "\$new" .new)
  install -m 0755 -o root -g root "\$new" "/usr/local/bin/\$name"
done
rm -rf -- "\$STAGE"
REMOTE

"$REPO_ROOT/scripts/device_session.sh" "$HOST" < "$REMOTE_SCRIPT"

echo "de_apps_deploy: installed ${#bins[@]} binaries to /usr/local/bin on $HOST"
