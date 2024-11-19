package main

import (
	"exzet-colony/pkg/lifecycle"
	"exzet-colony/pkg/provision"
	"exzet-colony/pkg/tasks"
	"exzet-colony/pkg/utils"
	"fmt"
	"log"
	"os"
)

func main() {
	// Write resources to accessible location
	cwd, err := os.Getwd()
	if err != nil {
		fmt.Printf("Error unpacking resources: %v\n", err)
		return
	}

	utils.WriteAllEmbeddedResources(fmt.Sprintf("%v/resources", cwd))

	// Send the Task to a node
	m, cfg, err := lifecycle.CreateVM("TESTVM0001")
	if err != nil {
		fmt.Printf("Error creating VM: %v\n", err)
		return
	}

	machineIp := m.Cfg.NetworkInterfaces[0].StaticConfiguration.IPConfiguration.IPAddr.IP

	defer func() {
		if err := lifecycle.StopVM(cfg.SocketPath); err != nil {
			log.Fatal(err)
		}
	}()

	combinedCommand := fmt.Sprintf(`
	#!/bin/sh
	# Bring up the interface
	ip link set dev eth0 up &&
	# Assign IP address to the interface
	ip addr add %s dev eth0 &&
	# Add the default gateway
	ip route add default via 10.168.0.1 &&
	# Update the resolv.conf for DNS
	echo -e "nameserver 8.8.8.8\nnameserver 8.8.4.4" > /etc/resolv.conf
	`, machineIp)

	// Send the task to configure networking on the VM
	out, err := provision.SendTask(fmt.Sprintf("%v", machineIp), tasks.Task{
		ID:      "setup-network",
		Command: "sh",
		Args:    []string{"-c", combinedCommand},
	})
	if err != nil {
		fmt.Printf("Error during network configuration and verification: %v\n", err)
	} else {
		fmt.Printf("Network configuration and verification successful:\n%v\n", out)
	}

	combinedCommand = `sh -c "
		systemctl enable NetworkManager &&
		echo 'nameserver 8.8.8.8\nnameserver 8.8.4.4' > /etc/resolv.conf &&
		echo 'Updated /etc/resolv.conf' &&
		sleep 1 &&
		echo 'Contents of /etc/resolv.conf:' &&
		cat /etc/resolv.conf &&
		sleep 1 &&
		systemctl restart NetworkManager &&
		sleep 5 &&
		echo 'Pinging google.com...' &&
		ping -c 4 google.com &&
		echo 'Displaying IP configuration:' &&
		ip a &&
		echo 'Displaying routing table:' &&
		ip route &&
		echo 'Sleeping for 5 seconds...' &&
		sleep 1"`

	out, err = provision.SendTask(fmt.Sprintf("%v", machineIp), tasks.Task{
		ID:      "network-config-full",
		Command: "sh",
		Args:    []string{"-c", combinedCommand},
	})
	if err != nil {
		fmt.Printf("Error during network configuration and verification: %v\n", err)
	} else {
		fmt.Printf("Network configuration and verification successful:\n%v\n", out)
	}

	out, err = provision.SendTask(fmt.Sprintf("%v", machineIp), tasks.Task{
		ID:      "124",
		Command: "echo",
		Args:    []string{"I AM IN A VM MOFUGGGGGAAAA"},
	})
	if err != nil {
		fmt.Printf("Error sending task: %v\n", err)
	} else {
		fmt.Printf("Task executed successfully: %v\n", out)
	}

	out, err = provision.SendTask(fmt.Sprintf("%v", machineIp), tasks.Task{
		ID:      "125",
		Command: "ping",
		Args:    []string{"google.com"},
	})
	if err != nil {
		fmt.Printf("Error sending task: %v\n", err)
	} else {
		fmt.Printf("Task executed successfully: %v\n", out)
	}
}
