#!/usr/bin/env python3
"""Root-only MUX boundary check. Never sends a valid GPU-mode request."""
import errno
import os
from pathlib import Path
import pwd

assert os.geteuid() == 0, "Run this diagnostic as root"
account = pwd.getpwnam("ashwinmhaskar")
assert account.pw_uid != 0
base = Path("/sys/class/misc/acer-phn16s")
before = ((base / "gpu_mux_mode").read_text(), (base / "gpu_mux_request").read_text())
mode = (base / "gpu_mux_mode").stat()
assert mode.st_uid == 0 and mode.st_mode & 0o777 == 0o640

# Prove the kernel checks the actual writer, even with an fd opened by root.
# All payloads here are invalid as a second safeguard against any mode change.
fd = os.open(base / "gpu_mux_mode", os.O_WRONLY)
try:
    child = os.fork()
    if child == 0:
        try:
            os.setgroups([])
            os.setgid(account.pw_gid)
            os.setuid(account.pw_uid)
            try:
                os.write(fd, b"not-a-mode")
            except OSError as error:
                os._exit(0 if error.errno == errno.EPERM else 10)
            os._exit(11)
        except BaseException:
            os._exit(12)
    _, status = os.waitpid(child, 0)
    assert os.waitstatus_to_exitcode(status) == 0, "unprivileged fd writer did not get EPERM"
finally:
    os.close(fd)
print("PASS: kernel rejects unprivileged writer even through a root-opened fd")

for payload in [b"intel", b"hybrid", b"0", b"1", b"2", b"3", b"0x302",
                b"auto extra", b"auto\0", b"nvidia; reboot", b"optimus\nraw",
                b"AUTO", b"LCD_OVERDRIVE", b"\xff", b"x" * 129]:
    fd = os.open(base / "gpu_mux_mode", os.O_WRONLY)
    try:
        try:
            os.write(fd, payload)
        except OSError as error:
            assert error.errno == errno.EINVAL, (payload, error)
        else:
            raise AssertionError("malformed root request accepted")
    finally:
        os.close(fd)
after = ((base / "gpu_mux_mode").read_text(), (base / "gpu_mux_request").read_text())
assert before == after, "diagnostic changed MUX state"
print("PASS: malformed root requests rejected; firmware selection/request record unchanged")

# Boot animation/sound has the same persistent kernel authorization boundary.
if (base / "boot_effects").exists():
    before_boot = (base / "boot_effects").read_text()
    assert (base / "boot_effects").stat().st_mode & 0o777 == 0o640
    fd = os.open(base / "boot_effects", os.O_WRONLY)
    try:
        child = os.fork()
        if child == 0:
            os.setgroups([])
            os.setgid(account.pw_gid)
            os.setuid(account.pw_uid)
            try:
                os.write(fd, b"not-a-setting")
            except OSError as error:
                os._exit(0 if error.errno == errno.EPERM else 20)
            os._exit(21)
        _, status = os.waitpid(child, 0)
        assert os.waitstatus_to_exitcode(status) == 0
    finally:
        os.close(fd)
    for payload in [b"2", b"on extra", b"off\0", b"sound only", b"x" * 129]:
        fd = os.open(base / "boot_effects", os.O_WRONLY)
        try:
            try:
                os.write(fd, payload)
            except OSError as error:
                assert error.errno == errno.EINVAL
            else:
                raise AssertionError("invalid boot effects request accepted")
        finally:
            os.close(fd)
    assert (base / "boot_effects").read_text() == before_boot
    print("PASS: boot effects root-opened-fd gate and malformed root writes; no setting changed")
