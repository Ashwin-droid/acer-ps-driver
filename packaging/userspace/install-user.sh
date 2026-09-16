#!/usr/bin/env bash
set -euo pipefail
[[ $EUID != 0 ]] || { printf '%s\n' 'Run this userspace installer as your normal account.' >&2; exit 1; }
bundle=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
bin_dir="$HOME/.local/bin"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/aps/profiles"
install -d -m 0755 "$bin_dir" "$config_dir"
install -m 0755 "$bundle/aps" "$bin_dir/aps"
for preset in "$bundle"/profiles/*.preset; do
    destination="$config_dir/$(basename "$preset")"
    [[ -e "$destination" ]] || install -m 0644 "$preset" "$destination"
done
printf 'Installed unprivileged CLI to %s/aps. Kernel package is separate.\n' "$bin_dir"
