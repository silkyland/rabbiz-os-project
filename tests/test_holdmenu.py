"""HoldMenu: Volume Down + Power tap = screenshot, 3 s hold = power menu (no evdev, no device)."""
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "rootfs-overlay", "usr", "local", "bin"))
from a21s_holdmenu import HoldMenu  # noqa: E402


class HoldMenuTest(unittest.TestCase):
    def test_tap_becomes_screenshot_on_release(self):
        h = HoldMenu()
        h.arm(10.0)
        self.assertIsNone(h.tick(10.5, True, True))
        self.assertEqual(h.release(), "screenshot")

    def test_hold_opens_menu_once_and_release_does_not_screenshot(self):
        h = HoldMenu()
        h.arm(10.0)
        self.assertIsNone(h.tick(12.9, True, True))
        self.assertEqual(h.tick(13.0, True, True), "menu")
        self.assertIsNone(h.tick(14.0, True, True))
        self.assertIsNone(h.release())

    def test_no_menu_if_a_key_was_let_go_before_the_deadline(self):
        h = HoldMenu()
        h.arm(10.0)
        self.assertIsNone(h.tick(13.5, False, True))
        self.assertEqual(h.release(), "screenshot")

    def test_volume_up_joining_cancels_the_hold(self):
        h = HoldMenu()
        h.arm(10.0)
        h.cancel()
        self.assertIsNone(h.tick(14.0, True, True))
        self.assertIsNone(h.release())

    def test_deadline_only_while_armed_and_unfired(self):
        h = HoldMenu()
        self.assertIsNone(h.deadline())
        h.arm(5.0)
        self.assertEqual(h.deadline(), 8.0)
        h.tick(8.0, True, True)
        self.assertIsNone(h.deadline())

    def test_rearming_while_armed_keeps_original_start(self):
        h = HoldMenu()
        h.arm(5.0)
        h.arm(6.5)
        self.assertEqual(h.deadline(), 8.0)


if __name__ == "__main__":
    unittest.main()
