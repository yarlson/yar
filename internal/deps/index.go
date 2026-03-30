package deps

import (
	"path/filepath"
	"strings"
)

// Index maps dependency aliases to their resolved directory paths,
// enabling the compiler's package loader to find dependency packages.
type Index struct {
	entries map[string]string // alias -> directory path
}

// EmptyIndex returns an index with no entries.
func EmptyIndex() *Index {
	return &Index{entries: make(map[string]string)}
}

// NewIndex creates an Index from resolved dependencies.
func NewIndex(resolved []ResolvedDep) *Index {
	entries := make(map[string]string, len(resolved))
	for _, rd := range resolved {
		entries[rd.Name] = rd.Dir
	}
	return &Index{entries: entries}
}

// NewIndexFromLock creates an Index from lock file entries and a cache directory.
func NewIndexFromLock(entries []LockEntry, cacheDir string) *Index {
	m := make(map[string]string, len(entries))
	for _, e := range entries {
		m[e.Name] = CachePath(cacheDir, e.Git, e.Commit)
	}
	return &Index{entries: m}
}

// Resolve maps an import path to a directory path. The first segment of the
// import path is matched against dependency aliases. Sub-packages are resolved
// as subdirectories within the dependency.
//
// Returns the directory path and true if the import matches a dependency,
// or empty string and false otherwise.
func (idx *Index) Resolve(importPath string) (string, bool) {
	if idx == nil {
		return "", false
	}
	alias, subPath, _ := strings.Cut(importPath, "/")
	dir, ok := idx.entries[alias]
	if !ok {
		return "", false
	}
	if subPath != "" {
		dir = filepath.Join(dir, filepath.FromSlash(subPath))
	}
	return dir, true
}

// Has returns true if the alias is a known dependency.
func (idx *Index) Has(alias string) bool {
	if idx == nil {
		return false
	}
	_, ok := idx.entries[alias]
	return ok
}

// Dir returns the root directory for a dependency alias.
func (idx *Index) Dir(alias string) string {
	if idx == nil {
		return ""
	}
	return idx.entries[alias]
}
