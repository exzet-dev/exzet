#!/usr/bin/env bash

set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then 
    echo "Root is needed for build, please run with sudo or as root user." >&2
    exit 1
fi

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
RESOURCES_DIR=$(cd "$SCRIPT_DIR/../pkg/resources" && mkdir -p bin && pwd)
BUILD_DIR=$RESOURCES_DIR/bin
BUILD_DIR=$(cd "$SCRIPT_DIR/.." && mkdir -p build && cd build && pwd)
ARCH="$(uname -m)"


echo "Building CNI plugins..."
cd "$BUILD_DIR"
rm -rf cni-plugins
if ! git clone https://github.com/firecracker-microvm/firecracker-go-sdk cni-plugins; then
    echo "Failed to clone CNI plugins repository!" >&2
    exit 1
fi

cd cni-plugins
ARTIFACTS=$(pwd)/BUILD
mkdir $ARTIFACTS
FC_TEST_DATA_PATH=$ARTIFACTS make deps
cp $ARTIFACTS/bin/* $BIN_DIR/.

echo "Build and setup complete! Plugins installed to $BIN_DIR."
