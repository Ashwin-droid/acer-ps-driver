#!/usr/bin/env bash
set -euo pipefail
control=/home/ashwinmhaskar/Documents/Tests/vibedriver-acer-ps/target/release/aps
finish() {
    set +e
    "$control" profile balanced
    "$control" fans auto
    "$control" keyboard timeout on
    "$control" lighting run four-zones
}
trap finish EXIT
"$control" keyboard timeout off
"$control" keyboard timeout get
"$control" keyboard timeout on
for mode in eco performance turbo balanced; do
    "$control" profile "$mode"
    sleep 1
done
"$control" lighting run slow-breathe
sleep 3
"$control" lighting apply --effect wave --brightness 65 --speed 3 --direction left
sleep 3
"$control" lighting apply --effect rainbow --brightness 65 --speed 3 --duration 5
"$control" lighting apply --effect strobe --colors FFFFFF --brightness 50 --speed 1 --duty 0.5 --duration 4
"$control" status
