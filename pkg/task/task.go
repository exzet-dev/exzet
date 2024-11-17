package task

import (
	"errors"
	"fmt"
	"os"
	"time"

	sdk "github.com/firecracker-microvm/firecracker-go-sdk"
	"golang.org/x/crypto/ssh"
)

// Task defines a structure for executing a command inside a VM.
type Task struct {
	Command string
}

func ConnectToVM(m *sdk.Machine, sshKeyPath string) (*ssh.Client, error) {
	key, err := os.ReadFile(sshKeyPath)
	if err != nil {
		return nil, err
	}

	signer, err := ssh.ParsePrivateKey(key)
	if err != nil {
		return nil, err
	}

	config := &ssh.ClientConfig{
		User: "root",
		Auth: []ssh.AuthMethod{
			ssh.PublicKeys(signer),
		},
		HostKeyCallback: ssh.InsecureIgnoreHostKey(),
		Timeout:         5 * time.Second,
	}

	if len(m.Cfg.NetworkInterfaces) == 0 {
		return nil, errors.New("no network interfaces")
	}

	ip := m.Cfg.NetworkInterfaces[0].StaticConfiguration.IPConfiguration.IPAddr.IP // IP of VM

	return ssh.Dial("tcp", fmt.Sprintf("%s:22", ip), config)
}

func RunCommandInVM(client *ssh.Client, command string) (string, error) {
	session, err := client.NewSession()
	if err != nil {
		return "", fmt.Errorf("failed to create SSH session: %v", err)
	}
	defer session.Close()

	output, err := session.CombinedOutput(command)
	if err != nil {
		return "", fmt.Errorf("command failed: %v", err)
	}
	return string(output), nil
}
