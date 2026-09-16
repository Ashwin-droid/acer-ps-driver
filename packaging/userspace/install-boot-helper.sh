#!/usr/bin/env bash
set -euo pipefail
[[ $EUID == 0 ]] || { echo 'Run this one-time helper installation as root.' >&2; exit 1; }
bundle=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
[[ $(< /sys/class/dmi/id/product_name) == 'Predator PHN16S-71' ]]
[[ $(< /sys/class/dmi/id/board_name) == FTYPE_ARX ]]
[[ $(< /sys/class/dmi/id/bios_version) == V1.28 ]]
install -d -m 0755 /usr/local/libexec
install -m 0755 "$bundle/aps-boot" /usr/local/libexec/aps-boot
echo 'Installed the root-owned boot helper. No boot settings or ESP files changed.'
