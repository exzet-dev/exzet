# **Exzet Architecture**

Exzet is designed as a modular, distributed task orchestration framework. This document outlines its core components and how they interact.

## **System Overview**
Exzet’s architecture consists of the following core services:
1. **Brain**: The central orchestrator for workflows and pipelines.
2. **Colony**: Delegates tasks to VM pools and manages their lifecycles.
3. **Spawn**: Executes tasks in isolated environments (Firecracker microVMs).
4. **Hive**: Serves as the backend for the web and CLI clients.
5. **HiveView**: The user-facing frontend for interacting with workflows and jobs.

### **Core Interactions**
```plaintext
[ HiveView ] <-> [ Hive ] <-> [ Brain ] <-> [ Colony ] <-> [ Spawn ]
```

1. **HiveView**:
   - Provides an intuitive user interface for creating workflows, monitoring jobs, and reviewing logs.
2. **Hive**:
   - Handles API requests, user authentication, and database operations.
3. **Brain**:
   - Orchestrates workflows and allocates resources.
   - Communicates with Colony to delegate tasks.
4. **Colony**:
   - Manages VM pools and provisions `Spawn` nodes.
   - Handles resource allocation and monitoring.
5. **Spawn**:
   - Executes tasks in isolated Firecracker microVMs.
   - Streams logs and results back to Brain or Colony.

---

## **Component Responsibilities**

### **1. Brain**
- Orchestrates workflows and pipelines.
- Tracks job status and manages task dependencies.
- Communicates with Colony and Spawn via messaging protocols.

### **2. Colony**
- Spins up/down Firecracker microVMs as `Spawn` nodes.
- Manages the lifecycle and provisioning of VM resources.
- Delegates tasks to Spawn nodes for execution.

### **3. Spawn**
- Executes tasks in isolated microVMs.
- Streams logs and results back to Colony or Brain.
- Cleans up resources after execution.

### **4. Hive**
- Provides API endpoints for HiveView and CLI clients.
- Manages user sessions and database interactions.
- Bridges user requests to Brain workflows.

### **5. HiveView**
- Displays an intuitive UI for managing workflows, pipelines, and jobs.
- Provides real-time job monitoring and log streaming.

---

## **Technologies Used**
- **Firecracker**: Lightweight microVMs for task isolation.
- **Go**: Used for Brain, Colony, Spawn, and Hive services.
- **SvelteKit**: Frontend framework for HiveView.
- **SQLite**: Default database for Hive, with ORM for other databases.
- **Docker**: For local development and CI pipelines.
