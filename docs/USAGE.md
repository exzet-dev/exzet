# **Exzet Usage Guide**

This document provides examples of how to use Exzet for common workflows.

---

## **Setup**

### **1. Starting Services**
Start the core services:
```bash
# Start Hive (backend)
cd hive && ./hive

# Start Brain (orchestrator)
cd brain && ./brain

# Start Colony (VM manager)
cd colony && ./colony

# Start Spawn (task executor)
cd spawn && ./spawn

# Start HiveView (frontend)
cd hiveview && npm run dev
```

---

## **Example Workflow**

### **Scenario: Build a Note-Taking App**

1. **Create a Workflow**:
   - Open HiveView (`http://localhost:3000`) and navigate to the **Workflows** tab.
   - Click **New Workflow** and add steps:
     - Clone the repository.
     - Build the application (e.g., `make build`).
     - Package the application (e.g., `fpm` for RPM files).

2. **Submit a Job**:
   - Navigate to the **Build** tab.
   - Select the **Note-Taking App** workflow.
   - Set parameters like:
     - **Platform**: x86
     - **Git Branch**: main
   - Click **Start Job**.

3. **Monitor Progress**:
   - View real-time logs in the **Jobs** tab.
   - Check for success/failure notifications.

4. **Retrieve Artifacts**:
   - After the job completes, download build artifacts (e.g., `.rpm` files) from the **Artifacts** tab.

---

## **Command Line Interface (CLI)**
Coming soon.
