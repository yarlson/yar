package deps

import (
	"testing"
)

func TestParseManifest(t *testing.T) {
	input := `
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
http = { git = "https://github.com/user/yar-http.git", tag = "v0.3.1" }
json = { git = "https://github.com/user/yar-json.git", rev = "a1b2c3d" }
logger = { git = "https://github.com/user/yar-logger.git", branch = "main" }
local_lib = { path = "../my-local-lib" }
`
	m, err := ParseManifest(input)
	if err != nil {
		t.Fatalf("ParseManifest: %v", err)
	}
	if m.Package.Name != "myapp" {
		t.Errorf("name = %q, want %q", m.Package.Name, "myapp")
	}
	if m.Package.Version != "0.1.0" {
		t.Errorf("version = %q, want %q", m.Package.Version, "0.1.0")
	}
	if len(m.Dependencies) != 4 {
		t.Fatalf("len(deps) = %d, want 4", len(m.Dependencies))
	}

	http := m.Dependencies["http"]
	if http.Git != "https://github.com/user/yar-http.git" {
		t.Errorf("http.Git = %q", http.Git)
	}
	if http.Tag != "v0.3.1" {
		t.Errorf("http.Tag = %q", http.Tag)
	}
	if http.IsLocal() {
		t.Error("http should not be local")
	}
	if http.VersionRef() != "v0.3.1" {
		t.Errorf("http.VersionRef() = %q", http.VersionRef())
	}

	json := m.Dependencies["json"]
	if json.Rev != "a1b2c3d" {
		t.Errorf("json.Rev = %q", json.Rev)
	}

	logger := m.Dependencies["logger"]
	if logger.Branch != "main" {
		t.Errorf("logger.Branch = %q", logger.Branch)
	}

	local := m.Dependencies["local_lib"]
	if !local.IsLocal() {
		t.Error("local_lib should be local")
	}
	if local.Path != "../my-local-lib" {
		t.Errorf("local_lib.Path = %q", local.Path)
	}
}

func TestParseManifestMissingName(t *testing.T) {
	input := `
[package]
version = "0.1.0"
`
	_, err := ParseManifest(input)
	if err == nil {
		t.Fatal("expected error for missing name")
	}
}

func TestParseManifestInValidAlias(t *testing.T) {
	input := `
[package]
name = "myapp"

[dependencies]
"my-lib" = { git = "https://example.com/repo.git", tag = "v1.0.0" }
`
	_, err := ParseManifest(input)
	if err == nil {
		t.Fatal("expected error for invalid alias")
	}
}

func TestParseManifestConflictingFields(t *testing.T) {
	input := `
[package]
name = "myapp"

[dependencies]
lib = { git = "https://example.com/repo.git", tag = "v1.0.0", rev = "abc123" }
`
	_, err := ParseManifest(input)
	if err == nil {
		t.Fatal("expected error for multiple version specifiers")
	}
}

func TestParseManifestPathWithGit(t *testing.T) {
	input := `
[package]
name = "myapp"

[dependencies]
lib = { path = "../lib", git = "https://example.com/repo.git" }
`
	_, err := ParseManifest(input)
	if err == nil {
		t.Fatal("expected error for path with git")
	}
}

func TestParseManifestNoVersionSpecifier(t *testing.T) {
	input := `
[package]
name = "myapp"

[dependencies]
lib = { git = "https://example.com/repo.git" }
`
	_, err := ParseManifest(input)
	if err == nil {
		t.Fatal("expected error for no version specifier")
	}
}

func TestParseManifestNoDeps(t *testing.T) {
	input := `
[package]
name = "myapp"
`
	m, err := ParseManifest(input)
	if err != nil {
		t.Fatalf("ParseManifest: %v", err)
	}
	if len(m.Dependencies) != 0 {
		t.Errorf("len(deps) = %d, want 0", len(m.Dependencies))
	}
}

func TestValidAlias(t *testing.T) {
	tests := []struct {
		name string
		want bool
	}{
		{"http", true},
		{"my_lib", true},
		{"Lib", true},
		{"lib2", true},
		{"_private", true},
		{"", false},
		{"2lib", false},
		{"my-lib", false},
		{"my.lib", false},
		{"my lib", false},
	}
	for _, tt := range tests {
		got := ValidAlias(tt.name)
		if got != tt.want {
			t.Errorf("ValidAlias(%q) = %v, want %v", tt.name, got, tt.want)
		}
	}
}

func TestWriteManifest(t *testing.T) {
	m := &Manifest{
		Package: PackageInfo{Name: "myapp", Version: "0.1.0"},
		Dependencies: map[string]Dependency{
			"http": {Git: "https://github.com/user/yar-http.git", Tag: "v0.3.1"},
		},
	}
	out, err := WriteManifest(m)
	if err != nil {
		t.Fatalf("WriteManifest: %v", err)
	}
	// Round-trip: parse the output back.
	m2, err := ParseManifest(out)
	if err != nil {
		t.Fatalf("round-trip ParseManifest: %v", err)
	}
	if m2.Package.Name != "myapp" {
		t.Errorf("round-trip name = %q", m2.Package.Name)
	}
	if len(m2.Dependencies) != 1 {
		t.Errorf("round-trip deps = %d", len(m2.Dependencies))
	}
	if m2.Dependencies["http"].Tag != "v0.3.1" {
		t.Errorf("round-trip http.Tag = %q", m2.Dependencies["http"].Tag)
	}
}
