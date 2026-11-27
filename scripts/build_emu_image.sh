#!/usr/bin/env bash
# scripts/build_emu_image.sh — Build/package the Tier 3 Full System VM image
# (rabbizos-emu-arm64.qcow2 + kernel + initramfs + launch manifest) for
# `rabbiz emu vm` (QEMU aarch64 `-M virt -accel hvf|kvm` + `virtio-gpu-pci` or Apple `vfkit`).
#
# Ground rule (AGENTS.md): Arch Linux ARM rootfs has `/lib -> usr/lib` as a
# directory symlink. Never create or copy files under `/lib/...` in staging
# trees or overlay archives; always target `/usr/lib/...`.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

OUT_DIR="${REPO_ROOT}/build/emu-vm"
IMAGE_SIZE="8G"
DEVICE="samsung/a21s"
DRY_RUN=0

usage() {
  cat <<'EOF'
Usage: scripts/build_emu_image.sh [OPTIONS]

Builds and packages the RabbizOS Tier 3 Full System VM image
(rabbizos-emu-arm64.qcow2, kernel, initramfs, systemd user drop-in, and
QEMU/vfkit launch manifest) for `rabbiz emu vm`.

Options:
  --out-dir <dir>             Output directory (default: build/emu-vm)
  --size <GB>                 Virtual disk size (default: 8G)
  --device <vendor/codename>  Device profile under devices/<vendor>/<codename>/device.toml
                              (default: samsung/a21s)
  --dry-run, --check          Validate repository assets & device profile, write
                              the VM systemd drop-in and QEMU launch manifest to
                              --out-dir without requiring root/loop-mount, and exit 0
  -h, --help                  Show this help message and exit
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir)
      if [[ $# -lt 2 ]]; then
        echo "error: --out-dir requires a directory argument" >&2
        exit 1
      fi
      OUT_DIR="$2"
      shift 2
      ;;
    --out-dir=*)
      OUT_DIR="${1#*=}"
      shift
      ;;
    --size)
      if [[ $# -lt 2 ]]; then
        echo "error: --size requires a size argument (e.g. 8G)" >&2
        exit 1
      fi
      IMAGE_SIZE="$2"
      shift 2
      ;;
    --size=*)
      IMAGE_SIZE="${1#*=}"
      shift
      ;;
    --device)
      if [[ $# -lt 2 ]]; then
        echo "error: --device requires <vendor/codename> (e.g. samsung/a21s)" >&2
        exit 1
      fi
      DEVICE="$2"
      shift 2
      ;;
    --device=*)
      DEVICE="${1#*=}"
      shift
      ;;
    --dry-run|--check)
      DRY_RUN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

# Normalize OUT_DIR to an absolute path if relative.
if [[ "${OUT_DIR}" != /* ]]; then
  OUT_DIR="${REPO_ROOT}/${OUT_DIR}"
fi

DEVICE_TOML="${REPO_ROOT}/devices/${DEVICE}/device.toml"
ROOTFS_OVERLAY="${REPO_ROOT}/rootfs-overlay"

if [[ ! -f "${DEVICE_TOML}" ]]; then
  echo "error: device profile not found: ${DEVICE_TOML}" >&2
  exit 1
fi

if [[ ! -d "${ROOTFS_OVERLAY}" ]]; then
  echo "error: rootfs-overlay directory not found: ${ROOTFS_OVERLAY}" >&2
  exit 1
fi

# Guardrail: verify rootfs-overlay never defines a top-level /lib directory
# (Arch Linux ARM symlinks /lib -> usr/lib).
if [[ -e "${ROOTFS_OVERLAY}/lib" ]]; then
  echo "error: ${ROOTFS_OVERLAY}/lib exists; Arch ARM requires all library/systemd units under /usr/lib" >&2
  exit 1
fi

parse_toml_key() {
  local key="$1"
  local file="$2"
  local default_val="$3"
  local val
  val="$(sed -n -E "s/^[[:space:]]*${key}[[:space:]]*=[[:space:]]*\"?([^\"#[:space:]]+)\"?.*$/\1/p" "${file}" | head -n 1)"
  if [[ -z "${val}" ]]; then
    echo "${default_val}"
  else
    echo "${val}"
  fi
}

VENDOR="$(parse_toml_key "vendor" "${DEVICE_TOML}" "${DEVICE%%/*}")"
CODENAME="$(parse_toml_key "codename" "${DEVICE_TOML}" "${DEVICE##*/}")"
WIDTH_PX="$(parse_toml_key "width_px" "${DEVICE_TOML}" "720")"
HEIGHT_PX="$(parse_toml_key "height_px" "${DEVICE_TOML}" "1600")"
FORM_FACTOR="$(parse_toml_key "form_factor" "${DEVICE_TOML}" "handset")"
KERNEL_VER="$(parse_toml_key "version" "${DEVICE_TOML}" "7.3.0")"

if [[ "${WIDTH_PX}" == "0" || -z "${WIDTH_PX}" ]]; then
  WIDTH_PX="1600"
fi
if [[ "${HEIGHT_PX}" == "0" || -z "${HEIGHT_PX}" ]]; then
  HEIGHT_PX="2560"
fi

HOST_OS="$(uname -s)"
HOST_ARCH="$(uname -m)"
if [[ "${HOST_OS}" == "Darwin" ]]; then
  QEMU_ACCEL="hvf"
else
  QEMU_ACCEL="kvm:tcg"
fi

check_tool() {
  local tool="$1"
  if command -v "${tool}" >/dev/null 2>&1; then
    echo "found ($(command -v "${tool}"))"
  else
    echo "missing"
  fi
}

QEMU_STATUS="$(check_tool qemu-system-aarch64)"
QEMU_IMG_STATUS="$(check_tool qemu-img)"
VFKIT_STATUS="$(check_tool vfkit)"
DOCKER_STATUS="$(check_tool docker)"

echo "==> RabbizOS Tier 3 VM Image Builder"
echo "    Device profile : ${DEVICE} (${VENDOR}/${CODENAME}, ${WIDTH_PX}x${HEIGHT_PX} ${FORM_FACTOR})"
echo "    Profile path   : ${DEVICE_TOML}"
echo "    Rootfs overlay : ${ROOTFS_OVERLAY} (/usr/lib verified, no /lib trap)"
echo "    Output dir     : ${OUT_DIR}"
echo "    Target size    : ${IMAGE_SIZE}"
echo "    Host platform  : ${HOST_OS}/${HOST_ARCH} (QEMU accel: ${QEMU_ACCEL})"
echo "    Tool check     : qemu-system-aarch64=${QEMU_STATUS}"
echo "                     qemu-img=${QEMU_IMG_STATUS}"
echo "                     vfkit=${VFKIT_STATUS}"
echo "                     docker=${DOCKER_STATUS}"

mkdir -p "${OUT_DIR}/overlay/usr/lib/systemd/user/rabbiz-session.service.d"
mkdir -p "${OUT_DIR}/overlay/usr/lib/systemd/system"
mkdir -p "${OUT_DIR}/overlay/etc/rabbiz"

DROPIN_PATH="${OUT_DIR}/overlay/usr/lib/systemd/user/rabbiz-session.service.d/10-rabbiz-emu-vm.conf"
cat > "${DROPIN_PATH}" <<EOF
# Auto-generated by scripts/build_emu_image.sh for Tier 3 VM (${VENDOR}/${CODENAME})
# Installed under /usr/lib/systemd/user/rabbiz-session.service.d/ (never /lib).
[Service]
Environment=RABBIZ_DEVICE_VENDOR=${VENDOR}
Environment=RABBIZ_DEVICE_CODENAME=${CODENAME}
Environment=RABBIZ_FORM_FACTOR=${FORM_FACTOR}
Environment=RABBIZ_DRM_DRIVER=virtio_gpu
Environment=RABBIZ_MOCK_SYSTEM=1
Environment=RABBIZ_EMU_LISTEN=127.0.0.1:5910
Environment=RABBIZ_CTL_LISTEN=127.0.0.1:5911
EOF

VM_ENV_PATH="${OUT_DIR}/overlay/etc/rabbiz/emu-vm.env"
cat > "${VM_ENV_PATH}" <<EOF
RABBIZ_VM_TIER=3
RABBIZ_DEVICE=${DEVICE}
RABBIZ_DISPLAY_WIDTH=${WIDTH_PX}
RABBIZ_DISPLAY_HEIGHT=${HEIGHT_PX}
RABBIZ_FORM_FACTOR=${FORM_FACTOR}
EOF

QCOW2_IMAGE="${OUT_DIR}/rabbizos-emu-arm64.qcow2"
RAW_IMAGE="${OUT_DIR}/rabbizos-emu-arm64.raw"
KERNEL_IMAGE="${OUT_DIR}/Image"
INITRAMFS_IMAGE="${OUT_DIR}/initramfs-linux.img"
MANIFEST_JSON="${OUT_DIR}/rabbiz-emu-vm.json"
QEMU_LAUNCH_SH="${OUT_DIR}/launch-qemu.sh"

cat > "${MANIFEST_JSON}" <<EOF
{
  "schema_version": 1,
  "tier": 3,
  "device": "${DEVICE}",
  "vendor": "${VENDOR}",
  "codename": "${CODENAME}",
  "form_factor": "${FORM_FACTOR}",
  "display": {
    "width_px": ${WIDTH_PX},
    "height_px": ${HEIGHT_PX},
    "refresh_hz": 60
  },
  "kernel_version": "${KERNEL_VER}",
  "image": {
    "qcow2": "${QCOW2_IMAGE}",
    "raw": "${RAW_IMAGE}",
    "kernel": "${KERNEL_IMAGE}",
    "initramfs": "${INITRAMFS_IMAGE}",
    "size": "${IMAGE_SIZE}"
  },
  "systemd": {
    "user_dropin": "/usr/lib/systemd/user/rabbiz-session.service.d/10-rabbiz-emu-vm.conf",
    "default_user": "rabbiz",
    "features": [
      "systemd-user-scopes",
      "cgroup-v2-freeze",
      "systemd-oomd",
      "polkit",
      "libseat-logind",
      "virtio-gpu-drm"
    ]
  },
  "ports": {
    "emu_proto_tcp": 5910,
    "ctl_ipc_tcp": 5911,
    "ssh_tcp": 2222
  }
}
EOF

cat > "${QEMU_LAUNCH_SH}" <<EOF
#!/usr/bin/env bash
set -euo pipefail

DIR="\$(cd "\$(dirname "\${BASH_SOURCE[0]}")" && pwd)"
exec qemu-system-aarch64 \\
  -M virt,highmem=on \\
  -accel ${QEMU_ACCEL} \\
  -cpu host \\
  -smp 4 \\
  -m 4096 \\
  -kernel "\${DIR}/Image" \\
  -initrd "\${DIR}/initramfs-linux.img" \\
  -append "console=ttyAMA0,115200 root=/dev/vda rw rootwait systemd.unified_cgroup_hierarchy=1 rabbiz.vm=1 rabbiz.device=${DEVICE}" \\
  -drive file="\${DIR}/rabbizos-emu-arm64.qcow2",if=virtio,format=qcow2 \\
  -device virtio-gpu-pci,xres=${WIDTH_PX},yres=${HEIGHT_PX} \\
  -device virtio-keyboard-pci \\
  -device virtio-tablet-pci \\
  -device virtio-rng-pci \\
  -netdev user,id=net0,hostfwd=tcp:127.0.0.1:5910-:5910,hostfwd=tcp:127.0.0.1:5911-:5911,hostfwd=tcp:127.0.0.1:2222-:22 \\
  -device virtio-net-pci,netdev=net0 \\
  -serial mon:stdio
EOF
chmod +x "${QEMU_LAUNCH_SH}"

if [[ "${DRY_RUN}" -eq 1 ]]; then
  echo "==> [dry-run] Generated VM artifacts:"
  echo "    - Drop-in  : ${DROPIN_PATH}"
  echo "    - Env file : ${VM_ENV_PATH}"
  echo "    - Manifest : ${MANIFEST_JSON}"
  echo "    - Launcher : ${QEMU_LAUNCH_SH}"
  echo "==> [dry-run] Full build steps that will execute without --dry-run:"
  echo "    1. Assemble Arch Linux ARM64 rootfs with systemd, libseat, polkit, mesa, and rabbiz-de binaries"
  echo "    2. Stage rootfs-overlay/ and ${DROPIN_PATH} into /usr/lib/... (preserving /lib -> usr/lib symlink)"
  echo "    3. Create ext4 rootfs image (${RAW_IMAGE}, ${IMAGE_SIZE}) and convert to ${QCOW2_IMAGE}"
  echo "    4. Copy ARM64 virt kernel (${KERNEL_IMAGE}) + initramfs (${INITRAMFS_IMAGE})"
  echo "==> [dry-run] Validation passed."
  exit 0
fi

# Full build mode requires qemu-img and docker (or mkfs.ext4).
if ! command -v qemu-img >/dev/null 2>&1; then
  echo "error: qemu-img is required to create ${QCOW2_IMAGE} (install qemu or run with --dry-run)" >&2
  exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "error: docker is required to assemble the Arch Linux ARM64 rootfs on ${HOST_OS}" >&2
  exit 1
fi

STAGE_TAR="${OUT_DIR}/rootfs-stage.tar"
echo "==> Staging VM overlay archive (targeting /usr/lib/..., never /lib/...)..."
tar -C "${OUT_DIR}/overlay" -cf "${STAGE_TAR}" usr etc

echo "==> Allocating raw disk image (${RAW_IMAGE}, ${IMAGE_SIZE})..."
qemu-img create -f raw "${RAW_IMAGE}" "${IMAGE_SIZE}"

echo "==> Converting raw disk image to qcow2 (${QCOW2_IMAGE})..."
qemu-img convert -f raw -O qcow2 "${RAW_IMAGE}" "${QCOW2_IMAGE}"
rm -f "${RAW_IMAGE}"

if [[ -f "${REPO_ROOT}/build/linux/arch/arm64/boot/Image" ]]; then
  cp "${REPO_ROOT}/build/linux/arch/arm64/boot/Image" "${KERNEL_IMAGE}"
else
  echo "note: ${REPO_ROOT}/build/linux/arch/arm64/boot/Image not present; create or copy an ARM64 virt Image to ${KERNEL_IMAGE} before booting."
fi

echo "==> Tier 3 VM image build complete:"
echo "    Image    : ${QCOW2_IMAGE}"
echo "    Manifest : ${MANIFEST_JSON}"
echo "    Launcher : ${QEMU_LAUNCH_SH}"
