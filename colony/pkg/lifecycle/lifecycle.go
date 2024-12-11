package lifecycle

import (
	"context"
	"exzet-colony/pkg/config"
	"exzet-colony/pkg/provision"
	"exzet-colony/pkg/tasks"
	"exzet-colony/pkg/utils"
	"fmt"
	"net/http"
	"os"
	"os/exec"
	"strings"
	"time"

	sdk "github.com/firecracker-microvm/firecracker-go-sdk"
)

type NetworkConfig struct {
	IP         string
	DefaultGW  string
	DNSServers []string
	WaitTime   time.Duration
}

func CreateVM(name string) (*sdk.Machine, config.VMConfig, error) {
	vmCfg := config.VMConfig{
		Name:            name,
		VcpuCount:       2,
		MemSizeMib:      256,
		Smt:             false,
		SocketPath:      fmt.Sprintf("/tmp/%s.socket", name),
		BinPath:         utils.GetResourcePath("firecracker"),
		KernelImagePath: utils.GetResourcePath("vmlinux"),
		RootfsPath:      utils.GetResourcePath("workspace.img"),
		InitrdPath:      utils.GetResourcePath("initramfs.cpio.xz"),
		NetworkName:     "fcnet",
		Subnet:          "10.168.0.0/24",
		TAPName:         "veth0",
		VMIP:            "10.168.0.2",
		CNIConfDir:      utils.GetResourcePath("cni.conf"),
		CNIBinDir:       utils.GetResourcePath("bin"),
		SSHKeyPath:      utils.GetResourcePath("rootfs.id_rsa"),
	}

	if err := setupVM(&vmCfg); err != nil {
		return nil, vmCfg, fmt.Errorf("VM setup failed: %w", err)
	}

	machine, err := startVM(&vmCfg)
	if err != nil {
		return nil, vmCfg, fmt.Errorf("VM start failed: %w", err)
	}

	if err := validateVM(machine); err != nil {
		StopVM(vmCfg.SocketPath)
		return nil, vmCfg, fmt.Errorf("VM validation failed: %w", err)
	}

	fmt.Printf("VM %s started successfully with IP %s\n", name, machine.Cfg.NetworkInterfaces[0].StaticConfiguration.IPConfiguration.IPAddr.IP)
	return machine, vmCfg, nil
}

func setupVM(vmCfg *config.VMConfig) error {
	if err := validateKernel(vmCfg.KernelImagePath); err != nil {
		return fmt.Errorf("kernel validation failed: %w", err)
	}

	logResourcePaths(vmCfg)

	if err := cleanupExisting(vmCfg); err != nil {
		return err
	}

	return config.WriteCNIConf(vmCfg.CNIConfDir, vmCfg.NetworkName, vmCfg.Subnet)
}

func startVM(vmCfg *config.VMConfig) (*sdk.Machine, error) {
	ctx := context.Background()
	cmd := sdk.VMCommandBuilder{}.
		WithSocketPath(vmCfg.SocketPath).
		WithBin(vmCfg.BinPath).
		Build(ctx)

	sdkCfg := vmCfg.CreateMachineConfig()
	machine, err := sdk.NewMachine(ctx, sdkCfg, sdk.WithProcessRunner(cmd))
	if err != nil {
		return nil, fmt.Errorf("failed to create machine: %w", err)
	}

	if err = machine.Start(ctx); err != nil {
		return nil, fmt.Errorf("failed to start machine: %w", err)
	}

	return machine, nil
}

func validateVM(machine *sdk.Machine) error {
	machineIP := machine.Cfg.NetworkInterfaces[0].StaticConfiguration.IPConfiguration.IPAddr.IP.String()

	if err := waitForVMServer(machineIP, 10, 2*time.Second); err != nil {
		return fmt.Errorf("server check failed: %w", err)
	}

	if err := verifyNetworking(machine); err != nil {
		return fmt.Errorf("network verification failed: %w", err)
	}

	return nil
}

func cleanupExisting(vmCfg *config.VMConfig) error {
	if err := os.Remove(vmCfg.SocketPath); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("socket cleanup failed: %w", err)
	}

	if err := config.CleanupCNI(vmCfg); err != nil {
		fmt.Printf("Warning: CNI cleanup failed: %v\n", err)
	}

	return nil
}

func cleanupStaleInterfaces() error {
	out, err := exec.Command("ip", "link", "show").Output()
	if err != nil {
		return fmt.Errorf("failed to list interfaces: %v", err)
	}

	for _, line := range strings.Split(string(out), "\n") {
		if !strings.Contains(line, "veth") {
			continue
		}

		parts := strings.Split(line, ":")
		if len(parts) < 2 {
			continue
		}

		ifName := strings.TrimSpace(parts[1])
		if _, err := exec.Command("ip", "link", "delete", ifName).Output(); err != nil {
			fmt.Printf("Warning: Failed to delete interface %s: %v\n", ifName, err)
		}
	}

	return nil
}

func verifyNetworking(machine *sdk.Machine) error {
	machineIP := machine.Cfg.NetworkInterfaces[0].StaticConfiguration.IPConfiguration.IPAddr.IP
	task := tasks.Task{
		ID:      "network-verify",
		Command: "sh",
		Args:    []string{"-c", "ping -c 1 -W 5 8.8.8.8 && ping -c 1 -W 5 google.com"},
	}

	if _, err := provision.SendTask(machineIP.String(), task); err != nil {
		return fmt.Errorf("network verification failed: %w", err)
	}
	return nil
}

func waitForVMServer(vmIP string, maxAttempts int, timeout time.Duration) error {
	client := &http.Client{Timeout: 2 * time.Second}

	for attempt := 1; attempt <= maxAttempts; attempt++ {
		resp, err := client.Head(fmt.Sprintf("http://%s:8080/health", vmIP))
		if err == nil {
			resp.Body.Close()
			if resp.StatusCode == http.StatusOK {
				return nil
			}
		}
		fmt.Printf("Waiting for VM server (attempt %d/%d)...\n", attempt, maxAttempts)
		time.Sleep(timeout)
	}

	return fmt.Errorf("server failed to respond after %d attempts", maxAttempts)
}

func validateKernel(kernelPath string) error {
	f, err := os.Open(kernelPath)
	if err != nil {
		return fmt.Errorf("kernel not found at %s: %w", kernelPath, err)
	}
	defer f.Close()

	magic := make([]byte, 4)
	if _, err := f.Read(magic); err != nil {
		return fmt.Errorf("cannot read kernel header: %w", err)
	}

	if magic[0] != 0x7F || magic[1] != 0x45 || magic[2] != 0x4C || magic[3] != 0x46 {
		return fmt.Errorf("invalid kernel format: not an ELF file")
	}

	return nil
}

func logResourcePaths(vmCfg *config.VMConfig) {
	fmt.Printf("Using kernel at: %s\n", vmCfg.KernelImagePath)
	fmt.Printf("Using rootfs at: %s\n", vmCfg.RootfsPath)
	if vmCfg.InitrdPath != "" {
		fmt.Printf("Using initrd at: %s\n", vmCfg.InitrdPath)
	}
}

func StopVM(socketPath string) error {
	ctx := context.Background()
	cmd := sdk.VMCommandBuilder{}.WithSocketPath(socketPath).Build(ctx)
	machine, err := sdk.NewMachine(ctx, sdk.Config{SocketPath: socketPath}, sdk.WithProcessRunner(cmd))
	if err != nil {
		return fmt.Errorf("failed to connect to machine: %w", err)
	}

	if err = machine.Shutdown(ctx); err != nil {
		return fmt.Errorf("failed to shutdown machine: %w", err)
	}

	if err := cleanupStaleInterfaces(); err != nil {
		fmt.Printf("Warning: Failed to cleanup interfaces: %v\n", err)
	}

	fmt.Printf("VM with socket path %s has been shut down\n", socketPath)
	return nil
}
