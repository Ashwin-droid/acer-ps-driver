#!/usr/bin/env python3
"""Read only the APS F14 virtual device for a bounded physical-button test."""
import glob
import os
import select
import struct
import time

devices = []
for path in glob.glob('/sys/class/input/event*/device/name'):
    with open(path) as handle:
        if handle.read().strip() == 'APS Turbo Button':
            devices.append('/dev/input/' + path.split('/')[4])
if len(devices) != 1:
    raise SystemExit('Expected exactly one APS Turbo Button event device')
event = struct.Struct('llHHi')
with open(devices[0], 'rb', buffering=0) as handle:
    poll = select.poll()
    poll.register(handle, select.POLLIN)
    deadline = time.monotonic() + 180
    saw_press = set()
    completed = set()
    print('READY: listening only for APS F14/F15 events', flush=True)
    while time.monotonic() < deadline:
        if not poll.poll(500):
            continue
        packet = handle.read(event.size)
        if len(packet) != event.size:
            raise SystemExit('Input device closed')
        _, _, kind, code, value = event.unpack(packet)
        if kind == 1 and code in (184,185):
            print(f'F{code-170} value={value}', flush=True)
            if value == 1:
                saw_press.add(code)
            if value == 0 and code in saw_press:
                completed.add(code)
                print(f'PASS: captured F{code-170} press/release', flush=True)
                if completed == {184,185}:
                    break
    else:
        print(f'TIMEOUT: completed keys={sorted(completed)}', flush=True)
