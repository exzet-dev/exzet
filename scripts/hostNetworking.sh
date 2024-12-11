#!/bin/bash

# ENABLE ERROR HANDLING
set -euo pipefail
trap 'echo "Error on line $LINENO. Exit code: $?"' ERR

# BACKUP FUNCTION
backup_configs() {
    local BACKUP_DIR="/root/network_backup_$(date +%Y%m%d_%H%M%S)"
    echo "[BACKUP] Creating backup at ${BACKUP_DIR}"
    mkdir -p "${BACKUP_DIR}"
    
    # BACKUP EXISTING CONFIGS
    [ -f /etc/nftables.conf ] && cp /etc/nftables.conf "${BACKUP_DIR}/"
    [ -d /etc/nftables.d ] && cp -r /etc/nftables.d "${BACKUP_DIR}/"
    [ -d /etc/systemd/network ] && cp -r /etc/systemd/network "${BACKUP_DIR}/"
    [ -f /etc/docker/daemon.json ] && cp /etc/docker/daemon.json "${BACKUP_DIR}/"
    [ -f /etc/qemu-ifup ] && cp /etc/qemu-ifup "${BACKUP_DIR}/"
}

# VERIFY FUNCTION
verify_network() {
    echo "[VERIFY] Checking network configuration..."
    
    # CHECK INTERFACE STATUS
    ip link show br0 >/dev/null 2>&1 || { echo "ERROR: br0 not found"; return 1; }
    ip link show docker0 >/dev/null 2>&1 || echo "WARNING: docker0 not found (normal if Docker isn't running)"
    
    # CHECK FORWARDING
    local FWD
    FWD=$(sysctl -n net.ipv4.ip_forward)
    [ "$FWD" -eq 1 ] || { echo "ERROR: IP forwarding not enabled"; return 1; }
    
    # CHECK BRIDGE ADDRESS
    ip addr show br0 | grep -q "inet.*10.168.0.1/24" || { 
        echo "[SETUP] Adding VM network address to br0"
        ip addr add 10.168.0.1/24 dev br0 2>/dev/null || true
    }
    
    # VERIFY CONNECTIVITY
    ping -c 1 -W 2 8.8.8.8 >/dev/null 2>&1 || { echo "ERROR: No internet connectivity"; return 1; }
    
    echo "[VERIFY] Network checks passed"
    return 0
}

# MAIN SETUP FUNCTION
setup_network() {
    echo "[SETUP] Starting network configuration..."
    
    # DISABLE NETWORKMANAGER
    systemctl stop NetworkManager
    systemctl disable NetworkManager
    
    # ENABLE SYSTEMD-NETWORKD
    systemctl enable systemd-networkd
    systemctl start systemd-networkd
    
    # CREATE BRIDGE CONFIG
    cat > /etc/systemd/network/br0.netdev << 'EOF'
[NetDev]
Name=br0
Kind=bridge

[Bridge]
STP=no
ForwardDelaySec=0
HelloTimeSec=0
EOF

    # BRIDGE NETWORK CONFIG
    cat > /etc/systemd/network/br0.network << 'EOF'
[Match]
Name=br0

[Network]
DHCP=yes
IPForward=yes
DNS=8.8.8.8

[Address]
Address=10.168.0.1/24

[DHCP]
RouteMetric=10
UseMTU=true
EOF

    # PHYSICAL INTERFACE CONFIG
    cat > /etc/systemd/network/enp8s0.network << 'EOF'
[Match]
Name=enp8s0

[Network]
Bridge=br0
EOF

    # NFTABLES CONFIG
    cat > /etc/nftables.conf << 'EOF'
#!/usr/sbin/nft -f

flush ruleset

table bridge filter {
    chain input { type filter hook input priority 0; policy accept; }
    chain forward { type filter hook forward priority 0; policy accept; }
    chain output { type filter hook output priority 0; policy accept; }
}

table ip nat {
    chain postrouting {
        type nat hook postrouting priority 100;
        # VM NAT
        ip saddr 10.168.0.0/24 oif br0 masquerade
        # DOCKER NAT
        ip saddr 172.17.0.0/16 oif br0 masquerade
    }
}

table ip filter {
    chain forward {
        type filter hook forward priority 0;
        # VM TRAFFIC
        iifname "tap*" oifname "br0" accept
        iifname "br0" oifname "tap*" ct state related,established accept
        # DOCKER TRAFFIC
        iifname "docker0" oifname "br0" accept
        iifname "br0" oifname "docker0" ct state related,established accept
    }
}
EOF

    # DOCKER CONFIG
    mkdir -p /etc/docker
    cat > /etc/docker/daemon.json << 'EOF'
{
    "iptables": false,
    "bridge": "docker0",
    "ip": "172.17.0.1",
    "fixed-cidr": "172.17.0.0/16"
}
EOF

    # QEMU NETWORK SCRIPT
    cat > /etc/qemu-ifup << 'EOF'
#!/bin/bash
ip link set dev $1 up
ip link set $1 master br0
EOF
    chmod +x /etc/qemu-ifup

    # ENABLE IP FORWARDING
    echo "net.ipv4.ip_forward=1" > /etc/sysctl.d/99-ip-forward.conf
    sysctl -p /etc/sysctl.d/99-ip-forward.conf

    # RESTART SERVICES
    systemctl restart systemd-networkd
    systemctl restart nftables
    systemctl restart docker || true
}

# MAIN EXECUTION
main() {
    echo "[START] Network recovery script starting..."
    
    # CREATE BACKUP
    backup_configs
    
    # SETUP NETWORK
    setup_network
    
    # VERIFY SETUP
    if verify_network; then
        echo "[SUCCESS] Network setup completed successfully"
        echo "Host IP (br0): $(ip -4 addr show br0 | grep -oP '(?<=inet\s)\d+(\.\d+){3}')"
        echo "VM Gateway: 10.168.0.1"
    else
        echo "[ERROR] Network setup failed. Check logs and try again"
        echo "To restore from backup, check: /root/network_backup_*"
        exit 1
    fi
}

# RUN SCRIPT
main "$@"
