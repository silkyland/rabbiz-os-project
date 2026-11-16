import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

PATCHER = Path(__file__).resolve().parents[1] / "patch_egl.py"
spec = importlib.util.spec_from_file_location("patch_egl", PATCHER)
patch_egl = importlib.util.module_from_spec(spec)
spec.loader.exec_module(patch_egl)


class MesaVersionGateTests(unittest.TestCase):
    def test_unknown_binary_is_rejected(self):
        for contents in (b"", b"\x7fELF" + bytes(0x30000)):
            with self.subTest(size=len(contents)), self.assertRaises(ValueError):
                patch_egl.patch_library(contents)

    def test_rejected_version_does_not_create_output_or_modify_source(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "mesa.so"
            source.write_bytes(b"unsupported Mesa version")
            destination = Path(directory) / "private"
            result = subprocess.run(
                [sys.executable, str(PATCHER), str(source), str(destination)],
                capture_output=True, text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Unsupported Mesa binary", result.stderr)
            self.assertFalse(destination.exists())
            self.assertEqual(source.read_bytes(), b"unsupported Mesa version")


if __name__ == "__main__":
    unittest.main()
