package main

import (
	"encoding/json"
	"fmt"
	"net/http"
	"time"
)

// StatusResponse represents the status of Hive and Brain
type StatusResponse struct {
	HiveStatus  string `json:"hiveStatus"`
	BrainStatus string `json:"brainStatus"`
}

// JobRequest represents the incoming request to start a job
type JobRequest struct {
	Job string `json:"job"`
}

// JobResponse represents the response after starting a job
type JobResponse struct {
	JobID string `json:"jobId"`
}

func main() {
	fmt.Println("Exzet Hive: Backend Service")

	// Define API routes
	http.HandleFunc("/api/status", handleStatus)
	http.HandleFunc("/api/start-job", handleStartJob)

	// Serve the Svelte frontend from the "hiveview/build" directory
	staticDir := "hiveview/build"
	fs := http.FileServer(http.Dir(staticDir))
	http.Handle("/", fs)

	// Start the server
	port := "8080"
	fmt.Printf("Hive backend running on port %s\n", port)
	err := http.ListenAndServe(":"+port, nil)
	if err != nil {
		fmt.Printf("Error starting server: %v\n", err)
	}
}

// handleStatus handles the `/api/status` endpoint
func handleStatus(w http.ResponseWriter, r *http.Request) {
	// Simulate fetching the statuses
	status := StatusResponse{
		HiveStatus:  "Online",
		BrainStatus: "Idle",
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(status)
}

// handleStartJob handles the `/api/start-job` endpoint
func handleStartJob(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Decode the incoming JSON request
	var jobReq JobRequest
	err := json.NewDecoder(r.Body).Decode(&jobReq)
	if err != nil || jobReq.Job == "" {
		http.Error(w, "Invalid job request", http.StatusBadRequest)
		return
	}

	// Simulate starting a job (e.g., generating a job ID)
	jobID := fmt.Sprintf("%d", time.Now().UnixNano())

	// Respond with the job ID
	jobResp := JobResponse{
		JobID: jobID,
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(jobResp)

	fmt.Printf("Job started: %s\n", jobReq.Job)
}
