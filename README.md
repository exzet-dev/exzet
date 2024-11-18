![xz_lowpoly_cropped](https://github.com/user-attachments/assets/3bcfbeb6-5c23-4ed9-ba7c-753b9e11e66d)

Exzet is a modern, lightweight distributed task orchestration framework, interface, and CI tool designed to automate complex workflows across distributed systems. Leveraging Firecracker microVMs and a modular architecture, Exzet provides an efficient and secure environment for executing tasks and building pipelines with high scalability and flexibility.

---

## **Table of Contents**
1. [Features](#features)
2. [Core Components](#core-components)
3. [Architecture Overview](#architecture-overview)
4. [Installation](#installation)
5. [Usage](#usage)
6. [Development](#development)
7. [Contributing](#contributing)
8. [License](#license)

---

## **Features**
- **MicroVM-based Execution**: Leverages Firecracker microVMs for secure, isolated task execution.
- **Distributed Architecture**: Decoupled services for orchestration, task delegation, and execution.
- **Modular Design**: Clear separation of concerns with reusable components.
- **Scalability**: Dynamically manages resources and workloads across distributed nodes.
- **Flexibility**: Support for custom workflows, pipelines, and task parameters.
- **User-Friendly Interface**: Intuitive SvelteKit-based frontend for managing workflows and monitoring jobs.
- **Secure and Lightweight**: Focus on minimal resource usage and strong isolation.

---

## **Core Components**

### 1. **Brain**
- **Role**: Central orchestrator that manages workflows, pipelines, and node communication.
- **Responsibilities**:
  - Task scheduling and resource allocation.
  - Node health monitoring and coordination with `Colony` and `Spawn`.
  - Workflow orchestration and pipeline management.

### 2. **Colony**
- **Role**: Task delegator and VM pool manager.
- **Responsibilities**:
  - Spins up/down `Spawn` nodes (Firecracker microVMs).
  - Handles the lifecycle and provisioning of VMs.
  - Communicates with `Brain` for task assignments.

### 3. **Spawn**
- **Role**: Task executor running as a Firecracker microVM.
- **Responsibilities**:
  - Executes tasks in isolation.
  - Streams logs and results back to `Brain` or `Colony`.
  - Cleans up resources after task completion.

### 4. **Hive**
- **Role**: Backend service acting as the bridge between users and the core system.
- **Responsibilities**:
  - Serves REST/gRPC APIs for the frontend and CLI.
  - Handles user authentication, session management, and database operations.
  - Provides access to job data, logs, and artifacts.

### 5. **HiveView**
- **Role**: Frontend for user interaction and visualization.
- **Responsibilities**:
  - Provides an intuitive interface for configuring workflows, pipelines, and jobs.
  - Displays real-time job status and logs.
  - Manages user inputs and parameters for workflows.

---

## **Architecture Overview**
Exzet's architecture is based on a distributed, modular design:

```plaintext
[ HiveView ] <-> [ Hive ] <-> [ Brain ] <-> [ Colony ] <-> [ Spawn ]
```

- **HiveView** communicates with the **Hive** backend to manage user interactions.
- **Hive** acts as the bridge between users and the orchestration logic in **Brain**.
- **Brain** handles workflow orchestration and communicates with **Colony** for task delegation.
- **Colony** manages VM lifecycles and delegates tasks to **Spawn** nodes.
- **Spawn** nodes execute tasks in Firecracker microVMs and return results to **Brain**.

---

## **Installation**

### **Prerequisites**
- **Go** (1.20+)
- **Rust** (latest stable)
- **Node.js** (16+)
- **Docker** (for development environments)
- **Firecracker** (v1.10.1+)

### **Setup**
1. Clone the repository:
   ```bash
   git clone https://github.com/exzet-dev/exzet.git
   cd exzet
   ```

2. Run the setup script to initialize the project structure:
   ```bash
   ./scripts/setup-dev-env.sh
   ```

3. Build each component:
   - **Brain**:
     ```bash
     cd brain && go build -o brain ./cmd
     ```
   - **Colony**:
     ```bash
     cd colony && go build -o colony ./cmd
     ```
   - **Spawn**:
     ```bash
     cd spawn && go build -o spawn ./cmd
     ```
   - **Hive**:
     ```bash
     cd hive && go build -o hive ./cmd
     ```
   - **HiveView**:
     ```bash
     cd hiveview && npm install && npm run build
     ```

---

## **Usage**

### **Starting Services**
1. Start the `Hive` backend:
   ```bash
   ./hive/hive
   ```

2. Start the `Brain` service:
   ```bash
   ./brain/brain
   ```

3. Start the `Colony` service:
   ```bash
   ./colony/colony
   ```

4. Start a `Spawn` node:
   ```bash
   ./spawn/spawn
   ```

5. Start the `HiveView` frontend:
   ```bash
   cd hiveview && npm run dev
   ```

### **Accessing the Web Interface**
- Navigate to `http://localhost:3000` to access the HiveView frontend.

---

## **Development**

### **Adding a Workflow**
1. Define your workflow in the `brain/pkg/workflows/` directory.
2. Expose relevant endpoints via the `Hive` backend API.
3. Add UI components in `hiveview/src/routes/workflows`.

### **Testing**
- Run unit tests:
  ```bash
  go test ./...
  ```
- Run end-to-end tests for HiveView:
  ```bash
  npm run test
  ```

---

## **Contributing**

We welcome contributions! Please see [CONTRIBUTING.md](docs/CONTRIBUTING.md) for guidelines.

---

## **License**

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
