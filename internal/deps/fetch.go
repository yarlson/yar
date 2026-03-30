package deps

import (
	"context"
	"crypto/sha256"
	"fmt"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

// CacheDir returns the root cache directory for fetched dependencies.
// It respects YAR_CACHE, falling back to os.UserCacheDir()/yar/deps.
func CacheDir() (string, error) {
	if env := os.Getenv("YAR_CACHE"); env != "" {
		return env, nil
	}
	base, err := os.UserCacheDir()
	if err != nil {
		return "", fmt.Errorf("determining cache directory: %w", err)
	}
	return filepath.Join(base, "yar", "deps"), nil
}

// CachePath returns the directory path where a specific dependency version
// is cached: {cacheDir}/{urlHash}/{commitSHA}/.
func CachePath(cacheDir, gitURL, commitSHA string) string {
	urlHash := fmt.Sprintf("%x", sha256.Sum256([]byte(gitURL)))[:16]
	return filepath.Join(cacheDir, urlHash, commitSHA)
}

// IsCached returns true if the dependency is already in the cache.
func IsCached(cacheDir, gitURL, commitSHA string) bool {
	dir := CachePath(cacheDir, gitURL, commitSHA)
	info, err := os.Stat(dir)
	return err == nil && info.IsDir()
}

// Fetch clones a git repository at the specified ref into the cache.
// For tag and branch refs, it uses shallow clone. For rev, it does a full clone.
func Fetch(ctx context.Context, cacheDir string, dep Dependency, commitSHA string) (string, error) {
	destDir := CachePath(cacheDir, dep.Git, commitSHA)
	if IsCached(cacheDir, dep.Git, commitSHA) {
		return destDir, nil
	}

	tmpDir, err := ensureTempDir(cacheDir)
	if err != nil {
		return "", err
	}
	defer os.RemoveAll(tmpDir)

	cloneDir := filepath.Join(tmpDir, "repo")
	if err := gitClone(ctx, dep, cloneDir); err != nil {
		return "", err
	}

	// Verify the commit SHA matches what we expect.
	actual, err := gitRevParse(ctx, cloneDir)
	if err != nil {
		return "", err
	}
	if commitSHA != "" && actual != commitSHA {
		return "", fmt.Errorf("commit mismatch for %s: expected %s, got %s", dep.Git, commitSHA, actual)
	}

	// Remove .git directory before moving to cache.
	if err := os.RemoveAll(filepath.Join(cloneDir, ".git")); err != nil {
		return "", fmt.Errorf("removing .git: %w", err)
	}

	// Move to final cache location.
	if err := os.MkdirAll(filepath.Dir(destDir), 0o755); err != nil {
		return "", fmt.Errorf("creating cache path: %w", err)
	}
	if err := os.Rename(cloneDir, destDir); err != nil {
		return "", fmt.Errorf("moving to cache: %w", err)
	}
	return destDir, nil
}

// FetchAndResolveCommit clones a repository to determine the commit SHA
// for a tag/branch ref. Returns (commitSHA, contentHash, error).
func FetchAndResolveCommit(ctx context.Context, cacheDir string, dep Dependency) (commitSHA, contentHash string, err error) {
	tmpDir, err := ensureTempDir(cacheDir)
	if err != nil {
		return "", "", err
	}
	defer os.RemoveAll(tmpDir)

	cloneDir := filepath.Join(tmpDir, "repo")
	if err := gitClone(ctx, dep, cloneDir); err != nil {
		return "", "", err
	}

	commitSHA, err = gitRevParse(ctx, cloneDir)
	if err != nil {
		return "", "", err
	}

	// Remove .git and move to cache.
	if err := os.RemoveAll(filepath.Join(cloneDir, ".git")); err != nil {
		return "", "", fmt.Errorf("removing .git: %w", err)
	}

	destDir := CachePath(cacheDir, dep.Git, commitSHA)
	if !IsCached(cacheDir, dep.Git, commitSHA) {
		if err := os.MkdirAll(filepath.Dir(destDir), 0o755); err != nil {
			return "", "", fmt.Errorf("creating cache path: %w", err)
		}
		if err := os.Rename(cloneDir, destDir); err != nil {
			return "", "", fmt.Errorf("moving to cache: %w", err)
		}
	}

	contentHash, err = HashDir(destDir)
	if err != nil {
		return "", "", err
	}
	return commitSHA, contentHash, nil
}

func ensureTempDir(cacheDir string) (string, error) {
	tmpDir, err := os.MkdirTemp(cacheDir, "fetch-*")
	if err != nil {
		if mkErr := os.MkdirAll(cacheDir, 0o755); mkErr != nil {
			return "", fmt.Errorf("creating cache directory: %w", mkErr)
		}
		tmpDir, err = os.MkdirTemp(cacheDir, "fetch-*")
		if err != nil {
			return "", fmt.Errorf("creating temp directory: %w", err)
		}
	}
	return tmpDir, nil
}

// validGitURL checks that a git URL uses a safe transport scheme.
func validGitURL(u string) bool {
	for _, prefix := range []string{"https://", "http://", "ssh://", "git://", "git@"} {
		if strings.HasPrefix(u, prefix) {
			return true
		}
	}
	return false
}

func gitClone(ctx context.Context, dep Dependency, dest string) error {
	if !validGitURL(dep.Git) {
		return fmt.Errorf("unsupported git URL scheme: %s", dep.Git)
	}
	var args []string
	switch {
	case dep.Tag != "":
		args = []string{"clone", "--depth=1", "--branch", dep.Tag, dep.Git, dest}
	case dep.Branch != "":
		args = []string{"clone", "--depth=1", "--branch", dep.Branch, dep.Git, dest}
	default:
		// For rev pins, we need a full clone then checkout.
		args = []string{"clone", dep.Git, dest}
	}

	cmd := exec.CommandContext(ctx, "git", args...)
	cmd.Stderr = os.Stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("git clone %s: %w", dep.Git, err)
	}

	if dep.Rev != "" {
		checkout := exec.CommandContext(ctx, "git", "-C", dest, "checkout", dep.Rev)
		checkout.Stderr = os.Stderr
		if err := checkout.Run(); err != nil {
			return fmt.Errorf("git checkout %s: %w", dep.Rev, err)
		}
	}
	return nil
}

func gitRevParse(ctx context.Context, dir string) (string, error) {
	cmd := exec.CommandContext(ctx, "git", "-C", dir, "rev-parse", "HEAD")
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("git rev-parse HEAD in %s: %w", dir, err)
	}
	return strings.TrimSpace(string(out)), nil
}

// HashDir computes a deterministic SHA-256 hash of a directory tree.
// Files are sorted lexicographically by relative path. Each file contributes
// its relative path and content to the hash.
func HashDir(dir string) (string, error) {
	type entry struct {
		rel     string
		content []byte
	}
	var entries []entry

	err := filepath.WalkDir(dir, func(path string, d fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if d.IsDir() {
			return nil
		}
		rel, relErr := filepath.Rel(dir, path)
		if relErr != nil {
			return relErr
		}
		rel = filepath.ToSlash(rel)
		content, readErr := os.ReadFile(path) //nolint:gosec // walking a trusted cache directory
		if readErr != nil {
			return readErr
		}
		entries = append(entries, entry{rel: rel, content: content})
		return nil
	})
	if err != nil {
		return "", fmt.Errorf("hashing directory %s: %w", dir, err)
	}

	sort.Slice(entries, func(i, j int) bool {
		return entries[i].rel < entries[j].rel
	})

	h := sha256.New()
	for _, e := range entries {
		fmt.Fprintf(h, "file %s %d\n", e.rel, len(e.content))
		h.Write(e.content)
	}
	return fmt.Sprintf("sha256:%x", h.Sum(nil)), nil
}

// VerifyHash checks that the cached directory matches the expected hash.
func VerifyHash(dir, expected string) error {
	if !strings.HasPrefix(expected, "sha256:") {
		return fmt.Errorf("invalid hash format for %s: expected sha256: prefix, got %q", dir, expected)
	}
	actual, err := HashDir(dir)
	if err != nil {
		return err
	}
	if actual != expected {
		return fmt.Errorf("hash mismatch for %s: expected %s, got %s", dir, expected, actual)
	}
	return nil
}
