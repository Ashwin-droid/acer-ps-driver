#!/usr/bin/env bash
set -euo pipefail
[[ $EUID == 0 ]]
if [[ -d /sys/module/acer_phn16s ]]; then rmmod acer_phn16s; fi
release=7.1.5-76070105-generic
rm -f "/lib/modules/$release/updates/acer_phn16s.ko" /etc/udev/rules.d/70-aps.rules /etc/modules-load.d/aps.conf /usr/libexec/aps-sysfs-permissions /etc/aps/sysfs-group
depmod "$release"
udevadm control --reload-rules
printf '%s\n' 'APS kernel and its access rules removed; userspace binary and presets retained.'
