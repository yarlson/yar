package deps

import (
	"path/filepath"
	"runtime"
	"testing"
)

func TestIndexResolve(t *testing.T) {
	base := "/cache"
	if runtime.GOOS == "windows" {
		base = `C:\cache`
	}
	httpDir := filepath.Join(base, "http-abc123")
	jsonDir := filepath.Join(base, "json-def456")

	idx := &Index{
		entries: map[string]string{
			"http": httpDir,
			"json": jsonDir,
		},
	}

	tests := []struct {
		importPath string
		wantDir    string
		wantOK     bool
	}{
		{"http", httpDir, true},
		{"json", jsonDir, true},
		{"http/router", filepath.Join(httpDir, "router"), true},
		{"http/internal/util", filepath.Join(httpDir, "internal", "util"), true},
		{"unknown", "", false},
		{"unknown/sub", "", false},
	}
	for _, tt := range tests {
		dir, ok := idx.Resolve(tt.importPath)
		if ok != tt.wantOK {
			t.Errorf("Resolve(%q) ok = %v, want %v", tt.importPath, ok, tt.wantOK)
		}
		if dir != tt.wantDir {
			t.Errorf("Resolve(%q) dir = %q, want %q", tt.importPath, dir, tt.wantDir)
		}
	}
}

func TestNilIndex(t *testing.T) {
	var idx *Index

	dir, ok := idx.Resolve("http")
	if ok {
		t.Error("nil index should return false")
	}
	if dir != "" {
		t.Errorf("nil index dir = %q", dir)
	}
	if idx.Has("http") {
		t.Error("nil index Has should return false")
	}
	if idx.Dir("http") != "" {
		t.Error("nil index Dir should return empty")
	}
}

func TestIndexHas(t *testing.T) {
	idx := &Index{
		entries: map[string]string{
			"http": "/cache/http",
		},
	}
	if !idx.Has("http") {
		t.Error("Has(http) = false")
	}
	if idx.Has("json") {
		t.Error("Has(json) = true")
	}
}

func TestNewIndex(t *testing.T) {
	resolved := []ResolvedDep{
		{Name: "http", Dir: "/cache/http-abc"},
		{Name: "json", Dir: "/cache/json-def"},
	}
	idx := NewIndex(resolved)
	if !idx.Has("http") {
		t.Error("missing http")
	}
	if !idx.Has("json") {
		t.Error("missing json")
	}
	if idx.Dir("http") != "/cache/http-abc" {
		t.Errorf("http dir = %q", idx.Dir("http"))
	}
}

func TestNewIndexFromLock(t *testing.T) {
	entries := []LockEntry{
		{Name: "http", Git: "https://github.com/user/http.git", Commit: "abc123", Hash: "sha256:1"},
	}
	idx := NewIndexFromLock(entries, "/cache")
	if !idx.Has("http") {
		t.Error("missing http")
	}
	dir := idx.Dir("http")
	expected := CachePath("/cache", "https://github.com/user/http.git", "abc123")
	if dir != expected {
		t.Errorf("dir = %q, want %q", dir, expected)
	}
}
