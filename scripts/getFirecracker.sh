#!/usr/bin/env bash

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
BIN_DIR=$(cd "$SCRIPT_DIR/.." && mkdir -p bin && cd bin && pwd)
BUILD_DIR=$(cd "$SCRIPT_DIR/.." && mkdir -p build && cd build && pwd)
ARCH="$(uname -m)"

REL_URL="https://github.com/firecracker-microvm/firecracker/releases"
LATEST_VERSION=$(basename $(curl -fsSLI -o /dev/null -w  %{url_effective} ${REL_URL}/latest))
WORK_DIR="${BUILD_DIR}/release-${LATEST_VERSION}-${ARCH}"
TARGET_DIR="${BIN_DIR}/release-${LATEST_VERSION}-${ARCH}"

mkdir -p "${WORK_DIR}" "${TARGET_DIR}"
cd "${WORK_DIR}"

curl -L ${REL_URL}/download/${LATEST_VERSION}/firecracker-${LATEST_VERSION}-${ARCH}.tgz \
| tar -xz

# Rename bin to "firecracker"
mv release-${LATEST_VERSION}-${ARCH}/firecracker-${LATEST_VERSION}-${ARCH} "${TARGET_DIR}/firecracker"
