package utils

import (
	"exzet-colony/pkg/resources"
	"fmt"
	"os"
	"path/filepath"
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
