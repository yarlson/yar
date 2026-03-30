package deps

import (
	"context"
	"fmt"
	"path/filepath"
	"strings"
)

// ResolvedDep represents a fully resolved dependency ready for the lock file.
type ResolvedDep struct {
	Name   string
	Dep    Dependency
	Commit string
	Hash   string
	Dir    string
}

// Resolve walks the dependency graph starting from the root manifest,
// resolves all transitive dependencies, detects conflicts, and returns
// a flat list of resolved dependencies.
func Resolve(ctx context.Context, rootDir string, manifest *Manifest, cacheDir string) ([]ResolvedDep, error) {
	r := &resolver{
		ctx:      ctx,
		cacheDir: cacheDir,
		resolved: make(map[string]*ResolvedDep),
		visiting: make(map[string]bool),
	}

	for alias, dep := range manifest.Dependencies {
		if err := r.resolve(alias, dep, rootDir, nil); err != nil {
			return nil, err
		}
	}

	result := make([]ResolvedDep, 0, len(r.resolved))
	for _, rd := range r.resolved {
		result = append(result, *rd)
	}
	return result, nil
}

type resolver struct {
	ctx      context.Context
	cacheDir string
	resolved map[string]*ResolvedDep
	visiting map[string]bool
	lockIdx  map[string]*LockEntry
}

func (r *resolver) resolve(alias string, dep Dependency, parentDir string, chain []string) error {
	// Local path dependencies resolve immediately.
	if dep.IsLocal() {
		dir := dep.Path
		if !filepath.IsAbs(dir) {
			dir = filepath.Join(parentDir, dep.Path)
		}
		dir = filepath.Clean(dir)

		if existing, ok := r.resolved[alias]; ok {
			if existing.Dir != dir {
				return fmt.Errorf("dependency conflict for %q: %s vs %s", alias, existing.Dir, dir)
			}
			return nil
		}
		r.resolved[alias] = &ResolvedDep{
			Name: alias,
			Dep:  dep,
			Dir:  dir,
		}
		return r.resolveTransitive(alias, dir, chain)
	}

	// Check for dependency cycles.
	if r.visiting[alias] {
		return fmt.Errorf("dependency cycle detected: %s -> %s", formatChain(chain), alias)
	}

	// Check if already resolved.
	if existing, ok := r.resolved[alias]; ok {
		if existing.Dep.Git != dep.Git || existing.Dep.VersionRef() != dep.VersionRef() {
			return fmt.Errorf(
				"dependency conflict for %q: %s@%s (from %s) vs %s@%s",
				alias, existing.Dep.Git, existing.Dep.VersionRef(),
				formatChain(chain), dep.Git, dep.VersionRef(),
			)
		}
		return nil
	}

	r.visiting[alias] = true
	defer delete(r.visiting, alias)

	// Try lock file first.
	if r.lockIdx != nil {
		if entry, ok := r.lockIdx[alias]; ok {
			if entry.Git == dep.Git && matchesRef(entry, dep) {
				dir := CachePath(r.cacheDir, dep.Git, entry.Commit)
				if IsCached(r.cacheDir, dep.Git, entry.Commit) {
					r.resolved[alias] = &ResolvedDep{
						Name:   alias,
						Dep:    dep,
						Commit: entry.Commit,
						Hash:   entry.Hash,
						Dir:    dir,
					}
					return r.resolveTransitive(alias, dir, chain)
				}
				// Cached entry missing, need to fetch.
				fetchedDir, err := Fetch(r.ctx, r.cacheDir, dep, entry.Commit)
				if err != nil {
					return fmt.Errorf("fetching %s: %w", alias, err)
				}
				if err := VerifyHash(fetchedDir, entry.Hash); err != nil {
					return fmt.Errorf("verifying %s: %w", alias, err)
				}
				r.resolved[alias] = &ResolvedDep{
					Name:   alias,
					Dep:    dep,
					Commit: entry.Commit,
					Hash:   entry.Hash,
					Dir:    fetchedDir,
				}
				return r.resolveTransitive(alias, fetchedDir, chain)
			}
		}
	}

	// Fresh resolve: fetch and determine commit SHA.
	commit, hash, err := FetchAndResolveCommit(r.ctx, r.cacheDir, dep)
	if err != nil {
		return fmt.Errorf("resolving %s: %w", alias, err)
	}

	dir := CachePath(r.cacheDir, dep.Git, commit)
	r.resolved[alias] = &ResolvedDep{
		Name:   alias,
		Dep:    dep,
		Commit: commit,
		Hash:   hash,
		Dir:    dir,
	}
	return r.resolveTransitive(alias, dir, chain)
}

func (r *resolver) resolveTransitive(alias, dir string, chain []string) error {
	manifestPath := filepath.Join(dir, ManifestFile)
	m, err := ReadManifest(manifestPath)
	if err != nil {
		// No yar.toml means no transitive deps — that's fine.
		return nil //nolint:nilerr // missing manifest is expected for leaf deps
	}

	newChain := make([]string, len(chain)+1)
	copy(newChain, chain)
	newChain[len(chain)] = alias

	for transAlias, transDep := range m.Dependencies {
		if err := r.resolve(transAlias, transDep, dir, newChain); err != nil {
			return err
		}
	}
	return nil
}

func matchesRef(entry *LockEntry, dep Dependency) bool {
	if dep.Tag != "" {
		return entry.Tag == dep.Tag
	}
	if dep.Rev != "" {
		return entry.Rev == dep.Rev || entry.Commit == dep.Rev ||
			(len(dep.Rev) < len(entry.Commit) && entry.Commit[:len(dep.Rev)] == dep.Rev)
	}
	return entry.Branch == dep.Branch
}

func formatChain(chain []string) string {
	if len(chain) == 0 {
		return "root"
	}
	parts := make([]string, 0, len(chain)+1)
	parts = append(parts, "root")
	parts = append(parts, chain...)
	return strings.Join(parts, " -> ")
}

// ToLockEntries converts resolved dependencies to lock file entries.
// Local (path) dependencies are excluded from the lock file.
func ToLockEntries(resolved []ResolvedDep) []LockEntry {
	var entries []LockEntry
	for _, rd := range resolved {
		if rd.Dep.IsLocal() {
			continue
		}
		entry := LockEntry{
			Name:   rd.Name,
			Git:    rd.Dep.Git,
			Commit: rd.Commit,
			Hash:   rd.Hash,
		}
		if rd.Dep.Tag != "" {
			entry.Tag = rd.Dep.Tag
		}
		if rd.Dep.Rev != "" {
			entry.Rev = rd.Dep.Rev
		}
		if rd.Dep.Branch != "" {
			entry.Branch = rd.Dep.Branch
		}
		entries = append(entries, entry)
	}
	return entries
}
