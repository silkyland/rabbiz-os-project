#!/usr/bin/env python3
"""Host unit tests for rootfs-overlay/usr/local/bin/rabbiz-splash (no device)."""

import importlib.machinery
import importlib.util
import os
import unittest

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SPLASH_PATH = os.path.join(REPO_ROOT, "rootfs-overlay", "usr", "local", "bin", "rabbiz-splash")
BMP_PATH = os.path.join(REPO_ROOT, "rootfs-overlay", "usr", "share", "rabbiz", "rabbizos-splash.bmp")

loader = importlib.machinery.SourceFileLoader("rabbiz_splash", SPLASH_PATH)
spec = importlib.util.spec_from_loader("rabbiz_splash", loader)
splash = importlib.util.module_from_spec(spec)
loader.exec_module(splash)


def count_color(buf, stride, fw, fh, x0, y0, w, h, color):
    count = 0
    for y in range(max(0, y0), min(fh, y0 + h)):
        row_off = y * stride
        for x in range(max(0, x0), min(fw, x0 + w)):
            p = row_off + x * 4
            if buf[p:p + 4] == color:
                count += 1
    return count


class TestRabbizSplash(unittest.TestCase):
    def setUp(self):
        self.fw = 720
        self.fh = 1600
        self.stride = self.fw * 4
        self.buf = bytearray(self.stride * self.fh)
        with open(BMP_PATH, "rb") as f:
            self.bmp = f.read()

    def test_paint_and_progress_1_50_100(self):
        base = splash.paint_splash_base(self.buf, self.stride, self.fw, self.fh, self.bmp)
        self.assertIsNotNone(base)
        bg, logo_y, logo_h = base
        self.assertEqual(logo_h, 225)

        layout = splash.compute_bar_layout(self.fw, self.fh, logo_y, logo_h)
        self.assertGreaterEqual(layout["bar_y"], logo_y + logo_h)
        self.assertLessEqual(layout["left_x"] + layout["total_w"], self.fw)
        self.assertLessEqual(layout["bar_y"] + layout["tick_h"], self.fh)

        ticks_w = (layout["num_ticks"] - 1) * layout["tick_pitch"] + layout["tick_w"]

        splash.draw_progress(self.buf, self.stride, self.fw, self.fh, bg, layout, 1)
        c1 = count_color(
            self.buf, self.stride, self.fw, self.fh,
            layout["ticks_x"], layout["bar_y"], ticks_w, layout["tick_h"], splash.COPPER,
        )
        s1 = count_color(
            self.buf, self.stride, self.fw, self.fh,
            layout["ticks_x"], layout["bar_y"], ticks_w, layout["tick_h"], splash.SAND,
        )
        self.assertGreater(c1, 0)
        self.assertGreater(s1, c1)

        splash.draw_progress(self.buf, self.stride, self.fw, self.fh, bg, layout, 50)
        c50 = count_color(
            self.buf, self.stride, self.fw, self.fh,
            layout["ticks_x"], layout["bar_y"], ticks_w, layout["tick_h"], splash.COPPER,
        )
        s50 = count_color(
            self.buf, self.stride, self.fw, self.fh,
            layout["ticks_x"], layout["bar_y"], ticks_w, layout["tick_h"], splash.SAND,
        )
        self.assertGreater(c50, c1)
        self.assertGreater(s50, 0)

        splash.draw_progress(self.buf, self.stride, self.fw, self.fh, bg, layout, 100)
        c100 = count_color(
            self.buf, self.stride, self.fw, self.fh,
            layout["ticks_x"], layout["bar_y"], ticks_w, layout["tick_h"], splash.COPPER,
        )
        s100 = count_color(
            self.buf, self.stride, self.fw, self.fh,
            layout["ticks_x"], layout["bar_y"], ticks_w, layout["tick_h"], splash.SAND,
        )
        self.assertEqual(s100, 0)
        self.assertEqual(c100, layout["num_ticks"] * layout["tick_w"] * layout["tick_h"])

        plum_count = count_color(
            self.buf, self.stride, self.fw, self.fh,
            layout["left_x"], layout["bar_y"], layout["total_w"], layout["tick_h"], splash.PLUM,
        )
        self.assertGreater(plum_count, 0)

    def test_compute_target_pct_milestones(self):
        self.assertEqual(splash.compute_target_pct(0.0, {}), 1)
        self.assertEqual(splash.compute_target_pct(0.0, {"dbus": True}), 25)
        self.assertEqual(splash.compute_target_pct(0.0, {"seatd": True}), 45)
        self.assertEqual(splash.compute_target_pct(0.0, {"user_bus": True}), 65)
        self.assertEqual(splash.compute_target_pct(1.0, {"wayland": True}), 100)
        self.assertEqual(splash.compute_target_pct(splash.MAX_WAIT_S, {}), 100)

        prev = 0
        for t in range(0, 20):
            pct = splash.compute_target_pct(float(t), {})
            self.assertGreaterEqual(pct, prev)
            self.assertLessEqual(pct, 95)
            prev = pct


if __name__ == "__main__":
    unittest.main()
