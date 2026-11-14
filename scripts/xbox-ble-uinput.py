#!/usr/bin/env python3
"""BLE HID -> uinput bridge for the Xbox Wireless Controller on the A21s.

The kernel lacks CONFIG_UHID so BlueZ input-hog cannot create an evdev node.
This bridge reads the HID-over-GATT input report via BlueZ's D-Bus API and
re-emits it through /dev/uinput (CONFIG_INPUT_UINPUT=y).

Input report (ID 1) layout decoded from the device's report map:
  off 0-3    LX, LY  u16 LE each        off 4-7    RX, RY  u16 LE each
  off 8-9    LT      10-bit             off 10-11  RT      10-bit
  off 12 lo  hat     0=null, 1-8 N..NW  off 13-14  buttons 15-bit
  off 15     consumer bit0 = Share
"""
import sys

import dbus
import dbus.mainloop.glib
from evdev import AbsInfo, UInput, ecodes
from gi.repository import GLib

BLUEZ = "org.bluez"
ADDR = sys.argv[1] if len(sys.argv) > 1 else "A8:8C:3E:74:0A:95"
DEV_PATH = "/org/bluez/hci0/dev_" + ADDR.replace(":", "_")
HID_SERVICE = "00001812-0000-1000-8000-00805f9b34fb"
REPORT_CHAR = "00002a4d-0000-1000-8000-00805f9b34fb"
REPORT_REF = "00002908-0000-1000-8000-00805f9b34fb"

BUTTONS = [
    ecodes.BTN_SOUTH, ecodes.BTN_EAST, ecodes.BTN_NORTH, ecodes.BTN_WEST,
    ecodes.BTN_TL, ecodes.BTN_TR, ecodes.BTN_SELECT, ecodes.BTN_START,
    ecodes.BTN_MODE, ecodes.BTN_THUMBL, ecodes.BTN_THUMBR,
]
HAT = {1: (0, -1), 2: (1, -1), 3: (1, 0), 4: (1, 1),
       5: (0, 1), 6: (-1, 1), 7: (-1, 0), 8: (-1, -1)}

CAP = {
    ecodes.EV_KEY: BUTTONS + [ecodes.KEY_RECORD],
    ecodes.EV_ABS: [
        (ecodes.ABS_X,     AbsInfo(0, 0, 65535, 16, 512, 0)),
        (ecodes.ABS_Y,     AbsInfo(0, 0, 65535, 16, 512, 0)),
        (ecodes.ABS_RX,    AbsInfo(0, 0, 65535, 16, 512, 0)),
        (ecodes.ABS_RY,    AbsInfo(0, 0, 65535, 16, 512, 0)),
        (ecodes.ABS_Z,     AbsInfo(0, 0, 1023, 0, 0, 0)),
        (ecodes.ABS_RZ,    AbsInfo(0, 0, 1023, 0, 0, 0)),
        (ecodes.ABS_HAT0X, AbsInfo(0, -1, 1, 0, 0, 0)),
        (ecodes.ABS_HAT0Y, AbsInfo(0, -1, 1, 0, 0, 0)),
    ],
}

dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus = dbus.SystemBus()
ui = None
report_path = None


def make_uinput():
    return UInput(CAP, name="Xbox Wireless Controller",
                  bustype=ecodes.BUS_BLUETOOTH, vendor=0x045E,
                  product=0x0B13, version=0x0523)


def find_report_char():
    """Locate the 0x2A4D Report char whose report reference is input type."""
    om = dbus.Interface(bus.get_object(BLUEZ, "/"),
                        "org.freedesktop.DBus.ObjectManager")
    objs = om.GetManagedObjects()
    svc = None
    for path, ifaces in objs.items():
        if str(path).startswith(DEV_PATH + "/service") and \
                ifaces.get("org.bluez.GattService1", {}).get("UUID") == HID_SERVICE:
            svc = str(path)
            break
    if not svc:
        return None
    for path, ifaces in objs.items():
        if not str(path).startswith(svc + "/char"):
            continue
        if ifaces.get("org.bluez.GattCharacteristic1", {}).get("UUID") != REPORT_CHAR:
            continue
        for dpath, difaces in objs.items():
            if not str(dpath).startswith(str(path) + "/desc"):
                continue
            if difaces.get("org.bluez.GattDescriptor1", {}).get("UUID") != REPORT_REF:
                continue
            desc = dbus.Interface(bus.get_object(BLUEZ, dpath),
                                  "org.bluez.GattDescriptor1")
            ref = bytes(desc.ReadValue({}, byte_arrays=True))
            if len(ref) == 2 and ref[1] == 1:
                return str(path)
    return None


def on_report(_iface, changed, _inv, path=None):
    global ui
    if "Value" not in changed or ui is None:
        return
    data = bytes(changed["Value"])
    if len(data) == 17:
        data = data[1:]
    if len(data) < 16:
        return
    lx, ly, rx, ry = (data[0] | data[1] << 8, data[2] | data[3] << 8,
                      data[4] | data[5] << 8, data[6] | data[7] << 8)
    lt = (data[8] | data[9] << 8) & 0x3FF
    rt = (data[10] | data[11] << 8) & 0x3FF
    hat = data[12] & 0x0F
    btns = (data[13] | data[14] << 8) & 0x7FFF
    share = data[15] & 1

    ui.write(ecodes.EV_ABS, ecodes.ABS_X, lx)
    ui.write(ecodes.EV_ABS, ecodes.ABS_Y, ly)
    ui.write(ecodes.EV_ABS, ecodes.ABS_RX, rx)
    ui.write(ecodes.EV_ABS, ecodes.ABS_RY, ry)
    ui.write(ecodes.EV_ABS, ecodes.ABS_Z, lt)
    ui.write(ecodes.EV_ABS, ecodes.ABS_RZ, rt)
    hx, hy = HAT.get(hat, (0, 0))
    ui.write(ecodes.EV_ABS, ecodes.ABS_HAT0X, hx)
    ui.write(ecodes.EV_ABS, ecodes.ABS_HAT0Y, hy)
    for i, code in enumerate(BUTTONS):
        ui.write(ecodes.EV_KEY, code, (btns >> i) & 1)
    ui.write(ecodes.EV_KEY, ecodes.KEY_RECORD, share)
    ui.syn()


def attach():
    global ui, report_path
    if ui is not None:
        return
    report_path = find_report_char()
    if not report_path:
        print("report char not found", file=sys.stderr)
        return
    ui = make_uinput()
    print("uinput:", ui.device, "report:", report_path, flush=True)
    char = dbus.Interface(bus.get_object(BLUEZ, report_path),
                          "org.bluez.GattCharacteristic1")
    try:
        char.StartNotify()
    except dbus.DBusException as e:
        print("StartNotify:", e, file=sys.stderr)
    bus.add_signal_receiver(
        on_report, signal_name="PropertiesChanged",
        dbus_interface="org.freedesktop.DBus.Properties",
        path=report_path)


def detach():
    global ui, report_path
    report_path = None
    if ui:
        ui.close()
        ui = None
        print("controller disconnected, uinput removed", flush=True)


def on_device(_iface, changed, _inv):
    if "Connected" not in changed:
        return
    if changed["Connected"]:
        attach()
    else:
        detach()


bus.add_signal_receiver(
    on_device, signal_name="PropertiesChanged",
    dbus_interface="org.freedesktop.DBus.Properties", path=DEV_PATH)


def reconnect_tick():
    """Nudge Connect() while the bonded controller is powered on."""
    try:
        dev = bus.get_object(BLUEZ, DEV_PATH)
        props = dbus.Interface(dev, "org.freedesktop.DBus.Properties")
        if not props.Get("org.bluez.Device1", "Connected"):
            dbus.Interface(dev, "org.bluez.Device1").Connect()
    except dbus.DBusException:
        pass
    return True


try:
    dev = dbus.Interface(bus.get_object(BLUEZ, DEV_PATH),
                         "org.freedesktop.DBus.Properties")
    if dev.Get("org.bluez.Device1", "Connected"):
        attach()
    else:
        print("waiting for controller to connect...", flush=True)
except dbus.DBusException as e:
    print("device:", e, file=sys.stderr)

GLib.timeout_add_seconds(15, reconnect_tick)
GLib.MainLoop().run()
