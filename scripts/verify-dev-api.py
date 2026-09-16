#!/usr/bin/env python3
"""Signed self-test + known read-only raw operations. No firmware writes."""
from pathlib import Path
import subprocess
import sys
import time

probe = sys.argv[1] if len(sys.argv) > 1 else '/usr/local/sbin/aps-probe'
base = Path('/sys/class/misc/acer-phn16s')
prefs = Path.home() / '.local/state/aps/preferences'
original = prefs.read_bytes()
generation = (base / 'preferences_generation').read_text()
def call(*args):
    time.sleep(.12)
    result = subprocess.run([probe, *args], text=True, capture_output=True, check=True)
    print(result.stdout.strip(), flush=True)
    return result.stdout.splitlines()

call('selftest')
# VFS treats a zero-byte write as a no-op without invoking sysfs store.
assert (base / 'dev_probe').write_bytes(b'') == 0
for data in [b'00', b'x' * 784, b'0' * 786, b'0' * 4096]:
    try:
        (base / 'dev_probe').write_bytes(data)
    except OSError as error:
        assert error.errno == 22, error
    else:
        raise AssertionError('malformed request accepted')
try:
    response = call('hid', 'lighting', 'get', 'a100000000')
    assert len(response[1]) == 10 and response[1].startswith('a1'), response
    response = call('wmi', '05', '01020000', '8')  # Existing fixed CPU RPM getter.
    assert len(response[1]) == 16 and response[1].startswith('00'), response
    response = call('hid', 'ec', 'get', 'a0' + '00' * 64)
    assert len(response[1]) == 130 and response[1].startswith('a0'), response
    save = subprocess.run(['/usr/local/libexec/aps-state', 'save'], capture_output=True, text=True)
    assert save.returncode != 0 and 'developer firmware session active' in save.stderr, save
    assert prefs.read_bytes() == original, 'raw probe overwrote preferences'
    assert (base / 'preferences_generation').read_text() == generation
finally:
    # All test operations above were known reads; no hardware state to undo.
    call('finish')
assert prefs.read_bytes() == original
print('PASS: three raw transports, malformed-input rejection, and preference isolation')
