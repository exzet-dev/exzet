# **Exzet API Documentation**

This document outlines the key API endpoints for the Hive backend.

---

## **Authentication**

### **Login**
- **Endpoint**: `/api/auth/login`
- **Method**: `POST`
- **Request**:
  ```json
  {
    "username": "user",
    "password": "pass"
  }
  ```
- **Response**:
  ```json
  {
    "token": "JWT-TOKEN"
  }
  ```

### **Logout**
- **Endpoint**: `/api/auth/logout`
- **Method**: `POST`
- **Headers**:
  - `Authorization: Bearer <TOKEN>`

---

## **Workflows**

### **Get All Workflows**
- **Endpoint**: `/api/workflows`
- **Method**: `GET`
- **Response**:
  ```json
  [
    {
      "id": "1",
      "name": "Build Note-Taking App",
      "steps": ["Clone repo", "Build app", "Package app"]
    }
  ]
  ```

### **Create a Workflow**
- **Endpoint**: `/api/workflows`
- **Method**: `POST`
- **Request**:
  ```json
  {
    "name": "New Workflow",
    "steps": ["Clone repo", "Build", "Package"]
  }
  ```

---

## **Jobs**

### **Submit Job**
- **Endpoint**: `/api/jobs`
- **Method**: `POST`
- **Request**:
  ```json
  {
    "workflowId": "1",
    "parameters": {
      "platform": "x86",
      "branch": "main"
    }
  }
  ```

### **Get Job Status**
- **Endpoint**: `/api/jobs/{jobId}`
- **Method**: `GET`
- **Response**:
  ```json
  {
    "id": "123",
    "status": "Running",
    "logs": ["Cloning repo...", "Building app..."]
  }
  ```

---

## **Nodes**

### **Get All Nodes**
- **Endpoint**: `/api/nodes`
- **Method**: `GET`
- **Response**:
  ```json
  [
    {
      "id": "node-1",
      "status": "Healthy",
      "resources": {
        "memory": "4GB",
        "cpu": "4 cores"
      }
    }
  ]
  ```
