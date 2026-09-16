#!/bin/sh
set -eu
# Copy firmware tables for offline disassembly. No ACPI methods are evaluated.
destination=/home/ashwinmhaskar/Documents/Tests/vibedriver-acer-ps/research/firmware/tables
install -d -m 0755 "$destination"
for table in /sys/firmware/acpi/tables/DSDT /sys/firmware/acpi/tables/SSDT*; do
    [ -f "$table" ] || continue
    install -m 0644 "$table" "$destination/$(basename "$table").aml"
done
