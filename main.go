package main

import (
	"context"
	"fmt"
	"log"
	"os"

	"github.com/firecracker-microvm/firecracker-go-sdk"
)

func main() {
	// Define paths for the kernel and rootfs
	kernelPath := "./vmlinux"     // Replace with your kernel image path
	rootfsPath := "./rootfs.ext4" // Replace with your rootfs path

	// Set up the logger
	logger := log.New(os.Stdout, "firecracker", log.LstdFlags|log.Lmicroseconds)

	// Create a context for the microVM
	ctx := context.Background()

	// Configure Firecracker machine
	machineConfig := firecracker.MachineConfig{
		VcpuCount:  1,
		MemSizeMib: 128,
		HtEnabled:  false,
	}

	// Define drive configuration
	drive := firecracker.BlockDevice{
		HostPath:     firecracker.String(rootfsPath),
		Mode:         "rw",
		IsRootDevice: firecracker.Bool(true),
		IsReadOnly:   firecracker.Bool(false),
	}

	// Define network configuration
	network := firecracker.NetworkInterface{
		MacAddress:  "AA:FC:00:00:00:01",
		HostDevName: "tap0", // Ensure tap0 is created and configured on the host
		AllowMMDS:   true,
	}

	// Define VM configuration
	vmConfig := firecracker.Config{
		SocketPath:      "firecracker.sock",
		LogFifo:         "firecracker-log.fifo",
		MetricsFifo:     "firecracker-metrics.fifo",
		KernelImagePath: kernelPath,
		MachineCfg:      machineConfig,
		Drives:          []firecracker.BlockDevice{drive},
		NetworkInterfaces: []firecracker.NetworkInterface{
			network,
		},
	}

	// Create a new Firecracker VM
	cmd := firecracker.VMCommandBuilder{}.
		WithBin("/usr/local/bin/firecracker").
		WithSocketPath("firecracker.sock").
		Build(ctx)

	machine, err := firecracker.NewMachine(ctx, vmConfig, firecracker.WithLogger(logger), firecracker.WithProcessRunner(cmd))
	if err != nil {
		log.Fatalf("Failed to create Firecracker machine: %v", err)
	}

	// Start the machine
	if err := machine.Start(ctx); err != nil {
		log.Fatalf("Failed to start Firecracker machine: %v", err)
	}
	defer func() {
		// Clean up the VM after execution
		if err := machine.StopVMM(); err != nil {
			log.Printf("Failed to stop Firecracker machine: %v", err)
		}
	}()

	// Run a command inside the VM
	command := "/bin/echo 'Hello from Firecracker!'"
	result, err := machine.Handlers.FcClient.ExecuteGuestCommand(ctx, firecracker.ExecuteGuestCommandInput{
		Command: command,
	})
	if err != nil {
		log.Fatalf("Failed to execute command in Firecracker machine: %v", err)
	}

	fmt.Printf("Command output: %s\n", result.Stdout)
}
