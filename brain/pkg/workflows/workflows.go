package workflows

import (
	"encoding/json"
	"net/http"
)

type Workflow struct {
	ID     string   `json:"id"`
	Name   string   `json:"name"`
	Status string   `json:"status"`
	Nodes  []string `json:"nodes"`
}

func ListWorkflowsHandler(w http.ResponseWriter, r *http.Request) {
	workflows := []Workflow{
		{ID: "1", Name: "Build NoteTakingApp", Status: "pending", Nodes: []string{"node1", "node2"}},
	}
	json.NewEncoder(w).Encode(workflows)
}

func CreateWorkflowHandler(w http.ResponseWriter, r *http.Request) {
	var workflow Workflow
	err := json.NewDecoder(r.Body).Decode(&workflow)
	if err != nil {
		http.Error(w, "Invalid request", http.StatusBadRequest)
		return
	}
	workflow.ID = "2" // Example
	workflow.Status = "created"
	json.NewEncoder(w).Encode(workflow)
}
