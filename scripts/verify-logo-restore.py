#!/usr/bin/env python3
"""Unprivileged live restore test. Preserve the user's current settings."""
from pathlib import Path
import subprocess

base = Path('/sys/class/misc/acer-phn16s')
prefs = Path.home() / '.local/state/aps/preferences'
other_names = ('profile', 'button_mode', 'keyboard_state', 'keyboard_timeout',
               'turbo_led_mode', 'turbo_led_manual')
def run(*args):
    return subprocess.run(args, check=True, text=True, capture_output=True).stdout
def others():
    return {name: (base / name).read_text() for name in other_names}

if 'active=0' not in run('/usr/local/sbin/aps-probe', 'status'):
    raise SystemExit('Another developer session is active; refusing to interfere')
run('/usr/local/libexec/aps-state', 'save')
original = (base / 'logo').read_text().strip()
if original == 'unknown':
    raise SystemExit('Choose a normal logo setting before the restore test')
saved = prefs.read_bytes()
before_other = others()
session = False
changed = False
try:
    # Known target-list read only. Freezes preference capture during the test.
    run('/usr/local/sbin/aps-probe', 'hid', 'lighting', 'get', 'a100000000')
    session = True
    run('/usr/local/sbin/aps', 'logo', 'off')
    changed = True
    assert (base / 'logo').read_text().strip() == 'off'
    assert prefs.read_bytes() == saved, 'test effect replaced saved preferences'
    assert others() == before_other, 'Other controls changed; aborting full restore'
    run('/usr/local/libexec/aps-state', 'restore')
    changed = False
    assert (base / 'logo').read_text().strip() == original
    assert prefs.read_bytes() == saved
    assert others() == before_other
    print('PASS: disk preference restore reapplied the logo and preserved other controls')
finally:
    try:
        # Never overwrite a newer logo selection made by the user during a test.
        if changed and (base / 'logo').read_text().strip() == 'off':
            (base / 'logo').write_text(original)
    finally:
        if session:
            run('/usr/local/sbin/aps-probe', 'finish')
