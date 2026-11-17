"""Hold-to-open power menu for the Volume Down + Power combo (pure logic, no evdev).

A tap of Volume Down + Power still means "screenshot", but it now fires on release so a
3 s hold can open the power menu instead (a customer who holds the keys wants Power off /
Reboot, not a stray screenshot). Volume Up + Power stays an instant screenshot.

volume-keys.py feeds it: arm() when the Down+Power combo is recognised, release() when
either key comes up, tick() from the select() loop, deadline() to size the select timeout.
"""
HOLD_SEC = 3.0


class HoldMenu:
    def __init__(self, hold_sec=HOLD_SEC):
        self.hold_sec = hold_sec
        self._armed_at = None
        self._fired = False

    def arm(self, now):
        if self._armed_at is None:
            self._armed_at = now
            self._fired = False

    def cancel(self):
        self._armed_at = None
        self._fired = False

    def release(self):
        """Either key came up. Returns "screenshot" for a tap, None after a hold or if idle."""
        was_tap = self._armed_at is not None and not self._fired
        self.cancel()
        return "screenshot" if was_tap else None

    def tick(self, now, down_held, power_held):
        """Returns "menu" once, when both keys have stayed down for the hold time."""
        if self._armed_at is None or self._fired:
            return None
        if not (down_held and power_held):
            return None
        if now - self._armed_at >= self.hold_sec:
            self._fired = True
            return "menu"
        return None

    def deadline(self):
        if self._armed_at is None or self._fired:
            return None
        return self._armed_at + self.hold_sec
