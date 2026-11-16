"""The Mesa upgrade fallback must not strand the shell or leak GPU overrides."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

LAUNCHER = Path(__file__).resolve().parents[3] / "rootfs-overlay/usr/local/bin/a21s-shell"


class ShellLauncherTests(unittest.TestCase):
    def run_launcher(self, check_status, floor_kb="0"):
        with tempfile.TemporaryDirectory() as directory:
            binaries = Path(directory)
            gpu = binaries / "a21s-qt-gpu"
            gpu.write_text(
                '#!/bin/sh\nif [ "$1" = --check ]; then exit '
                + str(check_status)
                + '; fi\nprintf "gpu:%s:%s\\n" "$1" "$QS_DISABLE_DMABUF"\n'
            )
            quickshell = binaries / "quickshell"
            quickshell.write_text(
                '#!/bin/sh\nprintf "%s|%s|%s|%s|%s\\n" "$QT_QUICK_BACKEND" '
                '"${LD_PRELOAD-unset}" "${DRI_PRIME-unset}" '
                '"${__EGL_VENDOR_LIBRARY_FILENAMES-unset}" "${QSG_RHI_BACKEND-unset}"\n'
            )
            gpu.chmod(0o755)
            quickshell.chmod(0o755)
            env = dict(os.environ, PATH=directory + os.pathsep + os.defpath,
                       QT_QUICK_BACKEND="rhi", LD_PRELOAD="", DRI_PRIME="gpu",
                       __EGL_VENDOR_LIBRARY_FILENAMES="private.json", QSG_RHI_BACKEND="opengl",
                       A21S_SHELL_CMA_FLOOR_KB=floor_kb)
            return subprocess.run(["/bin/bash", str(LAUNCHER)], env=env,
                                  capture_output=True, text=True)

    def test_valid_gate_launches_only_the_shell_through_gpu_wrapper(self):
        result = self.run_launcher(0)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "gpu:quickshell:1")

    def test_failed_gate_starts_software_without_private_mesa_environment(self):
        result = self.run_launcher(1)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "software|unset|unset|unset|unset")
        self.assertIn("using software glass fallback", result.stderr)

    def test_low_cma_starts_software_even_when_mesa_gate_would_pass(self):
        result = self.run_launcher(0, floor_kb="1073741824")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "software|unset|unset|unset|unset")


if __name__ == "__main__":
    unittest.main()
