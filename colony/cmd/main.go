package main

import (
	"exzet-colony/pkg/lifecycle"
	"exzet-colony/pkg/provision"
	"exzet-colony/pkg/tasks"
	"exzet-colony/pkg/utils"
	"fmt"
	"log"
	"net/http"
	"os"
)

func main() {
	go startHealthCheckServer()

	// Write resources to accessible location
	cwd, err := os.Getwd()
	if err != nil {
		fmt.Printf("Error unpacking resources: %v\n", err)
		return
	}

	resDir := fmt.Sprintf("%v/resources", cwd)
	utils.WriteAllEmbeddedResources(resDir)
	utils.CleanupBackups(resDir)
	utils.CopySystemFiles(resDir)

	// Create and configure VM
	m, cfg, err := lifecycle.CreateVM("TESTVM0001")
	if err != nil {
		fmt.Printf("Error creating VM: %v\n", err)
		return
	}

	defer func() {
		if err := lifecycle.StopVM(cfg.SocketPath); err != nil {
			log.Fatal(err)
		}
	}()

	// Your task execution code can now be much simpler
	machineIP := m.Cfg.NetworkInterfaces[0].StaticConfiguration.IPConfiguration.IPAddr.IP
	out, err := provision.SendTask(machineIP.String(), tasks.Task{
		ID:      "test-task",
		Command: "echo",
		Args:    []string{"VM is ready for tasks!"},
	})
	if err != nil {
		fmt.Printf("Error sending task: %v\n", err)
	} else {
		fmt.Printf("Task executed successfully: %v\n", out)
	}
}

func startHealthCheckServer() {
	http.HandleFunc("/healthcheck", func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		fmt.Fprintln(w, "OK (COLONY ALIVE)")
	})

	port := "8086"
	log.Printf("Starting health check server on port %s\n", port)
	if err := http.ListenAndServe(":"+port, nil); err != nil {
		log.Fatalf("Health check server failed: %v\n", err)
	}
}
