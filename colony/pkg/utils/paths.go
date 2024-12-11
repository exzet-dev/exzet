package utils

import (
	"bytes"
	"exzet-colony/pkg/resources"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

const (
	ExternalResourcesDir = "build/output"
)

// GetResourcePath retrieves the path to an embedded resource.
func GetResourcePath(resourceName string) string {
	cwd, err := os.Getwd()
	if err != nil {
		fmt.Printf("Failed to get cwd: %v\n", err)
		return ""
	}
	fp := filepath.Join(cwd, "resources", resourceName)
	return fp
}

// WriteAllEmbeddedResources extracts all embedded resources and writes them to their respective paths.
func WriteAllEmbeddedResources(baseDir string) error {
	list, err := resources.ListResources()
	if err != nil {
		return fmt.Errorf("failed to retrieve resource list: %w", err)
	}

	for _, resourceName := range list {
		outputPath := filepath.Join(baseDir, resourceName)
		err := WriteEmbeddedResource(resourceName, outputPath)
		if err != nil {
			return fmt.Errorf("failed to extract resource %s: %w", resourceName, err)
		}
	}

	return nil
}

// WriteEmbeddedResource writes a single embedded resource to a file.
func WriteEmbeddedResource(resourceName, outputPath string) error {
	data, err := resources.GetResource(resourceName)
	if err != nil {
		return fmt.Errorf("failed to retrieve resource %s: %w", resourceName, err)
	}

	// Ensure parent directories exist
	parentDir := filepath.Dir(outputPath)
	err = os.MkdirAll(parentDir, 0755)
	if err != nil {
		return fmt.Errorf("failed to create directories for %s: %w", resourceName, err)
	}

	// Write the resource to the specified file path
	err = os.WriteFile(outputPath, data, 0644)
	if err != nil {
		return fmt.Errorf("failed to write resource %s to file: %w", resourceName, err)
	}

	err = os.Chmod(outputPath, 0755) // Add execute permissions
	if err != nil {
		return fmt.Errorf("failed to set execute permissions for %s: %w", resourceName, err)
	}

	return nil
}

func CopySystemFiles(baseDir string) error {
	cwd, _ := os.Getwd()
	sourceDir := filepath.Join(cwd, ExternalResourcesDir)

	// FIRST CHECK IF DIRECTORY EXISTS
	if err := os.MkdirAll(baseDir, 0755); err != nil {
		return fmt.Errorf("failed to create destination directory: %w", err)
	}

	entries, err := os.ReadDir(sourceDir)
	if err != nil {
		return fmt.Errorf("failed to read source directory: %w", err)
	}

	for _, entry := range entries {
		sourcePath := filepath.Join(sourceDir, entry.Name())
		destPath := filepath.Join(baseDir, entry.Name())

		// CHECK IF FILE ALREADY EXISTS
		if _, err := os.Stat(destPath); err == nil {
			// FILE EXISTS, COMPARE CONTENTS
			sourceData, err := os.ReadFile(sourcePath)
			if err != nil {
				return fmt.Errorf("failed to read source %s: %w", entry.Name(), err)
			}

			destData, err := os.ReadFile(destPath)
			if err != nil {
				return fmt.Errorf("failed to read destination %s: %w", entry.Name(), err)
			}

			if bytes.Equal(sourceData, destData) {
				continue // Files are identical, skip
			}

			// Files differ, backup existing
			backupPath := destPath + ".bak"
			if err := os.Rename(destPath, backupPath); err != nil {
				return fmt.Errorf("failed to backup %s: %w", entry.Name(), err)
			}
		}

		// COPY FILE
		data, err := os.ReadFile(sourcePath)
		if err != nil {
			return fmt.Errorf("failed to read %s: %w", entry.Name(), err)
		}

		// PRESERVE ORIGINAL FILE MODE IF IT EXISTS
		fileMode := os.FileMode(0644)
		if info, err := os.Stat(sourcePath); err == nil {
			fileMode = info.Mode()
		}

		if err := os.WriteFile(destPath, data, fileMode); err != nil {
			return fmt.Errorf("failed to write %s: %w", entry.Name(), err)
		}
	}

	return nil
}

func CleanupBackups(baseDir string) error {
	entries, err := os.ReadDir(baseDir)
	if err != nil {
		return fmt.Errorf("failed to read directory: %w", err)
	}

	for _, entry := range entries {
		if strings.HasSuffix(entry.Name(), ".bak") {
			path := filepath.Join(baseDir, entry.Name())
			if err := os.Remove(path); err != nil {
				return fmt.Errorf("failed to remove backup %s: %w", entry.Name(), err)
			}
		}
	}

	return nil
}
