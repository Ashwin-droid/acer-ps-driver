#!/usr/bin/env bash
set -euo pipefail
[[ $EUID == 0 ]]
workspace=/home/ashwinmhaskar/Documents/Tests/vibedriver-acer-ps
account=ashwinmhaskar
state_dir=/home/ashwinmhaskar/.local/state
systemctl stop aps-state.service
if systemctl is-active --quiet aps-preferences.service; then systemctl stop aps-preferences.service; fi
systemctl disable aps-preferences.service 2>/dev/null || true
rm -f /etc/systemd/system/aps-preferences.service
install -d -m 0755 /usr/local/libexec
install -m 0755 "$workspace/dist/aps-userspace-0.2.0-linux-x86_64/aps-state" /usr/local/libexec/aps-state

# Capture live settings under the normal account before the old module unloads.
if ! runuser -u "$account" -- env XDG_STATE_HOME="$state_dir" /usr/local/libexec/aps-state save; then
    # An unreadable boot profile must not block installing its fix or destroy
    # the last saved settings. Proceed only with a valid existing user record.
    runuser -u "$account" -- env XDG_STATE_HOME="$state_dir" /usr/local/libexec/aps-state validate
fi
"$workspace/dist/aps-kernel-0.2.0-7.1.5/install.sh" "$account"
install -m 0755 "$workspace/dist/aps-userspace-0.2.0-linux-x86_64/aps" /usr/local/sbin/aps
install -m 0755 "$workspace/dist/aps-userspace-0.2.0-linux-x86_64/aps-probe" /usr/local/sbin/aps-probe
install -m 0755 "$workspace/dist/aps-userspace-0.2.0-linux-x86_64/aps-boot" /usr/local/libexec/aps-boot
sed -e 's|@USER@|ashwinmhaskar|g' \
    -e 's|@HOME@|/home/ashwinmhaskar|g' \
    -e 's|@STATE@|/home/ashwinmhaskar/.local/state|g' \
    "$workspace/packaging/userspace/aps-state.service.in" > /etc/systemd/system/aps-state.service
chmod 0644 /etc/systemd/system/aps-state.service
systemctl daemon-reload
runuser -u "$account" -- rm -f "$state_dir/aps/status"
systemctl enable --now aps-state.service
for attempt in {1..50}; do
    if [[ -f "$state_dir/aps/status" ]] && grep -q '^ready' "$state_dir/aps/status" && systemctl is-active --quiet aps-state.service; then break; fi
    sleep 0.1
done
systemctl is-active --quiet aps-state.service
grep -q '^ready' "$state_dir/aps/status"
runuser -u "$account" -- /usr/local/sbin/aps status
runuser -u "$account" -- /usr/local/sbin/aps preferences status
runuser -u "$account" -- /usr/local/sbin/aps button status
runuser -u "$account" -- /usr/local/sbin/aps turbo-led status
# Obsolete kernel-only preference records, removed only after the new service is active.
rm -f /var/lib/aps/state.a /var/lib/aps/state.b /var/lib/aps/format /var/lib/aps/preferences /var/lib/aps/lock
rmdir /var/lib/aps 2>/dev/null || true
stat -c '%A %U:%G %n' "$state_dir/aps" "$state_dir/aps/preferences"
systemctl show aps-state.service -p User -p MainPID -p ActiveState -p CapabilityBoundingSet
