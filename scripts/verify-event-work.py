#!/usr/bin/env python3
"""Bounded live checks: idle driver counters, or physical F14/F15 delivery.

Reads only APS sysfs and its dedicated virtual input device. No raw keyboard
capture, synthetic events, suspend, or performance-mode changes.
"""
import argparse
import os
from pathlib import Path
import select
import signal
import struct
import subprocess
import time

BASE = Path('/sys/class/misc/acer-phn16s')

def read(name):
    return (BASE / name).read_text().strip()

def notify(title, body):
    subprocess.run(['notify-send', '-a', 'APS', title, body], check=True)

def interrupted(signum, frame):
    raise InterruptedError('verification interrupted')

def keys():
    devices = [p for p in Path('/sys/class/input').glob('event*')
               if (p / 'device/name').read_text().strip() == 'APS Turbo Button']
    if len(devices) != 1:
        raise RuntimeError('Expected exactly one APS virtual input device')
    fd = os.open('/dev/input/' + devices[0].name, os.O_RDONLY | os.O_NONBLOCK)
    original = read('turbo_button_mode')
    before = (read('turbo_button_presses'), read('predatorsense_button_presses'))
    try:
        (BASE / 'turbo_button_mode').write_text('hotkey')
        notify('APS: press both laptop keys once',
               'Please open the lid if needed, then press Turbo and PredatorSense once each. '
               'Turbo is temporarily F14, so this test does not change performance mode. '
               'Listening for two minutes; the previous button mode will be restored.')
        print('Listening on APS virtual device for F14/F15 only.', flush=True)
        event = struct.Struct('@llHHi')
        seen = set()
        deadline = time.monotonic() + 120
        while seen != {184, 185}:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(f'Physical-key test timed out; received {sorted(seen)}')
            if not select.select([fd], [], [], remaining)[0]:
                continue
            data = os.read(fd, event.size * 64)
            if not data or len(data) % event.size:
                raise RuntimeError('Incomplete APS input event')
            for _, _, kind, code, value in event.iter_unpack(data):
                if kind == 1 and value == 1 and code in (184, 185):
                    seen.add(code)
                    print(f'Received F{code - 170}', flush=True)
        assert int(read('turbo_button_presses')) > int(before[0])
        assert int(read('predatorsense_button_presses')) > int(before[1])
        assert read('turbo_button_error') == '0'
        print('PASS: both physical callbacks dispatched F14/F15.', read('work_status'), flush=True)
    finally:
        os.close(fd)
        if read('turbo_button_mode') == 'hotkey':
            (BASE / 'turbo_button_mode').write_text(original)
        print('Button mode restored:', read('turbo_button_mode'), flush=True)
        subprocess.run(['notify-send', '-a', 'APS', 'APS key check finished',
                        'The key listener has stopped and the previous Turbo-button mode is restored.'], check=False)

def idle():
    before = read('work_status')
    print('Idle baseline:', before, flush=True)
    time.sleep(10)
    after = read('work_status')
    assert before == after, f'Work occurred during idle check: {before} -> {after}'
    print('PASS: no work executions or deadline callbacks for 10 seconds.', after, flush=True)

def lid():
    assert read('lid_state') == 'open', 'Start with the lid open'
    devices = [p for p in Path('/sys/class/input').glob('event*')
               if (p / 'device/name').read_text().strip() == 'Lid Switch'
               and (p / 'device/phys').read_text().strip() == 'PNP0C0D/button/input0']
    if len(devices) != 1:
        raise RuntimeError('Expected the ACPI lid input device')
    desired = {name: read(name) for name in ('keyboard_lighting', 'turbo_lighting', 'cover_lighting')}
    fd = os.open('/dev/input/' + devices[0].name, os.O_RDONLY | os.O_NONBLOCK)
    try:
        notify('APS: close and reopen the lid',
               'Only if closing the lid will not suspend your docked session: close it, wait two seconds, '
               'then reopen it. Checking that the normal lid event still reaches input listeners and APS. '
               'Listening for two minutes; no synthetic input or power commands.')
        print('Listening to ACPI SW_LID for close then open.', flush=True)
        event = struct.Struct('@llHHi')
        closed = False
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline:
            if not select.select([fd], [], [], max(0, deadline - time.monotonic()))[0]:
                continue
            data = os.read(fd, event.size * 64)
            if not data or len(data) % event.size:
                raise RuntimeError('Incomplete lid input event')
            for _, _, kind, code, value in event.iter_unpack(data):
                if kind != 5 or code != 0 or value not in (0, 1):
                    continue
                expected = 'closed' if value else 'open'
                settle = time.monotonic() + 2
                while read('lid_state') != expected and time.monotonic() < settle:
                    time.sleep(.01)  # Bounded test observation, not driver polling.
                assert read('lid_state') == expected, f'Input event not reflected by APS: {expected}'
                print('Observed SW_LID and APS state:', expected, read('work_status'), flush=True)
                if value:
                    closed = True
                elif closed:
                    assert {name: read(name) for name in desired} == desired
                    assert 'error=0' in read('lid_status')
                    print('PASS: lid events preserved; cached colors unchanged through close/open.', flush=True)
                    return
        raise TimeoutError('Did not observe both lid transitions')
    finally:
        os.close(fd)

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('check', choices=['idle', 'keys', 'lid'])
    args = parser.parse_args()
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    {'keys': keys, 'idle': idle, 'lid': lid}[args.check]()
