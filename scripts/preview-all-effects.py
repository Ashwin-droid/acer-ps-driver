#!/usr/bin/env python3
"""Labeled normal-CLI previews; preserve settings and freeze persistence while testing."""
from pathlib import Path
import json
import os
import signal
import subprocess
import time

BASE = Path('/sys/class/misc/acer-phn16s')
APS = '/usr/local/sbin/aps'
PROBE = '/usr/local/sbin/aps-probe'

def command(*args):
    result = subprocess.run(args, text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError(f'{args}: {result.stderr.strip()}')
    return result.stdout.strip()

def notify(title, body):
    subprocess.run(['notify-send', '-a', 'APS', '-t', '8000', title, body], check=False)

def stop(signum, frame):
    raise InterruptedError('preview interrupted')

signal.signal(signal.SIGTERM, stop)
signal.signal(signal.SIGINT, stop)
print(f'Preview PID: {os.getpid()}', flush=True)
if 'active=0' not in command(PROBE, 'status'):
    raise SystemExit('Another developer session is active; refusing to interfere')
command('/usr/local/libexec/aps-state', 'save')
keyboard = (BASE / 'keyboard_lighting').read_text().strip()
logo = (BASE / 'cover_lighting').read_text().strip()
if 'undefined' in keyboard or logo == 'undefined':
    raise SystemExit('Need known keyboard and logo settings before previewing')
before = {name: (BASE / name).read_text() for name in
          ('performance_mode', 'fan_control', 'turbo_button_mode', 'keyboard_backlight_timeout_seconds', 'turbo_lighting')}
tests = [
    ('Keyboard OFF', ['keyboard', 'off'], 3, 'Keyboard should be dark.'),
    ('Keyboard STATIC red', ['keyboard', 'static', 'FF0000', '50'], 7, 'Steady red, 50%.'),
    ('Keyboard STATIC four zones', ['keyboard', 'static', 'FF0000,00FF00,0000FF,FF00FF', '50'], 7, 'Four independent static colors.'),
    ('Keyboard BREATHING', ['keyboard', 'breathing', '50', '3'], 10, 'Firmware RGB-cycling fade, no fixed color.'),
    ('Keyboard NEON', ['keyboard', 'neon', '50', '3'], 8, 'Continuous RGB palette.'),
    ('Keyboard WAVE right', ['keyboard', 'wave', '50', '3', '--direction', 'right'], 8, 'Watch the movement direction.'),
    ('Keyboard WAVE left', ['keyboard', 'wave', '50', '3', '--direction', 'left'], 8, 'Movement should reverse.'),
    ('Keyboard SHIFTING', ['keyboard', 'shifting', '50', '3'], 8, 'Native Shifting with firmware colors.'),
    ('Keyboard ZOOM', ['keyboard', 'zoom', '50', '3'], 8, 'Native Zoom with firmware colors.'),
    ('Keyboard METEOR', ['keyboard', 'meteor', '50', '3'], 8, 'Native Meteor with firmware colors.'),
    ('Keyboard TWINKLING', ['keyboard', 'twinkling', '40', '3'], 8, 'Native Twinkling with firmware colors.'),
    ('Keyboard STROBE, slow', ['lighting', 'apply', 'strobe', 'FFFFFF', '30', '--speed', '1', '--duty', '0.5', '--duration', '5'], 0, 'Five seconds at ONE flash per second, 30% brightness.'),
    ('Keyboard RAINBOW', ['lighting', 'apply', 'rainbow', '40', '--speed', '3', '--duration', '6'], 0, 'Software Rainbow, two three-second cycles.'),
    ('Back logo OFF', ['logo', 'off'], 3, 'Back-of-lid logo only.'),
    ('Back logo STATIC magenta', ['logo', 'FF00FF', '60'], 7, 'Back logo steady magenta.'),
    ('Back logo BREATHING', ['logo', 'breathing', '60', '3'], 10, 'Back logo fades through RGB.'),
    ('Back logo NEON', ['logo', 'neon', '60', '3'], 8, 'Back logo smoothly cycles RGB.'),
]
results = []
active = False
try:
    # A known read starts a session, preventing previews from replacing boot prefs.
    command(PROBE, 'hid', 'lighting', 'get', 'a100000000')
    active = True
    for index, (label, args, delay, description) in enumerate(tests, 1):
        if label == 'Back logo OFF':
            (BASE / 'keyboard_lighting').write_text(keyboard)
        notify(f'APS {index}/{len(tests)}: {label}', description)
        output = command(APS, *args)
        results.append({'effect': label, 'command': [APS, *args], 'accepted': True, 'output': output})
        print(f'{index}/{len(tests)} {label}: command accepted', flush=True)
        time.sleep(delay)
    assert {name: (BASE / name).read_text() for name in before} == before, 'Unrelated control changed'
finally:
    try:
        (BASE / 'keyboard_lighting').write_text(keyboard)
        (BASE / 'cover_lighting').write_text(logo)
    finally:
        if active:
            command(PROBE, 'finish')
    notify('APS preview finished', 'Your original keyboard and logo settings have been restored. Report any effect that looked wrong. Software acceptance is not visual verification.')
    print(json.dumps({'commands': results, 'restored_keyboard': keyboard, 'restored_logo': logo}, indent=2), flush=True)
