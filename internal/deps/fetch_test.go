package deps

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestHashDir(t *testing.T) {
	dir := t.TempDir()

	// Create a simple directory structure.
	if err := os.WriteFile(filepath.Join(dir, "a.txt"), []byte("hello"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(dir, "sub"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "sub", "b.txt"), []byte("world"), 0o600); err != nil {
		t.Fatal(err)
	}

	hash1, err := HashDir(dir)
	if err != nil {
		t.Fatalf("HashDir: %v", err)
	}
	if !strings.HasPrefix(hash1, "sha256:") {
		t.Errorf("hash does not start with sha256: prefix: %q", hash1)
	}

	// Same content should produce the same hash.
	hash2, err := HashDir(dir)
	if err != nil {
		t.Fatalf("HashDir: %v", err)
	}
	if hash1 != hash2 {
		t.Errorf("deterministic hash failed: %q != %q", hash1, hash2)
	}

	// Different content should produce a different hash.
	if err := os.WriteFile(filepath.Join(dir, "a.txt"), []byte("changed"), 0o600); err != nil {
		t.Fatal(err)
	}
	hash3, err := HashDir(dir)
	if err != nil {
		t.Fatalf("HashDir: %v", err)
	}
	if hash1 == hash3 {
		t.Error("hash should differ after content change")
	}
}

func TestVerifyHash(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "a.txt"), []byte("hello"), 0o600); err != nil {
		t.Fatal(err)
	}

	hash, err := HashDir(dir)
	if err != nil {
		t.Fatal(err)
	}

	// Correct hash should pass.
	if err := VerifyHash(dir, hash); err != nil {
		t.Errorf("VerifyHash with correct hash: %v", err)
	}

	// Wrong hash should fail.
	if err := VerifyHash(dir, "sha256:0000"); err == nil {
		t.Error("VerifyHash should fail with wrong hash")
	}
}

func TestCachePath(t *testing.T) {
	dir := CachePath("/cache", "https://github.com/user/repo.git", "abc123")
	if dir == "" {
		t.Error("CachePath returned empty")
	}
	// Should contain the cache root, a URL hash segment, and the commit SHA.
	parts := strings.Split(dir, string(filepath.Separator))
	if parts[len(parts)-1] != "abc123" {
		t.Errorf("CachePath = %q, last segment should be abc123", dir)
	}

	// Same inputs should produce same path.
	dir2 := CachePath("/cache", "https://github.com/user/repo.git", "abc123")
	if dir != dir2 {
		t.Error("CachePath not deterministic")
	}

	// Different URL should produce different path.
	dir3 := CachePath("/cache", "https://github.com/other/repo.git", "abc123")
	if dir == dir3 {
		t.Error("different URLs should produce different paths")
	}
}

func TestIsCached(t *testing.T) {
	cacheDir := t.TempDir()

	// Not cached initially.
	if IsCached(cacheDir, "https://example.com/repo.git", "abc123") {
		t.Error("should not be cached initially")
	}

	// Create the cache entry.
	path := CachePath(cacheDir, "https://example.com/repo.git", "abc123")
	if err := os.MkdirAll(path, 0o755); err != nil {
		t.Fatal(err)
	}

	// Now it should be cached.
	if !IsCached(cacheDir, "https://example.com/repo.git", "abc123") {
		t.Error("should be cached after creating directory")
	}
}
