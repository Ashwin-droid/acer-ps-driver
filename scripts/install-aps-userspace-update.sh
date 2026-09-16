#!/usr/bin/env bash
set -euo pipefail
[[ $EUID == 0 ]]
workspace=/home/ashwinmhaskar/Documents/Tests/vibedriver-acer-ps
systemctl stop aps-state.service
install -m 0755 "$workspace/target/release/aps" /usr/local/sbin/aps
install -m 0755 "$workspace/target/release/aps-probe" /usr/local/sbin/aps-probe
install -m 0755 "$workspace/target/release/aps-state" /usr/local/libexec/aps-state
install -m 0755 "$workspace/target/release/aps-boot" /usr/local/libexec/aps-boot
systemctl start aps-state.service
