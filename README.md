# Exzet (Development Branch)

Exzet is a modern, high-performance orchestration framework designed for executing isolated workflows, pipelines, and jobs using lightweight virtualization technologies. This document outlines the project's architecture, tech stack, and development setup.

---

## Table of Contents

1. [Overview](#overview)
2. [Tech Stack](#tech-stack)
3. [System Architecture](#system-architecture)
4. [Execution Flow](#execution-flow)
5. [Development Setup](#development-setup)

---

## Overview

Exzet enables seamless execution of tasks across distributed nodes using microVMs, ensuring security, scalability, and performance. Its primary use cases include:
- Orchestrating isolated workflows.
- Running dynamic job pipelines.
- Managing build artifacts and task outputs.

---

## Tech Stack

### Frontend
- **Framework**: [Svelte (with Runes)](https://svelte.dev/)
- **Styling**: [TailwindCSS](https://tailwindcss.com/)
- **Dynamic Forms**: [JSONForms](https://jsonforms.io/)

### Backend
- **Language**: [Go](https://golang.org/)
- **Web Framework**: [Fiber](https://gofiber.io/) or [Gin](https://gin-gonic.com/)
- **Virtualization**: [Firecracker](https://firecracker-microvm.github.io/) (via the [Firecracker Go SDK](https://github.com/firecracker-microvm/firecracker-go-sdk))
- **Messaging**: [NATS](https://nats.io/) or [gRPC](https://grpc.io/)

### Database
- **Primary**: [PostgreSQL](https://www.postgresql.org/)
- **Alternative**: [SQLite](https://www.sqlite.org/) (for local development)

### Storage
- **Options**:
  - [NFS](https://en.wikipedia.org/wiki/Network_File_System) for shared storage.
  - [MinIO](https://min.io/) or AWS S3 for scalable object storage.

### Provisioning
- **Tool**: [Packer](https://www.packer.io/) (for templating and VM preparation).

---

## System Architecture

### High-Level Components

| **Component**  | **Technology**                | **Description**                                   |
|-----------------|-------------------------------|---------------------------------------------------|
| **Frontend**    | Svelte + TailwindCSS          | User interface for managing workflows and jobs.  |
| **API Server**  | Go (Fiber or Gin)             | Central brain for job orchestration and node management. |
| **Database**    | PostgreSQL                    | Persistent metadata and job history storage.     |
| **Messaging**   | NATS or gRPC                  | Efficient communication between nodes and the brain. |
| **Task Execution** | Firecracker                 | MicroVMs for isolated and secure task execution. |
| **Storage**     | NFS or MinIO                 | Shared storage for artifacts and task outputs.   |

### Communication and Execution
- **Brain to Nodes**: Efficient task distribution via gRPC or NATS.
- **Artifact Management**: Nodes store build artifacts in shared storage (e.g., NFS or S3-compatible).
- **Provisioning**: Nodes dynamically provision environments via Firecracker snapshots or templates.

---

## Execution Flow

1. **User Interaction**:
   - User navigates the web app served by the Brain server.
   - Jobs, workflows, and pipelines are configured through dynamic forms.

2. **Job Submission**:
   - The user submits a job with parameters (e.g., "Platform = x86", "Git Branch = main").

3. **Node Selection**:
   - The Brain evaluates nodes for available resources (e.g., memory, disk, network).
   - Selects the most suitable node(s) for the job.

4. **Task Execution**:
   - Nodes spin up Firecracker microVMs for task isolation.
   - Tasks are executed inside microVMs with real-time log streaming back to the Brain.

5. **Artifact Handling**:
   - Build artifacts are uploaded to shared storage (NFS or S3).
   - The Brain archives or serves artifacts as needed.

6. **Cleanup**:
   - MicroVMs are terminated, and temporary resources are cleaned up.

---

## Development Setup

### Prerequisites
1. **Backend**:
   - Install [Go](https://golang.org/dl/).
   - Install Firecracker binary: [Firecracker Releases](https://github.com/firecracker-microvm/firecracker/releases).
   - Install [NATS](https://docs.nats.io/running-a-nats-service/nats-server/installation) (optional).
2. **Frontend**:
   - Install [Node.js](https://nodejs.org/).
   - Install [Vite](https://vitejs.dev/).

### Setting Up the Project
1. Clone the repository:
   ```bash
   git clone https://github.com/exzet-dev/exzet.git
   cd exzet
   ```

2. Install dependencies:
   - **Backend**:
     ```bash
     go mod tidy
     ```
   - **Frontend**:
     ```bash
     cd frontend
     npm install
     ```

3. Start the application:
   - **Backend**:
     ```bash
     go run main.go
     ```
   - **Frontend**:
     ```bash
     npm run dev
     ```

4. Run Firecracker:
   ```bash
   firecracker --config-file config.json
   ```

---

## Contribution Guidelines

- **Branching Strategy**: 
  - Development occurs on the `dev` branch.
  - Features and fixes are merged into `main` after code review.

- **Code Standards**:
  - Follow Go [Effective Go](https://go.dev/doc/effective_go) guidelines for backend.
  - Use [Prettier](https://prettier.io/) for frontend formatting.

- **Issue Tracking**:
  - Report bugs and suggest features via GitHub Issues.

---

## License

This project is licensed under the MIT License. See the [LICENSE](./LICENSE) file for details.

---

## Contact

For questions or support, contact the Exzet development team:
- **GitHub**: [exzet-dev](https://github.com/exzet-dev)
