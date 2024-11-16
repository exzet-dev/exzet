#!/usr/bin/env bash

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
BIN_DIR=$(cd "$SCRIPT_DIR/.." && mkdir -p bin && cd bin && pwd)
ARCH="$(uname -m)"

release_url="https://github.com/firecracker-microvm/firecracker/releases"
latest=$(basename $(curl -fsSLI -o /dev/null -w  %{url_effective} ${release_url}/latest))

curl -L ${release_url}/download/${latest}/firecracker-${latest}-${ARCH}.tgz \
| tar -xz

# Rename bin to "firecracker"
mv release-${latest}-${ARCH}/firecracker-${latest}-${ARCH} firecracker
