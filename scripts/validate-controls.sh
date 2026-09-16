#!/usr/bin/env bash
set -euo pipefail
control=/home/ashwinmhaskar/Documents/Tests/vibedriver-acer-ps/target/release/aps
"$control" status
# Finish validation in Balanced and Auto, including on error or interruption.
trap '"$control" profile balanced; "$control" fans auto' EXIT
"$control" profile balanced
"$control" profile quiet
"$control" status
"$control" profile balanced
"$control" fans maximum 6
"$control" fans manual 55 55 6
"$control" status
