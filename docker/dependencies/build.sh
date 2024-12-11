#!/bin/bash
set -euo pipefail

# CORE ENV VALIDATION
[[ -z "${KERNEL_VERSION:-}" ]] && { echo "[ERROR] KERNEL_VERSION NOT SET"; exit 1; }

# PATHS
BUILD_ROOT="/build"
KERNEL_DIR="${BUILD_ROOT}/linux-${KERNEL_VERSION}"
KERNEL_CONF="${BUILD_ROOT}/microvm-kernel-ci-x86_64-${KERNEL_VERSION}.config"
ROOTFS_DIR="${BUILD_ROOT}/rootfs"
WORKSPACE_SIZE="4096" # SIZE IN MB

# KERNEL BUILD
build_kernel() {
    cd "${BUILD_ROOT}"
    wget -q "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz"
    tar xf "linux-${KERNEL_VERSION}.tar.xz"
    
    cd "${KERNEL_DIR}"
    
    # USE FIRECRACKER MICROVM CONFIG - NOW WITH CORRECT PATH
    cp "${KERNEL_CONF}" .config
    make olddefconfig
    make -j$(nproc) vmlinux bzImage
}

# SETUP BUSYBOX AND CORE SYSTEM
setup_rootfs() {
    rm -rf "${ROOTFS_DIR}"
    mkdir -p "${ROOTFS_DIR}"/{bin,sbin,dev,proc,sys,workspace,etc,usr/{bin,sbin}}
    
    # COPY AND SETUP BUSYBOX
    cp /bin/busybox "${ROOTFS_DIR}/bin/"
    chmod +x "${ROOTFS_DIR}/bin/busybox"
    
    cd "${ROOTFS_DIR}/bin"
    for app in $(./busybox --list | grep -v ^busybox$); do
        ln -s busybox "${app}"
    done
    
    
    cd "${ROOTFS_DIR}"
    ln -s bin/busybox sbin/busybox
    ln -s bin/busybox usr/bin/busybox
    ln -s bin/busybox usr/sbin/busybox
    
    # COPY SPAWN BINARY
    cp "${BUILD_ROOT}/spawn" "${ROOTFS_DIR}/bin/"
    chmod +x "${ROOTFS_DIR}/bin/spawn"
    
    # COPY FIRECRACKER INIT SCRIPT
    cp "${BUILD_ROOT}/init" "${ROOTFS_DIR}/init"
    chmod +x "${ROOTFS_DIR}/init"
    
    # CREATE BASIC SYSTEM FILES
    echo "root::0:0:root:/:/bin/sh" > "${ROOTFS_DIR}/etc/passwd"
    echo "root:x:0:" > "${ROOTFS_DIR}/etc/group"
    
    # PACK INITRAMFS
    cd "${ROOTFS_DIR}"
    find . | cpio -H newc -o | xz -9 --check=crc32 > "${BUILD_ROOT}/initramfs.cpio.xz"
}

# CREATE WORKSPACE FILESYSTEM
create_workspace() {
    cd "${BUILD_ROOT}"
    dd if=/dev/zero of=workspace.img bs=1M count="${WORKSPACE_SIZE}"
    mkfs.ext4 -F workspace.img
}

# MAIN BUILD FLOW
main() {
    build_kernel
    setup_rootfs
    create_workspace
    
    # VERIFY AND COPY OUTPUTS
    mkdir -p /output
    cp "${KERNEL_DIR}/vmlinux" /output/
    cp "${KERNEL_DIR}/arch/x86/boot/bzImage" /output/
    cp "${BUILD_ROOT}/initramfs.cpio.xz" /output/
    cp "${BUILD_ROOT}/workspace.img" /output/
    
    # VERIFY OUTPUT
    [[ -f /output/vmlinux ]] || { echo "[ERROR] VMLINUX BUILD FAILED"; exit 1; }
    [[ -f /output/bzImage ]] || { echo "[ERROR] BZIMAGE BUILD FAILED"; exit 1; }
    [[ -f /output/initramfs.cpio.xz ]] || { echo "[ERROR] INITRAMFS BUILD FAILED"; exit 1; }
    [[ -f /output/workspace.img ]] || { echo "[ERROR] WORKSPACE BUILD FAILED"; exit 1; }
    
    echo "[SUCCESS] BUILD COMPLETED"
    ls -lh /output
}

main
