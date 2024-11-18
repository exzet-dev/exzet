package provision

import (
	"bytes"
	"encoding/json"
	"exzet-colony/pkg/tasks"
	"fmt"
	"io"
	"net/http"
)

// SendTask sends a Task to a specified VM's spawn service and returns the output.
func SendTask(vmIP string, task tasks.Task) (string, error) {
	// Construct the URL for the spawn service
	url := fmt.Sprintf("http://%s:8080/task", vmIP)

	// Marshal the Task into JSON
	payload, err := json.Marshal(task)
	if err != nil {
		return "", fmt.Errorf("failed to marshal task: %w", err)
	}

	// Make the HTTP POST request
	resp, err := http.Post(url, "application/json", bytes.NewReader(payload))
	if err != nil {
		return "", fmt.Errorf("failed to send HTTP POST request: %w", err)
	}
	defer resp.Body.Close()

	// Read the response body
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", fmt.Errorf("failed to read response body: %w", err)
	}

	// Check for non-2xx status codes
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return "", fmt.Errorf("unexpected response status %d: %s", resp.StatusCode, string(body))
	}

	// Decode the JSON response
	var result map[string]string
	err = json.Unmarshal(body, &result)
	if err != nil {
		return "", fmt.Errorf("failed to decode JSON response: %w", err)
	}

	// Return the "output" field from the response
	output, ok := result["output"]
	if !ok {
		return "", fmt.Errorf("response does not contain 'output' field: %s", string(body))
	}

	return output, nil
}
