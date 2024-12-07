#!/bin/bash

# SETUP BUILD ENV
export KERNEL_VERSION="6.1.1"
export BUSYBOX_VERSION="1.36.1"

# DOWNLOAD AND EXTRACT AS BEFORE
wget https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz
wget https://busybox.net/downloads/busybox-${BUSYBOX_VERSION}.tar.bz2
tar xf linux-${KERNEL_VERSION}.tar.xz
tar xf busybox-${BUSYBOX_VERSION}.tar.bz2

# BUILD KERNEL WITH ADDITIONAL STORAGE SUPPORT
cd linux-${KERNEL_VERSION}
cat > hybrid.config << 'EOF'
# PREVIOUS CONFIGS
CONFIG_BINFMT_ELF=y
CONFIG_BINFMT_SCRIPT=y
CONFIG_BLK_DEV=y
CONFIG_BLK_DEV_LOOP=y
CONFIG_BLK_DEV_RAM=y
CONFIG_BLOCK=y
CONFIG_FUTEX=y
CONFIG_FUTEX_PI=y
CONFIG_INET=y
CONFIG_INET_DIAG=y
CONFIG_INET_TCP_DIAG=y
CONFIG_INET_UDP_DIAG=y
CONFIG_INOTIFY_USER=y
CONFIG_NET=y
CONFIG_NETDEVICES=y
CONFIG_NET_CORE=y
CONFIG_PACKET=y
CONFIG_POSIX_TIMERS=y
CONFIG_PROC_FS=y
CONFIG_SHMEM=y
CONFIG_SYSFS=y
CONFIG_TTY=y
CONFIG_UNIX=y
CONFIG_UNIX98_PTYS=y
CONFIG_VETH=y
CONFIG_VIRTIO=y
CONFIG_VIRTIO_BLK=y
CONFIG_VIRTIO_NET=y
CONFIG_VIRTIO_PCI=y
CONFIG_VIRTIO_RING=y

# ADDITIONAL STORAGE CONFIGS
CONFIG_EXT4_FS=y
CONFIG_EXT4_USE_FOR_EXT2=y
CONFIG_TMPFS=y
CONFIG_TMPFS_POSIX_ACL=y
EOF

make allnoconfig
./scripts/kconfig/merge_config.sh .config hybrid.config
make -j$(nproc) vmlinux

# BUILD ENHANCED BUSYBOX
cd ../busybox-${BUSYBOX_VERSION}
make defconfig
sed -i 's/CONFIG_STATIC=n/CONFIG_STATIC=y/' .config
# Enable additional storage utilities
sed -i 's/CONFIG_MKFS_EXT2=n/CONFIG_MKFS_EXT2=y/' .config
sed -i 's/CONFIG_E2FSCK=n/CONFIG_E2FSCK=y/' .config
make -j$(nproc)

# CREATE HYBRID FILESYSTEM STRUCTURE
mkdir -p rootfs/{bin,sbin,etc,proc,sys,usr/{bin,sbin},mnt,var/{log,tmp},workspace,tmp}
cp busybox rootfs/bin/

# CREATE ENHANCED INIT SCRIPT
cat > rootfs/init << 'EOF'
#!/bin/busybox sh

# MOUNT CORE FILESYSTEMS
mount -t proc none /proc
mount -t sysfs none /sys
mount -t tmpfs -o size=2G tmpfs /tmp
mount -t tmpfs -o size=4G tmpfs /var/tmp

# SETUP WORKSPACE RAMDISK
mount -t tmpfs -o size=8G tmpfs /workspace/ramdisk

# DETECT AND MOUNT ROOT DEVICE
if [ -b /dev/vdb ]; then
    # CHECK AND MOUNT PERSISTENT STORAGE
    e2fsck -p /dev/vdb
    if [ $? -ge 4 ]; then
        # FILESYSTEM SEVERELY DAMAGED, CREATE NEW ONE
        mkfs.ext4 /dev/vdb
    fi
    mount /dev/vdb /workspace/persistent
fi

# SETUP NETWORK
ip link set eth0 up
udhcpc -i eth0

# CREATE WORKSPACE STRUCTURE
mkdir -p /workspace/{ramdisk,persistent}/build
mkdir -p /workspace/{ramdisk,persistent}/cache
mkdir -p /workspace/{ramdisk,persistent}/tmp

# SET ENVIRONMENT VARIABLES
export TMPDIR=/workspace/ramdisk/tmp
export XDG_CACHE_HOME=/workspace/persistent/cache

exec /bin/sh
EOF

chmod +x rootfs/init

# CREATE MAIN FILESYSTEM IMAGE
dd if=/dev/zero of=root.img bs=1M count=4096
mkfs.ext4 root.img
mkdir -p mnt
mount root.img mnt
cp -a rootfs/* mnt/
umount mnt
rmdir mnt

# CREATE INITIAL RAMDISK (SMALLER, BOOT-ONLY)
cd rootfs
find . | cpio -H newc -o | gzip > ../initramfs.cpio.gz

# OUTPUTS:
# - linux-${KERNEL_VERSION}/vmlinux (KERNEL)
# - initramfs.cpio.gz (BOOT RAMDISK)
# - root.img (MAIN FILESYSTEM)
EOF
