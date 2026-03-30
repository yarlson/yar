package deps

import (
	"fmt"
	"os"
	"strings"

	"github.com/BurntSushi/toml"
)

const ManifestFile = "yar.toml"

// Manifest represents the parsed yar.toml file.
type Manifest struct {
	Package      PackageInfo
	Dependencies map[string]Dependency
}

// PackageInfo holds the [package] section of yar.toml.
type PackageInfo struct {
	Name    string `toml:"name"`
	Version string `toml:"version,omitempty"`
}

// Dependency represents a single dependency declaration.
// Exactly one of Git or Path must be set.
type Dependency struct {
	Git    string `toml:"git,omitempty"`
	Tag    string `toml:"tag,omitempty"`
	Rev    string `toml:"rev,omitempty"`
	Branch string `toml:"branch,omitempty"`
	Path   string `toml:"path,omitempty"`
}

// IsLocal returns true if this is a path-based local dependency.
func (d Dependency) IsLocal() bool {
	return d.Path != ""
}

// VersionRef returns the version specifier (tag, rev, or branch).
func (d Dependency) VersionRef() string {
	if d.Tag != "" {
		return d.Tag
	}
	if d.Rev != "" {
		return d.Rev
	}
	return d.Branch
}

// manifestFile is the raw TOML structure for parsing.
type manifestFile struct {
	Package      PackageInfo           `toml:"package"`
	Dependencies map[string]Dependency `toml:"dependencies"`
}

// ReadManifest parses a yar.toml file at the given path.
func ReadManifest(path string) (*Manifest, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("reading %s: %w", path, err)
	}
	return ParseManifest(string(data))
}

// ParseManifest parses yar.toml content from a string.
func ParseManifest(content string) (*Manifest, error) {
	var raw manifestFile
	if _, err := toml.Decode(content, &raw); err != nil {
		return nil, fmt.Errorf("parsing yar.toml: %w", err)
	}
	if raw.Package.Name == "" {
		return nil, fmt.Errorf("yar.toml: [package] name is required")
	}
	if !ValidAlias(raw.Package.Name) {
		return nil, fmt.Errorf("yar.toml: invalid package name %q", raw.Package.Name)
	}
	if raw.Dependencies == nil {
		raw.Dependencies = make(map[string]Dependency)
	}
	for alias, dep := range raw.Dependencies {
		if !ValidAlias(alias) {
			return nil, fmt.Errorf("yar.toml: invalid dependency alias %q", alias)
		}
		if err := validateDependency(alias, dep); err != nil {
			return nil, err
		}
	}
	return &Manifest{
		Package:      raw.Package,
		Dependencies: raw.Dependencies,
	}, nil
}

// WriteManifest serializes a Manifest to yar.toml format.
func WriteManifest(m *Manifest) (string, error) {
	raw := manifestFile{
		Package:      m.Package,
		Dependencies: m.Dependencies,
	}
	var b strings.Builder
	enc := toml.NewEncoder(&b)
	if err := enc.Encode(raw); err != nil {
		return "", fmt.Errorf("encoding yar.toml: %w", err)
	}
	return b.String(), nil
}

func validateDependency(alias string, dep Dependency) error {
	if dep.Path != "" {
		if dep.Git != "" || dep.Tag != "" || dep.Rev != "" || dep.Branch != "" {
			return fmt.Errorf("yar.toml: dependency %q has both path and git fields", alias)
		}
		return nil
	}
	if dep.Git == "" {
		return fmt.Errorf("yar.toml: dependency %q must specify git or path", alias)
	}
	if !validGitURL(dep.Git) {
		return fmt.Errorf("yar.toml: dependency %q has unsupported git URL scheme %q", alias, dep.Git)
	}
	count := 0
	if dep.Tag != "" {
		count++
	}
	if dep.Rev != "" {
		count++
	}
	if dep.Branch != "" {
		count++
	}
	if count != 1 {
		return fmt.Errorf("yar.toml: dependency %q must specify exactly one of tag, rev, or branch", alias)
	}
	return nil
}

// ValidAlias checks that a name is a valid Yar import path segment:
// [a-zA-Z_][a-zA-Z0-9_]*.
func ValidAlias(name string) bool {
	if name == "" {
		return false
	}
	for i, r := range name {
		if i == 0 {
			if r != '_' && (r < 'A' || r > 'Z') && (r < 'a' || r > 'z') {
				return false
			}
			continue
		}
		if r != '_' && (r < 'A' || r > 'Z') && (r < 'a' || r > 'z') && (r < '0' || r > '9') {
			return false
		}
	}
	return true
}
