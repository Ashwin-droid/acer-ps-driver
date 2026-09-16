#!/usr/bin/env python3
"""Bounded read-only capture of vendor reports, excluding keyboard character reports."""
import glob
import os
import select
import struct
import time

devices = []
for entry in glob.glob('/sys/bus/hid/devices/*/uevent'):
    with open(entry) as file:
        if 'HID_ID=0018:00001025:0000174B\n' not in file.read():
            continue
    for raw in glob.glob(os.path.dirname(entry) + '/hidraw/hidraw*'):
        devices.append('/dev/' + os.path.basename(raw))
if len(devices) != 1:
    raise SystemExit('Expected the one Acer 1025:174B vendor interface')
fd = os.open(devices[0], os.O_RDONLY | os.O_NONBLOCK)
poll = select.poll()
poll.register(fd, select.POLLIN)
fds = {fd: 'hid'}
for name in glob.glob('/sys/class/input/event*/device/name'):
    with open(name) as file:
        label = file.read().strip()
    if label not in ('Acer WMI hotkeys', 'AT Translated Set 2 keyboard', 'Intel HID events', 'Intel HID 5 button array') and not label.startswith('1025174B:00'):
        continue
    event_path = '/dev/input/' + name.split('/')[4]
    event_fd = os.open(event_path, os.O_RDONLY | os.O_NONBLOCK)
    fds[event_fd] = label
    poll.register(event_fd, select.POLLIN)
event_struct = struct.Struct('llHHi')
deadline = time.monotonic() + 120
start = time.monotonic()
print('READY: vendor HID reports only; normal keyboard reports are excluded', flush=True)
try:
    while time.monotonic() < deadline:
        for ready, _ in poll.poll(250):
            report = os.read(ready, 256 if ready == fd else event_struct.size)
            if ready == fd:
                if report and report[0] not in (0x32, 0x0d):
                    print(f'{time.monotonic()-start:.3f}s report={report.hex()}', flush=True)
                elif report and report[0] == 0x32:
                    special = [usage for usage in report[3:] if usage >= 0x3a]
                    if special:
                        print(f'{time.monotonic()-start:.3f}s keyboard special usages={special}', flush=True)
            elif len(report) == event_struct.size:
                _, _, kind, code, value = event_struct.unpack(report)
                # Special keys and vendor ABS events only. Never log letter/number key events.
                if kind == 3 or (kind == 1 and (code >= 112 or 59 <= code <= 68 or code in (87, 88))):
                    print(f'{time.monotonic()-start:.3f}s {fds[ready]} type={kind} code={code} value={value}', flush=True)
finally:
    for handle in fds:
        os.close(handle)
print('Capture finished', flush=True)
