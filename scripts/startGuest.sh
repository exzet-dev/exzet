#!/usr/bin/env bash

if [ "$(id -u)" -ne 0 ]; then echo "Root is needed for net setup, please run with sudo or as root user." >&2; exit 1; fi

set -euo pipefail

SB_ID="${1:-0}" # Default to 0
TAP_DEV="fc-${SB_ID}-tap0"
HOST_IFACE=$(ip route get 1.1.1.1 | grep -Po '(?<=dev\s)\w+' | cut -f1 -d ' ')

# Setup TAP device that uses proxy ARP
MASK_SHORT="/30"
TAP_IP="$(printf '169.254.%s.%s' $(((4 * SB_ID + 2) / 256)) $(((4 * SB_ID + 2) % 256)))"

# Get Paths
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
PROJ_ROOT_DIR=$(cd "$SCRIPT_DIR/.." && pwd)
LOGS_DIR=$(cd "$PROJ_ROOT_DIR" && mkdir -p logs && cd logs && pwd)
RES_DIR=$(cd "$PROJ_ROOT_DIR" && mkdir -p resources && cd resources && pwd)
SOCKETS_DIR=$(cd "$PROJ_ROOT_DIR" && mkdir -p $RES_DIR/sockets && cd $RES_DIR/sockets && pwd)
ARCH="$(uname -m)"

# FC
FIRECRACKER="${RES_DIR}/firecracker"

# KERNEL
KERNEL="${RES_DIR}/vmlinux"
KERNEL_BOOT_ARGS="console=ttyS0 reboot=k panic=1 pci=off"
if [ ${ARCH} = "aarch64" ]; then
    KERNEL_BOOT_ARGS="keep_bootcon ${KERNEL_BOOT_ARGS}"
fi

# ROOTFS
ROOTFS_TYPE="ubuntu-22.04"
ROOTFS_FSTYPE="ext4"
ROOTFS_KEYTYPE="id_rsa"
ROOTFS="${RES_DIR}/${ROOTFS_TYPE}.${ROOTFS_FSTYPE}"
ROOTFS_KEY="${RES_DIR}/${ROOTFS_TYPE}.${ROOTFS_KEYTYPE}"



# -------- SETUP ----------

ipToMAC() {
    local ip=$1
    IFS='.' read -r -a octets <<< "$ip"
    printf "06:00:%02X:%02X:%02X\n" "${octets[1]}" "${octets[2]}" "${octets[3]}"
}

# Setup network interface
sudo ip link del "$TAP_DEV" 2> /dev/null || true
sudo ip tuntap add dev "$TAP_DEV" mode tap
sudo ip addr add "${TAP_IP}${MASK_SHORT}" dev "$TAP_DEV"
sudo ip link set dev "$TAP_DEV" up


# Enable ip forwarding
[ "$(cat /proc/sys/net/ipv4/ip_forward)" -ne 1 ] && sudo sh -c "echo 1 > /proc/sys/net/ipv4/ip_forward"


# Set up microVM internet access
# sudo iptables -t nat -D POSTROUTING -o "$HOST_IFACE" -j MASQUERADE || true
# sudo iptables -D FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT \
#     || true
# sudo iptables -D FORWARD -i "$TAP_DEV" -o "$HOST_IFACE" -j ACCEPT || true
# sudo iptables -t nat -A POSTROUTING -o "$HOST_IFACE" -j MASQUERADE
# sudo iptables -I FORWARD 1 -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
# sudo iptables -I FORWARD 1 -i "$TAP_DEV" -o "$HOST_IFACE" -j ACCEPT

sudo iptables -t nat -C POSTROUTING -o "$HOST_IFACE" -j MASQUERADE 2>/dev/null || \
    sudo iptables -t nat -D POSTROUTING -o "$HOST_IFACE" -j MASQUERADE || true
sudo iptables -C FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT 2>/dev/null || \
    sudo iptables -D FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT || true
sudo iptables -C FORWARD -i "$TAP_DEV" -o "$HOST_IFACE" -j ACCEPT 2>/dev/null || \
    sudo iptables -D FORWARD -i "$TAP_DEV" -o "$HOST_IFACE" -j ACCEPT || true
sudo iptables -t nat -C POSTROUTING -o "$HOST_IFACE" -j MASQUERADE 2>/dev/null || \
    sudo iptables -t nat -A POSTROUTING -o "$HOST_IFACE" -j MASQUERADE
sudo iptables -C FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT 2>/dev/null || \
    sudo iptables -I FORWARD 1 -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
sudo iptables -C FORWARD -i "$TAP_DEV" -o "$HOST_IFACE" -j ACCEPT 2>/dev/null || \
    sudo iptables -I FORWARD 1 -i "$TAP_DEV" -o "$HOST_IFACE" -j ACCEPT


# THE API SOCKET FOR THE VM
API_SOCKET="${SOCKETS_DIR}/firecracker-${SB_ID}.socket"

# Create log file
LOGFILE="${LOGS_DIR}/firecracker-${SB_ID}.log"
touch $LOGFILE
sudo curl -X PUT --unix-socket "${API_SOCKET}" \
    --data "{
        \"log_path\": \"${LOGFILE}\",
        \"level\": \"Debug\",
        \"show_level\": true,
        \"show_log_origin\": true
    }" \
    "http://localhost/logger"


# Set boot source
sudo curl -X PUT --unix-socket "${API_SOCKET}" \
    --data "{
        \"kernel_image_path\": \"${KERNEL}\",
        \"boot_args\": \"${KERNEL_BOOT_ARGS}\"
    }" \
    "http://localhost/boot-source"


# Set rootfs
sudo curl -X PUT --unix-socket "${API_SOCKET}" \
    --data "{
        \"drive_id\": \"rootfs\",
        \"path_on_host\": \"${ROOTFS}\",
        \"is_root_device\": true,
        \"is_read_only\": false
    }" \
    "http://localhost/drives/rootfs"


FC_MAC=$(ipToMAC "$TAP_IP")


# Set network interface
sudo curl -X PUT --unix-socket "${API_SOCKET}" \
    --data "{
        \"iface_id\": \"net1\",
        \"guest_mac\": \"$FC_MAC\",
        \"host_dev_name\": \"$TAP_DEV\"
    }" \
    "http://localhost/network-interfaces/net1"


# Start the VM
echo "Starting VM..."
sleep 0.015s
sudo curl -X PUT --unix-socket "$API_SOCKET" --data-binary "{
    \"action_type\": \"InstanceStart\"
}" "http://localhost/actions"

# Wait for the VM to boot
sleep 2s

# Configure internet and DNS inside the VM
ssh -o "StrictHostKeyChecking=no" -i "$ROOTFS_KEY" root@"$TAP_IP" "ip route add default via 172.16.0.1 dev eth0"
ssh -o "StrictHostKeyChecking=no" -i "$ROOTFS_KEY" root@"$TAP_IP" "echo 'nameserver 8.8.8.8' > /etc/resolv.conf"

# Final output
cat << EOF
---------------------------------------
TAP $SB_ID CREATED AND VM CONFIGURED!
TAP NAME: ${TAP_DEV}
IP: ${TAP_IP}
MAC: ${FC_MAC}
LOG FILE: ${LOGFILE}
SOCKET: ${API_SOCKET}
---------------------------------------
Run the below command to SSH into the VM:
ssh -o "StrictHostKeyChecking=no" -i $ROOTFS_KEY root@$TAP_IP
EOF
