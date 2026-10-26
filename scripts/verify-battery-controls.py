#!/usr/bin/env python3
"""Root boundary checks. Optional --test-limit toggles 80/100 then restores.

Never starts calibration, forces discharge, changes USB charging, or probes
unverified firmware selectors. The limit test checks firmware readback only.
"""
import argparse
import errno
import os
from pathlib import Path
import pwd

base=Path('/sys/class/misc/acer-phn16s')
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--test-limit',action='store_true')
args=parser.parse_args()
assert os.geteuid()==0, 'Run this test as root'
account=pwd.getpwnam('ashwinmhaskar')
assert account.pw_uid!=0
def read(name): return (base/name).read_text().strip()
def write(name,value):
    fd=os.open(base/name,os.O_WRONLY)
    try:
        data=value.encode('ascii')
        assert os.write(fd,data)==len(data)
    finally: os.close(fd)
original=read('battery_charge_limit')
before_calibration=read('battery_calibration')
before_caps=read('battery_capabilities')
assert before_calibration=='off', 'Do not interfere with active calibration'
generation=read('preferences_generation')
print('Initial:',before_caps,'limit='+original,'calibration='+before_calibration,flush=True)
for name, invalid in [('battery_charge_limit',[b'0',b'81',b'255',b'80 extra',b'80\0',b'80;reboot']),
                      ('battery_calibration',[b'start',b'on extra',b'raw',b'off\0'])]:
    path=base/name
    assert path.stat().st_uid==0 and path.stat().st_mode & 0o777==0o640
    fd=os.open(path,os.O_WRONLY)
    try:
        child=os.fork()
        if child==0:
            try:
                os.setgroups([]); os.setgid(account.pw_gid); os.setuid(account.pw_uid)
                try: os.write(fd,b'invalid')
                except OSError as error: os._exit(0 if error.errno==errno.EPERM else 10)
                os._exit(11)
            except BaseException: os._exit(12)
        assert os.waitstatus_to_exitcode(os.waitpid(child,0)[1])==0
    finally: os.close(fd)
    for payload in invalid+[b'x'*129]:
        fd=os.open(path,os.O_WRONLY)
        try:
            try: os.write(fd,payload)
            except OSError as error: assert error.errno==errno.EINVAL,(name,payload,error)
            else: raise AssertionError('Malformed battery request accepted')
        finally: os.close(fd)
    print('PASS:',name,'root-opened-fd gate and malformed requests',flush=True)
if args.test_limit:
    try:
        for value in ['80','100']:
            write('battery_charge_limit',value)
            assert read('battery_charge_limit')==value
            assert read('battery_calibration')==before_calibration
            assert read('battery_capabilities')==before_caps
            print('PASS: firmware charge-limit readback',value,flush=True)
    finally:
        write('battery_charge_limit',original)
assert read('battery_charge_limit')==original
assert read('battery_calibration')==before_calibration
assert read('battery_capabilities')==before_caps
assert read('preferences_generation')==generation
print('Original charge limit restored; calibration remained off; preference generation unchanged.',flush=True)
