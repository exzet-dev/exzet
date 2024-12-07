package lifecycle

import (
	"context"
	"fmt"
	"os"

	config "exzet-colony/pkg/config"
	"exzet-colony/pkg/utils"

	sdk "github.com/firecracker-microvm/firecracker-go-sdk"
)

// CreateVM creates and starts a new Firecracker microVM
func CreateVM(name string) (*sdk.Machine, config.VMConfig, error) {

	vmCfg := config.VMConfig{
		Name:            name,
		VcpuCount:       2,
		MemSizeMib:      256,
		Smt:             false,
		SocketPath:      fmt.Sprintf("/tmp/%s.socket", name),
		BinPath:         utils.GetResourcePath("firecracker"),
		KernelImagePath: utils.GetResourcePath("vmlinux"),
		RootfsPath:      utils.GetResourcePath("rootfs.ext4"),
		NetworkName:     "fcnet",
		Subnet:          "10.168.0.0/16",
		TAPName:         "veth0",
		VMIP:            "10.168.0.2",
		CNIConfDir:      utils.GetResourcePath("cni.conf"),
		CNIBinDir:       utils.GetResourcePath("bin"),
		SSHKeyPath:      utils.GetResourcePath("rootfs.id_rsa"),
	}

	// Delete existing socket
	err := os.Remove(vmCfg.SocketPath)
	if err != nil {
		fmt.Printf("Didnt delete socket: %v", err)
	}

	// Firecracker configuration
	sdkCfg := vmCfg.CreateMachineConfig()

	err = config.WriteCNIConf(vmCfg.CNIConfDir, vmCfg.NetworkName, vmCfg.Subnet)
	if err != nil {
		fmt.Printf("failed to write CNI config: %v", err)
		return nil, vmCfg, fmt.Errorf("failed to create config file: %w", err)
	}

	// Create the Firecracker command and machine
	cmd := sdk.VMCommandBuilder{}.WithSocketPath(vmCfg.SocketPath).WithBin(vmCfg.BinPath).Build(context.Background())
	machine, err := sdk.NewMachine(context.Background(), sdkCfg, sdk.WithProcessRunner(cmd))
	if err != nil {
		return nil, vmCfg, fmt.Errorf("failed to create machine: %w", err)
	}

	// Start the VM
	err = machine.Start(context.Background())
	if err != nil {
		return nil, vmCfg, fmt.Errorf("failed to start machine: %w", err)
	}

	fmt.Printf("VM %s started with socket path %s\n", name, vmCfg.SocketPath)
	return machine, vmCfg, nil
}

// StopVM gracefully shuts down the Firecracker microVM
func StopVM(socketPath string) error {
	ctx := context.Background()

	// Connect to the running machine
	cmd := sdk.VMCommandBuilder{}.WithSocketPath(socketPath).Build(ctx)
	machine, err := sdk.NewMachine(ctx, sdk.Config{SocketPath: socketPath}, sdk.WithProcessRunner(cmd))
	if err != nil {
		return fmt.Errorf("failed to connect to machine at %s: %w", socketPath, err)
	}

	// Shutdown the VM
	err = machine.Shutdown(ctx)
	if err != nil {
		return fmt.Errorf("failed to shutdown machine: %w", err)
	}

	fmt.Printf("VM with socket path %s has been shut down\n", socketPath)
	return nil
}
