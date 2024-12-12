#!/usr/bin/env bash

if [ "$(id -u)" -ne 0 ]; then 
    echo "Root is needed for net setup, please run with sudo or as root user." >&2
    exit 1
fi

set -euo pipefail

# Variables
SB_ID="${1:-0}" # Default to 0
TAP_DEV="fc-${SB_ID}-tap0"
HOST_IFACE=$(ip route get 1.1.1.1 | grep -Po '(?<=dev\s)\w+' | cut -f1 -d ' ')
MASK_SHORT="/30"
TAP_IP="172.16.0.$((4 * SB_ID + 1))"
GUEST_IP="172.16.0.$((4 * SB_ID + 2))"

# Directories
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
PROJ_ROOT_DIR=$(cd "$SCRIPT_DIR/.." && pwd)
LOGS_DIR=$(mkdir -p "$PROJ_ROOT_DIR/logs" && cd "$PROJ_ROOT_DIR/logs" && pwd)
RES_DIR=$(mkdir -p "$PROJ_ROOT_DIR/resources" && cd "$PROJ_ROOT_DIR/resources" && pwd)
SOCKETS_DIR=$(mkdir -p "$RES_DIR/sockets" && cd "$RES_DIR/sockets" && pwd)

# Architecture
ARCH="$(uname -m)"

# Resources
FIRECRACKER="${RES_DIR}/firecracker"
KERNEL="${RES_DIR}/vmlinux"
KERNEL_BOOT_ARGS="console=ttyS0 reboot=k panic=1 pci=off"
[ "$ARCH" = "aarch64" ] && KERNEL_BOOT_ARGS="keep_bootcon ${KERNEL_BOOT_ARGS}"

ROOTFS_TYPE="ubuntu-22.04"
ROOTFS_FSTYPE="ext4"
ROOTFS_KEYTYPE="id_rsa"
ROOTFS="${RES_DIR}/${ROOTFS_TYPE}.${ROOTFS_FSTYPE}"
ROOTFS_KEY="${RES_DIR}/${ROOTFS_TYPE}.${ROOTFS_KEYTYPE}"

# MAC Address Generator
ipToMAC() {
    local ip="$1"
    IFS='.' read -r _ _ octet3 octet4 <<< "$ip"
    printf "06:00:AC:10:%02X:%02X\n" "$octet3" "$octet4"
}

# Cleanup and Setup TAP Device
sudo ip link del "$TAP_DEV" 2>/dev/null || true
sudo ip tuntap add dev "$TAP_DEV" mode tap
sudo ip addr add "${TAP_IP}${MASK_SHORT}" dev "$TAP_DEV"
sudo ip link set dev "$TAP_DEV" up

# Enable IP Forwarding
[ "$(cat /proc/sys/net/ipv4/ip_forward)" -ne 1 ] && echo 1 | sudo tee /proc/sys/net/ipv4/ip_forward > /dev/null

# Configure iptables Rules
sudo iptables -t nat -D POSTROUTING -o "$HOST_IFACE" -j MASQUERADE 2>/dev/null || true
sudo iptables -D FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT 2>/dev/null || true
sudo iptables -D FORWARD -i "$TAP_DEV" -o "$HOST_IFACE" -j ACCEPT 2>/dev/null || true
sudo iptables -t nat -A POSTROUTING -o "$HOST_IFACE" -j MASQUERADE
sudo iptables -I FORWARD 1 -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
sudo iptables -I FORWARD 1 -i "$TAP_DEV" -o "$HOST_IFACE" -j ACCEPT

# Firecracker Configuration
API_SOCKET="${SOCKETS_DIR}/firecracker-${SB_ID}.socket"
LOGFILE="${LOGS_DIR}/firecracker-${SB_ID}.log"
touch "$LOGFILE"

FC_MAC=$(ipToMAC "$GUEST_IP")
echo -e "Generated MAC: ${FC_MAC}\nFrom IP: ${GUEST_IP}"

# Configure Firecracker
sudo curl -X PUT --unix-socket "$API_SOCKET" --data "{
    \"log_path\": \"${LOGFILE}\",
    \"level\": \"Debug\",
    \"show_level\": true,
    \"show_log_origin\": true
}" "http://localhost/logger"

sudo curl -X PUT --unix-socket "$API_SOCKET" --data "{
    \"kernel_image_path\": \"${KERNEL}\",
    \"boot_args\": \"${KERNEL_BOOT_ARGS}\"
}" "http://localhost/boot-source"

sudo curl -X PUT --unix-socket "$API_SOCKET" --data "{
    \"drive_id\": \"rootfs\",
    \"path_on_host\": \"${ROOTFS}\",
    \"is_root_device\": true,
    \"is_read_only\": false
}" "http://localhost/drives/rootfs"

sudo curl -X PUT --unix-socket "$API_SOCKET" --data "{
    \"iface_id\": \"net1\",
    \"guest_mac\": \"$FC_MAC\",
    \"host_dev_name\": \"$TAP_DEV\"
}" "http://localhost/network-interfaces/net1"

# Start VM
echo "Starting VM..."
sleep 0.5
sudo curl -X PUT --unix-socket "$API_SOCKET" --data "{
    \"action_type\": \"InstanceStart\"
}" "http://localhost/actions"

sleep 2

# Configure VM Networking
ssh -i "$ROOTFS_KEY" -o "StrictHostKeyChecking=no" root@"$GUEST_IP" \
"ip route add default via ${TAP_IP} dev eth0 && echo 'nameserver 8.8.8.8' > /etc/resolv.conf"


# Output VM Details
cat << EOF
---------------------------------------

TAP $SB_ID CREATED AND VM CONFIGURED!
IP: ${GUEST_IP}
TAP NAME: ${TAP_DEV}
TAP IP: ${TAP_IP}
MAC: ${FC_MAC}
LOG FILE: ${LOGFILE}
SOCKET: ${API_SOCKET}

---------------------------------------
Run the below command to SSH into the VM:
ssh -i ${ROOTFS_KEY} -o "StrictHostKeyChecking=no" root@${GUEST_IP}
EOF
