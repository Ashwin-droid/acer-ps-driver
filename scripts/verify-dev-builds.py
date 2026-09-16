#!/usr/bin/env python3
"""Check actual artifacts, not just a runtime dev switch."""
from pathlib import Path
import subprocess
import sys

production, development, public_key = map(Path, sys.argv[1:])
key = public_key.read_bytes()
for module, enabled in [(production, False), (development, True)]:
    data = module.read_bytes()
    imports = subprocess.check_output(['nm', '-u', str(module)], text=True)
    assert (key in data) == enabled, (module, 'embedded developer key')
    for name in [b'dev_challenge\x00', b'dev_probe\x00', b'APSDEV01']:
        assert (name in data) == enabled, (module, name)
    for symbol in ['public_key_verify_signature', 'sha256', 'get_random_bytes']:
        assert (symbol in imports) == enabled, (module, symbol)
    assert not subprocess.check_output(['modinfo', '-p', str(module)]).strip(), 'runtime mode switch found'
    print(f'PASS: {module}: development code present={enabled}')
