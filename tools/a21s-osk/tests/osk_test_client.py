#!/usr/bin/env python3
"""Minimal GTK4 text-input client used to exercise a21s-osk on the device.

GTK4 speaks zwp_text_input_v3, so focusing the entry makes labwc activate the input
method (our keyboard). The label mirrors exactly what the keyboard committed.
"""
import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, GLib  # noqa: E402


def build(app):
    win = Gtk.ApplicationWindow(application=app, title="OSK test")
    win.set_default_size(660, 320)

    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=18)
    box.set_margin_top(24)
    box.set_margin_bottom(24)
    box.set_margin_start(24)
    box.set_margin_end(24)

    entry = Gtk.Entry()
    entry.set_placeholder_text("tap here, then type on a21s-osk")
    entry.add_css_class("title-2")

    label = Gtk.Label(label="committed: <empty>")
    label.set_xalign(0.0)
    label.set_wrap(True)

    def changed(_entry):
        text = entry.get_text()
        label.set_text("committed: " + (text if text else "<empty>"))
        print("changed:", repr(text), flush=True)

    entry.connect("changed", changed)
    box.append(entry)
    box.append(label)
    win.set_child(box)
    win.present()
    entry.grab_focus()
    GLib.idle_add(lambda: entry.grab_focus() or False)
    return win


def main():
    app = Gtk.Application(application_id="dev.rabbiz.OskTest")
    app.connect("activate", lambda a: build(a))
    app.run([])


if __name__ == "__main__":
    main()
