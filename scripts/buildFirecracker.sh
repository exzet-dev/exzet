#!/usr/bin/env bash

if [ "$(id -u)" -ne 0 ]; then echo "Root is needed for build, please run with sudo or as root user." >&2; exit 1; fi

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
BIN_DIR=$(cd "$SCRIPT_DIR/.." && mkdir -p bin && cd bin && pwd)
BUILD_DIR=$(cd "$SCRIPT_DIR/.." && mkdir -p build && cd build && pwd)
ARCH="$(uname -m)"

# Get latest version for id
REL_URL="https://github.com/firecracker-microvm/firecracker/releases"
LATEST_VERSION=$(basename $(curl -fsSLI -o /dev/null -w  %{url_effective} ${REL_URL}/latest))
WORK_DIR="${BUILD_DIR}/build-${LATEST_VERSION}-${ARCH}"
TARGET_DIR="${BIN_DIR}/build-${LATEST_VERSION}-${ARCH}"

mkdir -p "${WORK_DIR}" "${TARGET_DIR}"
cd "${WORK_DIR}"

# Clone the firecracker repository
git clone https://github.com/firecracker-microvm/firecracker firecracker_src

# Start docker
sudo systemctl start docker

# Build firecracker
#
# It is possible to build for gnu, by passing the arguments '-l gnu'.
#
# This will produce the firecracker and jailer binaries under
# `./firecracker/build/cargo_target/${toolchain}/debug`.
#
sudo ./firecracker_src/tools/devtool build

# Rename the binary to "firecracker"
sudo cp ./firecracker_src/build/cargo_target/${ARCH}-unknown-linux-musl/debug/firecracker "${TARGET_DIR}firecracker"