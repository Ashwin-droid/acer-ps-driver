#!/usr/bin/env bash
set -euo pipefail
workspace=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$workspace"
kernel_stage=dist/aps-kernel-0.2.0-7.1.5
user_stage=dist/aps-userspace-0.2.0-linux-x86_64
install -d "$kernel_stage" "$user_stage/profiles"
install -m 0644 kernel/acer_phn16s.ko "$kernel_stage/acer_phn16s.ko"
if nm -u kernel/acer_phn16s.ko | grep -q public_key_verify_signature; then
    printf '%s\n' 'development: signed raw firmware interface enabled' > "$kernel_stage/BUILD_MODE"
else
    printf '%s\n' 'production: raw firmware interface compiled out' > "$kernel_stage/BUILD_MODE"
fi
install -m 0755 packaging/kernel/install.sh packaging/kernel/uninstall.sh packaging/kernel/aps-sysfs-permissions "$kernel_stage/"
install -m 0644 packaging/kernel/70-aps.rules packaging/kernel/aps.conf "$kernel_stage/"
install -m 0755 target/release/aps "$user_stage/aps"
install -m 0755 target/release/aps-state "$user_stage/aps-state"
install -m 0755 target/release/aps-probe "$user_stage/aps-probe"
install -m 0755 target/release/aps-boot "$user_stage/aps-boot"
install -m 0644 packaging/userspace/aps-state.service.in "$user_stage/aps-state.service.in"
install -m 0755 packaging/userspace/install-user.sh "$user_stage/install-user.sh"
install -m 0755 packaging/userspace/install-boot-helper.sh "$user_stage/install-boot-helper.sh"
# This file existed in a development bundle before persistence moved into the kernel.
if [[ -f "$user_stage/aps-preferences.service" ]]; then rm "$user_stage/aps-preferences.service"; fi
install -m 0644 profiles/*.preset "$user_stage/profiles/"
install -m 0644 packaging/kernel/README.md "$kernel_stage/README.md"
install -m 0644 packaging/userspace/README.md "$user_stage/README.md"
tar -C dist -czf dist/aps-userspace-0.2.0-linux-x86_64.tar.gz aps-userspace-0.2.0-linux-x86_64
# Kernel archive is finalized after the one-time signing step.
