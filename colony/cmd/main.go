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

	// Create a Task
	task := tasks.Task{
		ID:      "123",
		Command: "echo",
		Args:    []string{"Hello, Exzet!"},
	}

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

	out, err := provision.SendTask(fmt.Sprintf("%v", machineIp), task)
	if err != nil {
		fmt.Printf("Error sending task: %v\n", err)
	} else {
		fmt.Printf("Task executed successfully: %v\n", out)
	}
}
