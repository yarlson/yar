package stdlib

import (
	"embed"
	"io/fs"
	"path"
	"strings"
)

//go:embed all:packages
var packages embed.FS

// Has reports whether importPath names a stdlib package.
func Has(importPath string) bool {
	dir := path.Join("packages", importPath)
	entries, err := fs.ReadDir(packages, dir)
	if err != nil {
		return false
	}
	for _, entry := range entries {
		if !entry.IsDir() && strings.HasSuffix(entry.Name(), ".yar") {
			return true
		}
	}
	return false
}

// ReadDir returns the .yar file names in a stdlib package.
func ReadDir(importPath string) ([]string, error) {
	dir := path.Join("packages", importPath)
	entries, err := fs.ReadDir(packages, dir)
	if err != nil {
		return nil, err
	}
	var names []string
	for _, entry := range entries {
		if !entry.IsDir() && strings.HasSuffix(entry.Name(), ".yar") {
			names = append(names, entry.Name())
		}
	}
	return names, nil
}

// ReadFile returns the source content of a .yar file in a stdlib package.
func ReadFile(importPath, fileName string) (string, error) {
	p := path.Join("packages", importPath, fileName)
	data, err := fs.ReadFile(packages, p)
	if err != nil {
		return "", err
	}
	return string(data), nil
}
