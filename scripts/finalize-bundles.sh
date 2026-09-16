#!/usr/bin/env bash
set -euo pipefail
workspace=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$workspace"
kernel_stage=dist/aps-kernel-0.2.0-7.1.5
user_stage=dist/aps-userspace-0.2.0-linux-x86_64
[[ $(modinfo -F signer "$kernel_stage/acer_phn16s.ko") == 'Ashwin Module Signing Key' ]]
cmp "$kernel_stage/acer_phn16s.ko" /lib/modules/7.1.5-76070105-generic/updates/acer_phn16s.ko
cmp target/release/aps /usr/local/sbin/aps
cmp target/release/aps-state /usr/local/libexec/aps-state
cmp target/release/aps-probe /usr/local/sbin/aps-probe
cmp target/release/aps-boot /usr/local/libexec/aps-boot
install -m 0644 packaging/kernel/README.md "$kernel_stage/README.md"
install -m 0644 DEVELOPMENT.md "$kernel_stage/DEVELOPMENT.md"
install -m 0644 DEVELOPMENT.md "$user_stage/DEVELOPMENT.md"
install -m 0755 packaging/kernel/install.sh "$kernel_stage/install.sh"
if nm -u "$kernel_stage/acer_phn16s.ko" | grep -q public_key_verify_signature; then
    printf '%s\n' 'development: signed raw firmware interface enabled' > "$kernel_stage/BUILD_MODE"
else
    printf '%s\n' 'production: raw firmware interface compiled out' > "$kernel_stage/BUILD_MODE"
fi
install -m 0644 packaging/userspace/README.md "$user_stage/README.md"
install -m 0755 target/release/aps "$user_stage/aps"
install -m 0755 target/release/aps-state "$user_stage/aps-state"
install -m 0755 target/release/aps-probe "$user_stage/aps-probe"
install -m 0755 target/release/aps-boot "$user_stage/aps-boot"
install -m 0755 packaging/userspace/install-boot-helper.sh "$user_stage/install-boot-helper.sh"
install -m 0644 packaging/userspace/aps-state.service.in "$user_stage/aps-state.service.in"
tar -C dist -czf dist/aps-kernel-0.2.0-7.1.5.tar.gz aps-kernel-0.2.0-7.1.5
tar -C dist -czf dist/aps-userspace-0.2.0-linux-x86_64.tar.gz aps-userspace-0.2.0-linux-x86_64
cd dist
sha256sum aps-kernel-0.2.0-7.1.5.tar.gz aps-userspace-0.2.0-linux-x86_64.tar.gz > SHA256SUMS
sha256sum --check SHA256SUMS
