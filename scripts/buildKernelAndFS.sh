#!/usr/bin/env bash
set -euo pipefail

# GET ABSOLUTE PATHS
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd) || { echo "ERROR: Failed to get script directory"; exit 1; }
PROJECT_ROOT=$(cd "$SCRIPT_DIR/.." && pwd) || { echo "ERROR: Failed to get project root"; exit 1; }
BUILD_DIR="$PROJECT_ROOT/build"
OUT_DIR="$BUILD_DIR/output"
ARTIFACTS_DIR="$PROJECT_ROOT/vm-artifacts"



# CREATE DIRS AND VERIFY DOCKERFILE EXISTS
rm -rf $OUT_DIR
if ! mkdir -p "$OUT_DIR"; then
    echo "ERROR: Failed to create output directory $OUT_DIR"
    exit 1
fi

DOCKERFILE="$PROJECT_ROOT/docker/Dockerfile.firecracker"
if [ ! -f "$DOCKERFILE" ]; then
    echo "ERROR: Dockerfile not found at $DOCKERFILE"
    exit 1
fi

# VERIFY BUILD SCRIPT EXISTS
BUILD_SCRIPT="$PROJECT_ROOT/docker/dependencies/build.sh"
if [ ! -f "$BUILD_SCRIPT" ]; then
    echo "ERROR: Build script not found at $BUILD_SCRIPT"
    exit 1
fi

# CHECK DOCKER IS AVAILABLE
if ! command -v docker >/dev/null 2>&1; then
    echo "ERROR: Docker is not installed or not in PATH"
    exit 1
fi

# BUILD IMAGE
echo "Building Docker image..."
TAG="exzet-builder-$(date +%Y%m%d%H%M%S)"
if ! docker build -t "$TAG" -f "$DOCKERFILE" "$PROJECT_ROOT"; then
    echo "ERROR: Docker build failed"
    exit 1
fi

# RUN CONTAINER
echo "Running builder container..."
docker run --rm --privileged \
    -v "$OUT_DIR:/output" \
    "$TAG"

# VERIFY OUTPUTS AND CHECK SIZES
EXPECTED_FILES=("vmlinux" "bzImage" "initramfs.cpio.xz" "workspace.img")
for file in "${EXPECTED_FILES[@]}"; do
    file_path="$OUT_DIR/$file"
    if [ ! -f "$file_path" ]; then
        echo "ERROR: Expected output file $file not found in $OUT_DIR"
        exit 1
    fi

    size=$(stat -c %s "$file_path" 2>/dev/null)
    if [ -z "$size" ] || [ "$size" -eq 0 ]; then
        echo "ERROR: Output file $file is empty or size could not be determined"
        exit 1
    fi
done

 

echo "Build completed successfully!"
echo "Output files location: $OUT_DIR"
echo "Output files:"
ls -lh "$OUT_DIR"

echo "Copying build artifacts to $ARTIFACTS_DIR"
rm -rf $ARTIFACTS_DIR
mkdir -p $ARTIFACTS_DIR
cp -r $OUT_DIR/* $ARTIFACTS_DIR/.
echo "DONE!"
