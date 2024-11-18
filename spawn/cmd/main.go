package main

import (
	"exzet-spawn/pkg/tasks"
	"fmt"
	"log"
	"net/http"
)

func main() {
	fmt.Println("Exzet Spawn: Task Executor")
	http.HandleFunc("/task", tasks.HandleTask)

	port := "8080"
	fmt.Printf("Spawn service listening on port %s\n", port)
	log.Fatal(http.ListenAndServe(":"+port, nil))
}
