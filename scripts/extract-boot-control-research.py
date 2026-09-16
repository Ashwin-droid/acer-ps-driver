#!/usr/bin/env python3
"""Offline-only extraction from the local V1.26 capsule. Never executes firmware."""
from pathlib import Path
import hashlib
import lzma
import sys

source = Path(sys.argv[1])
destination = Path(sys.argv[2])
raw = source.read_bytes()
decoder = lzma.LZMADecompressor(format=lzma.FORMAT_ALONE, memlimit=128 * 1024 * 1024)
blob = decoder.decompress(raw[0x3FBC00:], max_length=64 * 1024 * 1024)
assert decoder.eof
assert blob[0x13E6894:0x13E6894 + 12] == 'Sound:'.encode('utf-16-le')
destination.mkdir(parents=True, exist_ok=True)
print('Source SHA256:', hashlib.sha256(raw).hexdigest())
for name, section, size in [
    ('A01WMISmmCallback', 0x1A932EC, 71172),
    ('AcerBIOSConfigurationToolSmm', 0x1A6597C, 11780),
]:
    assert int.from_bytes(blob[section:section + 3], 'little') == size
    assert blob[section + 3] == 0x10
    image = blob[section + 4:section + size]
    assert image[:2] == b'MZ'
    output = destination / (name + '.efi')
    if output.exists():
        assert output.read_bytes() == image
    else:
        with output.open('xb') as stream:
            stream.write(image)
    print(output, hashlib.sha256(image).hexdigest())

for name in ['A01ODMSmmServiceDriver', 'A01SmmServiceBody', 'A01DataServiceBodySmm', 'AcpiCallbacksSmm', 'PlatformInitAdvancedSmm']:
    ui = blob.find(name.encode('utf-16-le'))
    assert ui >= 0
    found = []
    for start in range(max(0, ui - 1000000), ui):
        if blob[start + 18] not in (7, 10, 12, 13):
            continue
        size = int.from_bytes(blob[start + 20:start + 23], 'little')
        if not (24 < size < 1000000 and ui < start + size < ui + 4096):
            continue
        section = start + 24
        images = []
        matching_ui = False
        while section < start + size:
            length = int.from_bytes(blob[section:section + 3], 'little')
            kind = blob[section + 3]
            if length < 4 or section + length > start + size:
                break
            if kind == 0x10:
                images.append(blob[section + 4:section + length])
            if kind == 0x15 and section + 4 == ui:
                matching_ui = True
            section = (section + length + 3) & ~3
        if matching_ui and len(images) == 1:
            found.append((start, images[0]))
    assert len(found) == 1, (name, len(found))
    start, image = found[0]
    assert image[:2] == b'MZ'
    output = destination / (name + '.efi')
    if output.exists():
        assert output.read_bytes() == image
    else:
        with output.open('xb') as stream:
            stream.write(image)
    print(output, 'FFS', hex(start), hashlib.sha256(image).hexdigest())
