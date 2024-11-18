package config

import (
	"fmt"
	"os"
	"path/filepath"

	sdk "github.com/firecracker-microvm/firecracker-go-sdk"
	models "github.com/firecracker-microvm/firecracker-go-sdk/client/models"
)

type VMConfig struct {
	Name            string
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
	VcpuCount       int64
	MemSizeMib      int64
	Smt             bool
}

func (cfg *VMConfig) CreateMachineConfig() sdk.Config {
	return sdk.Config{
		SocketPath:      cfg.SocketPath,
		KernelImagePath: cfg.KernelImagePath,
		MachineCfg: models.MachineConfiguration{
			VcpuCount:  &cfg.VcpuCount,
			MemSizeMib: &cfg.MemSizeMib,
			Smt:        &cfg.Smt,
		},
		Drives: []models.Drive{
			{
				DriveID:      sdk.String("root"),
				PathOnHost:   &cfg.RootfsPath,
				IsRootDevice: sdk.Bool(true),
				IsReadOnly:   sdk.Bool(false),
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
				AllowMMDS: true,
			},
		},
	}
}

func WriteCNIConf(cniConfDir, networkName, subnet string) error {
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
