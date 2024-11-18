package tasks

import (
	"encoding/json"
	"net/http"
	"os/exec"
)

func HandleTask(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Invalid method", http.StatusMethodNotAllowed)
		return
	}

	var task Task
	err := json.NewDecoder(r.Body).Decode(&task)
	if err != nil {
		http.Error(w, "Failed to decode task", http.StatusBadRequest)
		return
	}

	// Execute the task
	cmd := exec.Command(task.Command, task.Args...)
	output, err := cmd.CombinedOutput()

	response := TaskResponse{
		ID:     task.ID,
		Output: string(output),
	}

	if err != nil {
		response.Error = err.Error()
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(response)
}
