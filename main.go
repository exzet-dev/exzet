package main

import (
	"context"
	"log"
	"os"
	"path/filepath"
	"time"

	"github.com/exzet-dev/exzet/pkg/logger"
	"github.com/exzet-dev/exzet/pkg/task"
	"github.com/exzet-dev/exzet/pkg/vm"
)

func main() {
	if err := logger.Init("exzet.log"); err != nil {
		log.Fatalf("Failed to initialize logger: %v", err)
	}
	logger.Log.Info("Starting Exzet pipeline manager...")

	// Setup VM configuration
	dir, err := os.Getwd()
	if err != nil {
		log.Fatalf("Failed to get current directory: %v", err)
	}

	resourceDir := filepath.Join(dir, "resources")
	socketDir, err := os.MkdirTemp("", "FC_SOCKET_DIR")
	if err != nil {
		log.Fatal(err)
	}
	vmConfig := vm.VMConfig{
		SocketPath:      filepath.Join(socketDir, "firecracker.sock"),
		KernelImagePath: filepath.Join(resourceDir, "vmlinux"),
		RootfsPath:      filepath.Join(resourceDir, "ubuntu-22.04.ext4"),
		SSHKeyPath:      filepath.Join(resourceDir, "ubuntu-22.04.id_rsa"),
		CNIConfDir:      filepath.Join(dir, "cni.conf"),
		CNIBinDir:       filepath.Join(dir, "bin"),
		NetworkName:     "fcnet",
		Subnet:          "10.168.0.0/24",
		TAPName:         "veth0",
		VMIP:            "10.168.0.2",
	}

	// Start the VM
	ctx := context.Background()
	machine, err := vm.StartVM(ctx, vmConfig)
	if err != nil {
		log.Fatalf("Failed to start VM: %v", err)
	}
	defer func() {
		if err := machine.StopVMM(); err != nil {
			log.Printf("Error stopping VM: %v", err)
		}
	}()

	// Wait for the VM to boot
	time.Sleep(5 * time.Second)

	defer func() {
		if err := machine.StopVMM(); err != nil {
			log.Fatal(err)
		}
	}()
	defer func() {
		if err := machine.Shutdown(ctx); err != nil {
			log.Fatal(err)
		}
	}()

	// Connect to the VM via SSH
	client, err := task.ConnectToVM(machine, vmConfig.SSHKeyPath)
	if err != nil {
		log.Fatalf("Failed to connect to VM via SSH: %v", err)
	}
	defer client.Close()

	// Execute a task inside the VM
	// exampleTask := &task.Task{
	// 	Command: "echo Hello from Exzet!",
	// }

	output, err := task.RunCommandInVM(client, "echo Hello from Exzet! && ls -a && pwd && id && env")
	if err != nil {
		logger.Log.Fatalf("Failed to run task in VM: %v", err)
	}
	logger.Log.Infof("Task Output: %s", output)
	log.Printf("Task Output: %s", output)
	log.Println("Exzet pipeline manager completed successfully.")
}
