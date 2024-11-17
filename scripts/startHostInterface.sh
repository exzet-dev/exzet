#!/usr/bin/env bash

if [ "$(id -u)" -ne 0 ]; then echo "Root is needed for net setup, please run with sudo or as root user." >&2; exit 1; fi

set -euo pipefail

SB_ID="${1:-0}" # Default to 0
TAP_DEV="fc-${SB_ID}-tap0"
HOST_IFACE=$(ip route get 1.1.1.1 | grep -Po '(?<=dev\s)\w+' | cut -f1 -d ' ')

# Setup TAP device that uses proxy ARP
MASK_SHORT="/30"
TAP_IP="172.16.0.$((4 * SB_ID + 1))"

# Get Paths
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
PROJ_ROOT_DIR=$(cd "$SCRIPT_DIR/.." && pwd)
RES_DIR=$(cd "$PROJ_ROOT_DIR" && mkdir -p resources && cd resources && pwd)
SOCKETS_DIR=$(cd "$PROJ_ROOT_DIR" && mkdir -p $RES_DIR/sockets && cd $RES_DIR/sockets && pwd)
ARCH="$(uname -m)"

# FC
FIRECRACKER="${RES_DIR}/firecracker"



API_SOCKET="${SOCKETS_DIR}/firecracker-${SB_ID}.socket"
sudo rm -f $API_SOCKET
sudo $FIRECRACKER --api-sock "${API_SOCKET}"
