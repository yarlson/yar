package compiler

import (
	"fmt"
	"os"
	"runtime"
	"strings"
)

// Target describes the platform a build targets.
type Target struct {
	OS     string // "darwin", "linux", "windows"
	Arch   string // "amd64", "arm64"
	Triple string // LLVM target triple derived from OS+Arch
}

type targetKey struct {
	os, arch string
}

var tripleMap = map[targetKey]string{
	{"darwin", "amd64"}:  "x86_64-apple-darwin",
	{"darwin", "arm64"}:  "aarch64-apple-darwin",
	{"linux", "amd64"}:   "x86_64-unknown-linux-gnu",
	{"linux", "arm64"}:   "aarch64-unknown-linux-gnu",
	{"windows", "amd64"}: "x86_64-pc-windows-msvc",
}

// ResolveTarget reads YAR_OS and YAR_ARCH to determine the build target.
// If neither is set, the host platform is used. If only one is set, an error
// is returned.
func ResolveTarget() (Target, error) {
	yarOS := os.Getenv("YAR_OS")
	yarArch := os.Getenv("YAR_ARCH")

	if yarOS == "" && yarArch == "" {
		return hostTarget(), nil
	}
	if yarOS == "" || yarArch == "" {
		return Target{}, fmt.Errorf("set both YAR_OS and YAR_ARCH, or neither")
	}

	triple, ok := tripleMap[targetKey{yarOS, yarArch}]
	if !ok {
		return Target{}, fmt.Errorf("unsupported target %s/%s; supported targets: %s",
			yarOS, yarArch, supportedTargets())
	}
	return Target{OS: yarOS, Arch: yarArch, Triple: triple}, nil
}

// ExeSuffix returns ".exe" for Windows targets, "" otherwise.
func (t Target) ExeSuffix() string {
	if t.OS == "windows" {
		return ".exe"
	}
	return ""
}

// IsCross reports whether t differs from the host platform.
func (t Target) IsCross() bool {
	return t.OS != runtime.GOOS || t.Arch != runtime.GOARCH
}

func hostTarget() Target {
	triple, ok := tripleMap[targetKey{runtime.GOOS, runtime.GOARCH}]
	if !ok {
		// Fallback for hosts not in the map: leave triple empty so clang
		// uses its default (host) target.
		return Target{OS: runtime.GOOS, Arch: runtime.GOARCH}
	}
	return Target{OS: runtime.GOOS, Arch: runtime.GOARCH, Triple: triple}
}

func supportedTargets() string {
	targets := make([]string, 0, len(tripleMap))
	for k := range tripleMap {
		targets = append(targets, k.os+"/"+k.arch)
	}
	// Sort for deterministic output.
	sortTargets(targets)
	return strings.Join(targets, ", ")
}

func sortTargets(s []string) {
	for i := 1; i < len(s); i++ {
		for j := i; j > 0 && s[j] < s[j-1]; j-- {
			s[j], s[j-1] = s[j-1], s[j]
		}
	}
}
