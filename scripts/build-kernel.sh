#!/usr/bin/env bash
set -euo pipefail
workspace=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
kernel_release=7.1.5-76070105-generic
toolchain="$workspace/tools/rust-kernel"
metadata="$toolchain/usr/src/linux-lib-rust-$kernel_release/rust"
export LD_LIBRARY_PATH="$toolchain/usr/lib/x86_64-linux-gnu:$toolchain/usr/lib/rust-1.91/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
dev_flags=
case ${APS_DEV:-0} in
    0) ;;
    1) dev_flags='--cfg aps_dev' ;;
    *) echo 'APS_DEV must be 0 or 1' >&2; exit 1 ;;
esac
exec make -C "/lib/modules/$kernel_release/build" M="$workspace/kernel" \
    RUSTC="$toolchain/usr/bin/rustc-1.91" \
    RUSTFLAGS_MODULE="-L $metadata $dev_flags" modules "$@"
