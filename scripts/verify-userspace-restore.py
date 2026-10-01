#!/usr/bin/env python3
"""Restart only the driver and verify the user-account service restores live settings."""
from pathlib import Path
import subprocess
import time

base=Path('/sys/class/misc/acer-phn16s')
names=('performance_mode','turbo_button_mode','keyboard_lighting','keyboard_backlight_timeout_seconds','turbo_lighting','cover_lighting')
def snapshot():
    return {name:(base/name).read_text() for name in names}
expected=snapshot()
subprocess.run(['runuser','-u','ashwinmhaskar','--','/usr/local/libexec/aps-state','save'],check=True)
subprocess.run(['/usr/sbin/rmmod','acer_phn16s'],check=True)
time.sleep(1.2)
subprocess.run(['/usr/sbin/modprobe','acer_phn16s'],check=True)
subprocess.run(['/usr/libexec/aps-sysfs-permissions'],check=True)
for _ in range(100):
    try:
        if snapshot()==expected:
            print('PASS: user-account helper restored all saved settings after driver reload',flush=True)
            break
    except OSError:
        pass
    time.sleep(.1)
else:
    raise SystemExit('Restore did not match the original settings')
subprocess.run(['systemctl','show','aps-state.service','-p','User','-p','MainPID','-p','ActiveState','-p','CapabilityBoundingSet'],check=True)
