#!/usr/bin/env bash
set -euo pipefail
workspace=/home/ashwinmhaskar/Documents/Tests/vibedriver-acer-ps
module="$workspace/kernel/acer_phn16s.ko"
certificate=/var/lib/shim-signed/mok/MOK.der
private_key=/var/lib/shim-signed/mok/MOK.priv
expected_fingerprint=F3:2F:08:C3:D8:29:C6:54:C5:A8:09:AB:CF:59:90:44:03:65:47:EA

[[ $(uname -r) == 7.1.5-76070105-generic ]]
[[ $(< /sys/class/dmi/id/product_name) == 'Predator PHN16S-71' ]]
[[ $(< /sys/class/dmi/id/board_name) == FTYPE_ARX ]]
[[ $(< /sys/class/dmi/id/bios_version) == V1.28 ]]
[[ $(openssl x509 -inform DER -in "$certificate" -noout -subject -nameopt RFC2253) == 'subject=CN=Ashwin Module Signing Key' ]]
fingerprint=$(openssl x509 -inform DER -in "$certificate" -noout -fingerprint -sha1)
[[ ${fingerprint#*=} == "$expected_fingerprint" ]]
mokutil --list-enrolled | grep -Fi "${expected_fingerprint,,}" >/dev/null
printf '%s\n' 'Certificate identity and enrollment verified.'
# Only public-key digests are compared. Private key material stays in its existing protected file.
key_public=$(openssl pkey -in "$private_key" -pubout -outform DER | sha256sum)
cert_public=$(openssl x509 -inform DER -in "$certificate" -pubkey -noout | openssl pkey -pubin -outform DER | sha256sum)
if [[ "$key_public" != "$cert_public" ]]; then
    printf '%s\n' 'Configured signing private key does not match the enrolled certificate.' >&2
    exit 1
fi
printf '%s\n' 'Private key matches enrolled certificate.'
if [[ -d /sys/module/acer_phn16s && ${1:-} != --reload ]]; then
    printf '%s\n' 'Module is already loaded. Unload it before replacing it.' >&2
    exit 1
fi
if [[ -z $(modinfo -F signer "$module") ]]; then
    /usr/bin/kmodsign sha256 "$private_key" "$certificate" "$module"
fi
[[ $(modinfo -F signer "$module") == 'Ashwin Module Signing Key' ]]
if [[ -d /sys/module/acer_phn16s ]]; then
    /usr/sbin/rmmod acer_phn16s
fi
printf '%s\n' 'Signed by Ashwin Module Signing Key. Loading the model-locked module.'
/usr/sbin/insmod "$module"
"$workspace/target/release/aps" status
