#!/usr/bin/env bash
set -euo pipefail
[[ $EUID == 0 ]] || { printf '%s\n' 'Run this one-time kernel installation as root.' >&2; exit 1; }
bundle=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
account=${1:?Pass the normal account that should control APS}
[[ $(id -u "$account") != 0 ]]
access_group=$(id -g "$account")
release=7.1.5-76070105-generic
[[ $(uname -r) == "$release" ]]
[[ $(< /sys/class/dmi/id/product_name) == 'Predator PHN16S-71' ]]
[[ $(< /sys/class/dmi/id/board_name) == FTYPE_ARX ]]
[[ $(< /sys/class/dmi/id/bios_version) == V1.28 ]]
module="$bundle/acer_phn16s.ko"
if [[ -f "$bundle/BUILD_MODE" ]]; then head -n 1 "$bundle/BUILD_MODE"; fi
[[ $(modinfo -F vermagic "$module") == "$release "* ]]
certificate=/var/lib/shim-signed/mok/MOK.der
private_key=/var/lib/shim-signed/mok/MOK.priv
expected=F3:2F:08:C3:D8:29:C6:54:C5:A8:09:AB:CF:59:90:44:03:65:47:EA
fingerprint=$(openssl x509 -inform DER -in "$certificate" -noout -fingerprint -sha1)
[[ ${fingerprint#*=} == "$expected" ]]
mokutil --list-enrolled | grep -Fi "${expected,,}" >/dev/null
if [[ -z $(modinfo -F signer "$module") ]]; then
    key_public=$(openssl pkey -in "$private_key" -pubout -outform DER | sha256sum)
    cert_public=$(openssl x509 -inform DER -in "$certificate" -pubkey -noout | openssl pkey -pubin -outform DER | sha256sum)
    [[ "$key_public" == "$cert_public" ]]
    kmodsign sha256 "$private_key" "$certificate" "$module"
fi
[[ $(modinfo -F signer "$module") == 'Ashwin Module Signing Key' ]]
install -d -m 0755 /etc/aps /usr/libexec "/lib/modules/$release/updates"
printf '%s\n' "$access_group" > /etc/aps/sysfs-group
chmod 0644 /etc/aps/sysfs-group
install -m 0755 "$bundle/aps-sysfs-permissions" /usr/libexec/aps-sysfs-permissions
install -m 0644 "$bundle/70-aps.rules" /etc/udev/rules.d/70-aps.rules
install -m 0644 "$bundle/aps.conf" /etc/modules-load.d/aps.conf
udevadm control --reload-rules
if [[ -d /sys/module/acer_phn16s ]]; then rmmod acer_phn16s; fi
install -m 0644 "$module" "/lib/modules/$release/updates/acer_phn16s.ko"
depmod "$release"
modprobe acer_phn16s
/usr/libexec/aps-sysfs-permissions
printf 'APS kernel installed and loaded. Sysfs controls granted to primary group of %s.\n' "$account"
