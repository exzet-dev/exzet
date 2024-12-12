#!/bin/bash
# Clean up networking
ip link | grep -oP 'veth\w+' | xargs -I {} ip link delete {}
ip netns list | xargs -L1 ip netns delete
ip link | grep tap | awk '{print $2}' | tr -d ':' | xargs -I {} ip link delete {}

# Clean up CNI state
rm -rf /var/lib/cni/networks/*
rm -rf /var/lib/cni/fc-*

# Clean up iptables
iptables-save | grep CNI | awk '{print $2}' | while read chain; do
    iptables -t nat -F $chain 2>/dev/null
    iptables -t nat -X $chain 2>/dev/null
    iptables -t mangle -F $chain 2>/dev/null
    iptables -t mangle -X $chain 2>/dev/null
    iptables -F $chain 2>/dev/null
    iptables -X $chain 2>/dev/null
done