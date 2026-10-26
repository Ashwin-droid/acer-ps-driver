#!/usr/bin/env bash
# Explicit root-run candidate test. Leaves the installed DKMS module untouched.
set -euo pipefail
[[ $EUID == 0 ]]
candidate=${1:?Pass the locally built candidate module}
workspace=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
[[ $(< /sys/class/dmi/id/product_name) == 'Predator PHN16S-71' ]]
[[ $(< /sys/class/dmi/id/board_name) == FTYPE_ARX ]]
[[ $(< /sys/class/dmi/id/bios_version) == V1.28 ]]
aps_probe=$(mktemp -d /var/lib/aps/charge-limit-test.XXXXXX)
install -m 0644 "$candidate" "$aps_probe/acer_phn16s.ko"
[[ $(modinfo -F name "$aps_probe/acer_phn16s.ko") == acer_phn16s ]]
[[ $(modinfo -F vermagic "$aps_probe/acer_phn16s.ko") == "$(uname -r) "* ]]
fingerprint=$(openssl x509 -inform DER -in /var/lib/shim-signed/mok/MOK.der -noout -fingerprint -sha1)
[[ ${fingerprint#*=} == F3:2F:08:C3:D8:29:C6:54:C5:A8:09:AB:CF:59:90:44:03:65:47:EA ]]
kmodsign sha256 /var/lib/shim-signed/mok/MOK.priv /var/lib/shim-signed/mok/MOK.der "$aps_probe/acer_phn16s.ko"
[[ $(modinfo -F signer "$aps_probe/acer_phn16s.ko") == 'Ashwin Module Signing Key' ]]
aps_removed=0
aps_loaded=0
aps_service=$(systemctl is-active aps-state.service || true)
cleanup() {
    if [[ $aps_loaded == 1 ]]; then /usr/sbin/rmmod acer_phn16s; fi
    if [[ $aps_removed == 1 ]]; then
        /usr/sbin/modprobe acer_phn16s
        /usr/bin/udevadm settle
        /usr/libexec/aps-sysfs-permissions
    fi
    if [[ $aps_service == active ]]; then systemctl start aps-state.service; fi
}
trap cleanup EXIT
systemctl stop aps-state.service
/usr/sbin/rmmod acer_phn16s
aps_removed=1
/usr/sbin/insmod "$aps_probe/acer_phn16s.ko"
aps_loaded=1
/usr/bin/python3 "$workspace/scripts/verify-battery-controls.py" --test-limit
printf 'Signed candidate retained at %s\n' "$aps_probe"
