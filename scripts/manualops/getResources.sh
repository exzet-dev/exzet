#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
REL_URL="https://github.com/firecracker-microvm/firecracker/releases"
LATEST_VERSION=$(basename $(curl -fsSLI -o /dev/null -w  %{url_effective} ${REL_URL}/latest))
BIN_DIR=$(cd "$SCRIPT_DIR/.." && mkdir -p bin && cd bin && pwd)
RES_DIR=$(cd "$SCRIPT_DIR/.." && mkdir -p resources && cd resources && pwd)
ARCH="$(uname -m)"
S3_BUCKET="spec.ccfc.min"

# FC
FC_TYPE="release"
TARGET_DIR="${BIN_DIR}/${FC_TYPE}-${LATEST_VERSION}-${ARCH}"
FIRECRACKER_BIN="${TARGET_DIR}/firecracker"
FIRECRACKER_TARGET="${RES_DIR}/firecracker"

# KERNEL
LATEST_KERNEL_V=$(wget "http://${S3_BUCKET}.s3.amazonaws.com/?prefix=firecracker-ci/v1.10/x86_64/vmlinux-5.10&list-type=2" -O - 2>/dev/null | grep "(?<=<Key>)(firecracker-ci/v1.10/x86_64/vmlinux-5\.10\.[0-9]{3})(?=</Key>)" -o -P)
KERNEL_TARGET="${RES_DIR}/vmlinux"

# ROOTFS
ROOTFS_TYPE="ubuntu-22.04"
ROOTFS_URL_PART="https://s3.amazonaws.com/${S3_BUCKET}/firecracker-ci/v1.10/${ARCH}/${ROOTFS_TYPE}"
ROOTFS_FSTYPE="ext4"
ROOTFS_KEYTYPE="id_rsa"
ROOTFS_TARGET="${RES_DIR}/${ROOTFS_TYPE}.${ROOTFS_FSTYPE}"
ROOTFS_KEY_TARGET="${RES_DIR}/${ROOTFS_TYPE}.${ROOTFS_KEYTYPE}"

ensure_firecracker() {
    if [ -f $FIRECRACKER_BIN ]
    then
        echo "Firecracker binary found at: ${FIRECRACKER_BIN}"
    else
        echo "Firecracker binary NOT found at: ${FIRECRACKER_BIN}"
        if [[ "$FC_TYPE" == "release" ]]
        then
            echo "Downloading Firecracker from ${REL_URL}"
            $SCRIPT_DIR/getFirecracker.sh
        else
            echo "Building Firecracker from source"
            $SCRIPT_DIR/buildFirecracker.sh
        fi
    fi

    mv "${FIRECRACKER_BIN}" "${FIRECRACKER_TARGET}"
    echo "Saved Firecracker bin at ${FIRECRACKER_TARGET}..."
}

ensure_kernel() {
    if [ -f $KERNEL_TARGET ]
    then
        echo "Linux kernel found at: ${KERNEL_TARGET}"
    else
        echo "Linux kernel NOT found at: ${KERNEL_TARGET}, Downloading now..."
        wget "https://s3.amazonaws.com/${S3_BUCKET}/${LATEST_KERNEL_V}" -O "${KERNEL_TARGET}"
        echo "Saved kernel at ${KERNEL_TARGET}..."
    fi
}

ensure_rootfs() {
    if [ -f $ROOTFS_TARGET ]
    then
        echo "Rootfs found at: ${ROOTFS_TARGET}"
    else
        echo "Rootfs NOT found at: ${ROOTFS_TARGET}, Downloading now..."
        wget "${ROOTFS_URL_PART}.${ROOTFS_FSTYPE}" -O "${ROOTFS_TARGET}"
        echo "Saved rootfs at ${ROOTFS_TARGET}..."
    fi

    if [ -f $ROOTFS_KEY_TARGET ]
    then
        echo "Rootfs key found at: ${ROOTFS_KEY_TARGET}"
    else
        echo "Rootfs key NOT found at: ${ROOTFS_KEY_TARGET}, Downloading now..."
        wget "${ROOTFS_URL_PART}.${ROOTFS_KEYTYPE}" -O "${ROOTFS_KEY_TARGET}"
        chmod 400 "${ROOTFS_KEY_TARGET}"
        echo "Saved rootfs key at ${ROOTFS_KEY_TARGET}..."
    fi
}

ensure_firecracker
ensure_kernel
ensure_rootfs
