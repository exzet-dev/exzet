package resources

import (
	"embed"
	"io/fs"
	"path"
)

//go:embed bin/* fc.stamp firecracker jailer rootfs.ext4 rootfs.id_rsa vmlinux
var resources embed.FS

// GetResource returns the content of a specific resource file.
func GetResource(resourceName string) ([]byte, error) {

	data, err := resources.ReadFile(resourceName)
	if err != nil {
		return nil, err
	}
	return data, nil
}

// ListResources lists all files in the embedded resources folder.
func ListResources() ([]string, error) {
	var files []string
	err := fs.WalkDir(resources, ".", func(filePath string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if !d.IsDir() {
			files = append(files, path.Join(filePath))
		}
		return nil
	})
	return files, err
}
