package tasks

// Task represents a unit of work to be executed on a node.
type Task struct {
	ID      string
	Command string
	Args    []string
}

// NewTask creates and returns a new Task instance.
func NewTask(id, command string, args []string) Task {
	return Task{
		ID:      id,
		Command: command,
		Args:    args,
	}
}
