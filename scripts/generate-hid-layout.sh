#!/usr/bin/env bash
set -euo pipefail
workspace=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
toolchain="$workspace/tools/rust-kernel"
headers=/usr/src/linux-headers-7.1.5-76070105-generic
export LD_LIBRARY_PATH="$toolchain/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export LIBCLANG_PATH="$toolchain/usr/lib/llvm-21/lib"
trace_flags=()
if rg -q '^CONFIG_HAVE_FENTRY=y$' "$headers/.config"; then
    trace_flags=(-mfentry -DCC_USING_FENTRY)
fi
exec "$toolchain/usr/bin/bindgen" "$workspace/kernel/hid_layout.h" \
    --use-core --allowlist-var 'PH_.*' --no-recursive-allowlist \
    --no-layout-tests --no-doc-comments --output "$workspace/kernel/hid_layout.rs" -- \
    -D__KERNEL__ -DMODULE -funsigned-char -fno-builtin -fms-extensions "${trace_flags[@]}" \
    -I"$headers/arch/x86/include" -I"$headers/arch/x86/include/generated" \
    -I"$headers/include" -I"$headers/arch/x86/include/uapi" \
    -I"$headers/arch/x86/include/generated/uapi" -I"$headers/include/uapi" \
    -I"$headers/include/generated/uapi" \
    -include "$headers/include/linux/kconfig.h" \
    -include "$headers/include/linux/compiler_types.h"
