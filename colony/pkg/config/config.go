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
	InitrdPath      string
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

func WriteCNIConf(cniConfDir, networkName, subnet string) error {
	err := os.MkdirAll(cniConfDir, 0755)
	if err != nil {
		return fmt.Errorf("failed to create CNI config directory: %v", err)
	}

	cniConfPath := filepath.Join(cniConfDir, fmt.Sprintf("%s.conflist", networkName))

	// MATCH THE EXAMPLE'S CNI CONFIG
	conf := fmt.Sprintf(`{
        "cniVersion": "0.3.1",
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
    }`, networkName, subnet)

	return os.WriteFile(cniConfPath, []byte(conf), 0644)
}

func (cfg *VMConfig) CreateMachineConfig() sdk.Config {
	// CREATE DRIVE CONFIG
	driveID := "root"
	isRootDevice := true
	isReadOnly := false

	return sdk.Config{
		SocketPath:      cfg.SocketPath,
		KernelImagePath: cfg.KernelImagePath,
		InitrdPath:      cfg.InitrdPath,
		MachineCfg: models.MachineConfiguration{
			VcpuCount:  &cfg.VcpuCount,
			MemSizeMib: &cfg.MemSizeMib,
			Smt:        &cfg.Smt,
		},
		Drives: []models.Drive{
			{
				DriveID:      &driveID,
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
				AllowMMDS: true,
			},
		},
	}
}

// ADD CLEANUP FOR CNI
func CleanupCNI(cfg *VMConfig) error {
	// Remove CNI cache directory
	cniCacheDir := "/var/lib/cni"
	if err := os.RemoveAll(cniCacheDir); err != nil {
		return fmt.Errorf("failed to cleanup CNI cache: %v", err)
	}

	// Remove CNI configuration
	cniConfPath := filepath.Join(cfg.CNIConfDir, fmt.Sprintf("%s.conflist", cfg.NetworkName))
	if err := os.RemoveAll(cniConfPath); err != nil {
		return fmt.Errorf("failed to remove CNI config: %v", err)
	}

	return nil
}
