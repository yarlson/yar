package compiler

import (
	"bytes"
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	yarRuntime "yar/internal/runtime"
)

func fixtureDir(name string) string {
	return filepath.Join("..", "..", "testdata", name)
}

func TestTestDiscovery(t *testing.T) {
	graph, diags, err := loadPackageGraph(fixtureDir("testing_basic"), true)
	if err != nil {
		t.Fatal(err)
	}
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %v", diags)
	}

	tests := discoverTestFunctions(graph)
	if got := len(tests); got != 5 {
		t.Fatalf("expected 5 test functions, got %d", got)
	}

	want := []string{"test_add", "test_greet", "test_divide", "test_divide_by_zero", "test_bool_assertions"}
	for i, w := range want {
		if tests[i].name != w {
			t.Errorf("test %d: got %q, want %q", i, tests[i].name, w)
		}
	}
}

func TestTestDiscoveryIgnoresNonTestFunctions(t *testing.T) {
	graph, diags, err := loadPackageGraph(fixtureDir("testing_basic"), true)
	if err != nil {
		t.Fatal(err)
	}
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %v", diags)
	}

	tests := discoverTestFunctions(graph)
	for _, tt := range tests {
		if !strings.HasPrefix(tt.name, "test_") {
			t.Errorf("non-test function discovered: %q", tt.name)
		}
	}
}

func TestTestFilesExcludedFromNormalCompile(t *testing.T) {
	_, diags, err := CompilePath(fixtureDir("testing_basic"), "")
	if err != nil {
		t.Fatal(err)
	}
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics in normal compile: %v", diags)
	}
}

func TestCompileTestPathBasic(t *testing.T) {
	unit, diags, err := CompileTestPath(fixtureDir("testing_basic"), "")
	if err != nil {
		t.Fatalf("compile error: %v", err)
	}
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %v", diags)
	}
	if unit.IR == "" {
		t.Fatal("expected LLVM IR")
	}
}

func TestTestPathPassingTests(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	unit, diags, err := CompileTestPath(fixtureDir("testing_basic"), hostTarget().Triple)
	if err != nil {
		t.Fatalf("compile error: %v", err)
	}
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %v", diags)
	}

	stdout := buildAndRunTestBinary(ctx, t, unit)

	if !strings.Contains(stdout, "PASS: test_add") {
		t.Error("missing PASS for test_add")
	}
	if !strings.Contains(stdout, "5 passed, 0 failed") {
		t.Error("missing summary line")
	}
}

func TestTestPathFailingTests(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	unit, diags, err := CompileTestPath(fixtureDir("testing_fail"), hostTarget().Triple)
	if err != nil {
		t.Fatalf("compile error: %v", err)
	}
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %v", diags)
	}

	tmpDir := t.TempDir()
	target := hostTarget()
	outPath := filepath.Join(tmpDir, "test"+target.ExeSuffix())
	writeTestBuildFiles(t, tmpDir, unit)
	irPath := filepath.Join(tmpDir, "main.ll")
	runtimePath := filepath.Join(tmpDir, "runtime.c")
	if err := invokeCC(ctx, target, irPath, runtimePath, outPath); err != nil {
		t.Fatal(err)
	}

	cmd := exec.CommandContext(ctx, outPath)
	var stdout bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stdout
	err = cmd.Run()

	output := stdout.String()
	if err == nil {
		t.Fatal("expected non-zero exit code for failing tests")
	}
	if !strings.Contains(output, "FAIL: test_wrong_sum") {
		t.Error("missing FAIL for test_wrong_sum")
	}
	if !strings.Contains(output, "got 4, want 5") {
		t.Error("missing failure message for test_wrong_sum")
	}
	if !strings.Contains(output, "PASS: test_pass") {
		t.Error("missing PASS for test_pass")
	}
	if !strings.Contains(output, "1 passed, 2 failed") {
		t.Errorf("missing summary line, got: %q", output)
	}
}

func TestGenerateTestMain(t *testing.T) {
	tests := []testFunction{
		{name: "test_add"},
		{name: "test_sub"},
	}
	src := generateTestMain("main", tests)

	if !strings.Contains(src, "package main") {
		t.Error("missing package declaration")
	}
	if !strings.Contains(src, `import "testing"`) {
		t.Error("missing testing import")
	}
	if !strings.Contains(src, "test_add(t0)") {
		t.Error("missing test_add call")
	}
	if !strings.Contains(src, "test_sub(t1)") {
		t.Error("missing test_sub call")
	}
}

func buildAndRunTestBinary(ctx context.Context, t *testing.T, unit *Unit) string {
	t.Helper()
	tmpDir := t.TempDir()
	target := hostTarget()

	writeTestBuildFiles(t, tmpDir, unit)

	outPath := filepath.Join(tmpDir, "test"+target.ExeSuffix())
	irPath := filepath.Join(tmpDir, "main.ll")
	runtimePath := filepath.Join(tmpDir, "runtime.c")
	if err := invokeCC(ctx, target, irPath, runtimePath, outPath); err != nil {
		t.Fatal(err)
	}

	cmd := exec.CommandContext(ctx, outPath)
	var stdout bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stdout
	if err := cmd.Run(); err != nil {
		t.Fatalf("test binary failed: %v\noutput: %s", err, stdout.String())
	}

	return stdout.String()
}

func writeTestBuildFiles(t *testing.T, tmpDir string, unit *Unit) {
	t.Helper()
	irPath := filepath.Join(tmpDir, "main.ll")
	runtimePath := filepath.Join(tmpDir, "runtime.c")

	if err := os.WriteFile(irPath, []byte(unit.IR), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(runtimePath, []byte(yarRuntime.Source()), 0o600); err != nil {
		t.Fatal(err)
	}
}
