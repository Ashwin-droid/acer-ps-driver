#!/usr/bin/env bash
set -euo pipefail
control=/home/ashwinmhaskar/Documents/Tests/vibedriver-acer-ps/target/release/aps
for color in FF0000 00FF00 0000FF; do
    printf 'Keyboard color: %s\n' "$color"
    "$control" keyboard static "$color" 80
    sleep 3
done
printf '%s\n' 'Four zones: red, green, blue, purple'
"$control" keyboard static FF0000,00FF00,0000FF,BB00FF 80
