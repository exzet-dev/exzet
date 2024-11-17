#!/usr/bin/env bash

if [ "$(id -u)" -ne 0 ]; then echo "Root is needed for net setup, please run with sudo or as root user." >&2; exit 1; fi

set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
upperlim="${2:-1}"
parallel="${3:-1}"

for ((i=0; i<parallel; i++)); do
  s=$((i * upperlim / parallel))
  e=$(((i+1) * upperlim / parallel))
  for ((j=s; j<e; j++)); do
    $SCRIPT_DIR/startGuest.sh "$j"
  done &
done