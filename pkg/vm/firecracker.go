package vm

import (
	"context"
	"fmt"
	"log"
	"os"
	"path/filepath"

	sdk "github.com/firecracker-microvm/firecracker-go-sdk"
	"github.com/firecracker-microvm/firecracker-go-sdk/client/models"
)

type VMConfig struct {
	SocketPath      string
	BinPath         string
	KernelImagePath string
	RootfsPath      string
	SSHKeyPath      string
	CNIConfDir      string
	CNIBinDir       string
	NetworkName     string
	Subnet          string
	TAPName         string
	VMIP            string
}

// StartVM starts a Firecracker microVM with the specified configuration.
func StartVM(ctx context.Context, cfg VMConfig) (*sdk.Machine, error) {
	log.Println("Starting Firecracker VM...")

	// Write CNI configuration
	err := writeCNIConf(cfg.CNIConfDir, cfg.NetworkName, cfg.Subnet)
	if err != nil {
		return nil, fmt.Errorf("failed to write CNI config: %v", err)
	}

	// Define the machine configuration
	vcpuCount := int64(2)
	memSizeMib := int64(512)
	smt := false
	rootDriveID := "root"
	isRootDevice := true
	isReadOnly := false

	machineCfg := sdk.Config{
		SocketPath:      cfg.SocketPath,
		KernelImagePath: cfg.KernelImagePath,
		MachineCfg: models.MachineConfiguration{
			VcpuCount:  &vcpuCount,
			MemSizeMib: &memSizeMib,
			Smt:        &smt,
		},
		Drives: []models.Drive{
			{
				DriveID:      &rootDriveID,
				IsRootDevice: &isRootDevice,
				IsReadOnly:   &isReadOnly,
				PathOnHost:   &cfg.RootfsPath,
			},
		},
		NetworkInterfaces: []sdk.NetworkInterface{
			{
				CNIConfiguration: &sdk.CNIConfiguration{
					NetworkName: cfg.NetworkName,
					IfName:      cfg.TAPName,
					ConfDir:     cfg.CNIConfDir,
					BinPath:     []string{cfg.CNIBinDir},
					VMIfName:    "eth0",
				},
				// StaticConfiguration: &sdk.StaticNetworkConfiguration{
				// 	IPConfiguration: &sdk.IPConfiguration{
				// 		IPAddr: net.IPNet{
				// 			IP:   net.ParseIP(cfg.VMIP),
				// 			Mask: net.CIDRMask(24, 32),
				// 		},
				// 		Gateway:     net.ParseIP("10.168.0.1"),
				// 		Nameservers: []string{"8.8.8.8"},
				// 	},
				// },
			},
		},
	}

	// Create the machine
	machine, err := sdk.NewMachine(ctx, machineCfg)
	if err != nil {
		return nil, fmt.Errorf("failed to create Firecracker machine: %v", err)
	}

	// Start the VM
	err = machine.Start(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to start Firecracker machine: %v", err)
	}

	log.Println("Firecracker VM started successfully.")
	return machine, nil
}

// writeCNIConf writes the CNI configuration to disk.
func writeCNIConf(cniConfDir, networkName, subnet string) error {
	// Ensure the directory exists
	err := os.MkdirAll(cniConfDir, 0755)
	if err != nil {
		return fmt.Errorf("failed to create CNI config directory: %v", err)
	}

	// Construct the path for the CNI config file
	cniConfPath := filepath.Join(cniConfDir, fmt.Sprintf("%s.conflist", networkName))
	cniVersion := "0.3.1"
	// Write the configuration to the file
	conf := fmt.Sprintf(`{
		"cniVersion": "%s",
		"name": "%s",
		"plugins": [
			{
				"type": "ptp",
				"ipam": {
					"type": "host-local",
					"subnet": "%s"
				}
			},
			{
				"type": "tc-redirect-tap"
			}
		]
	}`, cniVersion, networkName, subnet)

	err = os.WriteFile(cniConfPath, []byte(conf), 0644)
	if err != nil {
		return fmt.Errorf("failed to write CNI config file: %v", err)
	}

	return nil
}
