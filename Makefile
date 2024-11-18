# Define directories for each service
BRAIN_DIR := brain/cmd
COLONY_DIR := colony/cmd
SPAWN_DIR := spawn/cmd
HIVE_DIR := hive/cmd
HIVEVIEW_DIR := hiveview

# Define build output directory and names
BIN_DIR := bin
BRAIN_BIN := $(BIN_DIR)/brain
COLONY_BIN := $(BIN_DIR)/colony
SPAWN_BIN := $(BIN_DIR)/spawn
HIVE_BIN := $(BIN_DIR)/hive

# Ensure the bin directory exists
$(BIN_DIR):
	@mkdir -p $(BIN_DIR)

# Default target
.PHONY: all
all: build

# Build all components
.PHONY: build
build: $(BIN_DIR) build-brain build-colony build-spawn build-hive

.PHONY: build-brain
build-brain: $(BIN_DIR)
	@echo "Building Brain Service..."
	cd $(BRAIN_DIR) && go build -o ../../$(BRAIN_BIN)

.PHONY: build-colony
build-colony: $(BIN_DIR)
	@echo "Building Colony Service..."
	cd $(COLONY_DIR) && go build -o ../../$(COLONY_BIN)

.PHONY: build-spawn
build-spawn: $(BIN_DIR)
	@echo "Building Spawn Service..."
	cd $(SPAWN_DIR) && go build -o ../../$(SPAWN_BIN)

.PHONY: build-hive
build-hive: build-hiveview $(BIN_DIR)
	@echo "Building Hive Backend Service..."
	cd $(HIVE_DIR) && go build -o ../../$(HIVE_BIN)

.PHONY: build-hiveview
build-hiveview:
	@echo "Building HiveView Frontend..."
	cd $(HIVEVIEW_DIR) && npm install && npm run build

# Run all components
.PHONY: run
run: run-brain run-colony run-spawn run-hive

.PHONY: run-brain
run-brain: $(BRAIN_BIN)
	@echo "Running Brain Service..."
	./$(BRAIN_BIN)

.PHONY: run-colony
run-colony: $(COLONY_BIN)
	@echo "Running Colony Service..."
	./$(COLONY_BIN)

.PHONY: run-spawn
run-spawn: $(SPAWN_BIN)
	@echo "Running Spawn Service..."
	./$(SPAWN_BIN)

.PHONY: run-hive
run-hive: $(HIVE_BIN)
	@echo "Running Hive Backend Service..."
	./$(HIVE_BIN)

.PHONY: run-hiveview
run-hiveview:
	@echo "Running HiveView Frontend..."
	cd $(HIVEVIEW_DIR) && npm run dev

# Clean build artifacts
.PHONY: clean
clean:
	@echo "Cleaning build artifacts..."
	rm -rf $(BIN_DIR)
	cd $(HIVEVIEW_DIR) && rm -rf node_modules build

# Setup development environment
.PHONY: setup
setup:
	@echo "Setting up development environment..."
	./scripts/setup-dev-env.sh

# Help message
.PHONY: help
help:
	@echo "Available targets:"
	@echo "  build            - Build all components"
	@echo "  build-brain      - Build Brain service"
	@echo "  build-colony     - Build Colony service"
	@echo "  build-spawn      - Build Spawn service"
	@echo "  build-hive       - Build Hive Backend service (depends on build-hiveview)"
	@echo "  build-hiveview   - Build HiveView Frontend"
	@echo "  run              - Run all components"
	@echo "  run-brain        - Run Brain service"
	@echo "  run-colony       - Run Colony service"
	@echo "  run-spawn        - Run Spawn service"
	@echo "  run-hive         - Run Hive Backend service"
	@echo "  run-hiveview     - Run HiveView Frontend"
	@echo "  clean            - Clean all build artifacts"
	@echo "  setup            - Setup development environment"
