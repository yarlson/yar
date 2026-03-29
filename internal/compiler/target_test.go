package compiler

import (
	"runtime"
	"testing"
)

func TestResolveTargetDefaultsToHost(t *testing.T) {
	t.Setenv("YAR_OS", "")
	t.Setenv("YAR_ARCH", "")

	target, err := ResolveTarget()
	if err != nil {
		t.Fatal(err)
	}
	if target.OS != runtime.GOOS {
		t.Errorf("OS = %q, want %q", target.OS, runtime.GOOS)
	}
	if target.Arch != runtime.GOARCH {
		t.Errorf("Arch = %q, want %q", target.Arch, runtime.GOARCH)
	}
}

func TestResolveTargetExplicitPair(t *testing.T) {
	t.Setenv("YAR_OS", "linux")
	t.Setenv("YAR_ARCH", "amd64")

	target, err := ResolveTarget()
	if err != nil {
		t.Fatal(err)
	}
	if target.OS != "linux" {
		t.Errorf("OS = %q, want linux", target.OS)
	}
	if target.Arch != "amd64" {
		t.Errorf("Arch = %q, want amd64", target.Arch)
	}
	if target.Triple != "x86_64-unknown-linux-gnu" {
		t.Errorf("Triple = %q, want x86_64-unknown-linux-gnu", target.Triple)
	}
}

func TestResolveTargetOnlyOneSetErrors(t *testing.T) {
	t.Setenv("YAR_OS", "linux")
	t.Setenv("YAR_ARCH", "")

	_, err := ResolveTarget()
	if err == nil {
		t.Fatal("expected error when only YAR_OS is set")
	}
}

func TestResolveTargetUnknownPairErrors(t *testing.T) {
	t.Setenv("YAR_OS", "plan9")
	t.Setenv("YAR_ARCH", "mips")

	_, err := ResolveTarget()
	if err == nil {
		t.Fatal("expected error for unknown target")
	}
}

func TestTargetExeSuffix(t *testing.T) {
	win := Target{OS: "windows", Arch: "amd64", Triple: "x86_64-pc-windows-msvc"}
	if got := win.ExeSuffix(); got != ".exe" {
		t.Errorf("ExeSuffix() = %q, want .exe", got)
	}

	linux := Target{OS: "linux", Arch: "amd64", Triple: "x86_64-unknown-linux-gnu"}
	if got := linux.ExeSuffix(); got != "" {
		t.Errorf("ExeSuffix() = %q, want empty", got)
	}
}

func TestTargetIsCross(t *testing.T) {
	host := hostTarget()
	if host.IsCross() {
		t.Error("host target should not be cross")
	}

	cross := Target{OS: "windows", Arch: "amd64"}
	if runtime.GOOS != "windows" || runtime.GOARCH != "amd64" {
		if !cross.IsCross() {
			t.Error("windows/amd64 should be cross on non-windows host")
		}
	}
}

func TestTripleMapping(t *testing.T) {
	tests := []struct {
		os, arch, triple string
	}{
		{"darwin", "amd64", "x86_64-apple-darwin"},
		{"darwin", "arm64", "aarch64-apple-darwin"},
		{"linux", "amd64", "x86_64-unknown-linux-gnu"},
		{"linux", "arm64", "aarch64-unknown-linux-gnu"},
		{"windows", "amd64", "x86_64-pc-windows-msvc"},
	}
	for _, tt := range tests {
		t.Setenv("YAR_OS", tt.os)
		t.Setenv("YAR_ARCH", tt.arch)

		target, err := ResolveTarget()
		if err != nil {
			t.Errorf("%s/%s: %v", tt.os, tt.arch, err)
			continue
		}
		if target.Triple != tt.triple {
			t.Errorf("%s/%s: Triple = %q, want %q", tt.os, tt.arch, target.Triple, tt.triple)
		}
	}
}
