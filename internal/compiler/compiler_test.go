package compiler

import (
	"bytes"
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"
)

func TestCompile(t *testing.T) {
	src, err := os.ReadFile(filepath.Join("..", "..", "testdata", "divide.yar"))
	if err != nil {
		t.Fatal(err)
	}

	unit, diags, err := Compile(string(src))
	if err != nil {
		t.Fatal(err)
	}
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}
	if unit.IR == "" {
		t.Fatal("expected LLVM IR")
	}
}

func TestBuildAndRun(t *testing.T) {
	src, err := os.ReadFile(filepath.Join("..", "..", "testdata", "divide.yar"))
	if err != nil {
		t.Fatal(err)
	}

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := Build(ctx, string(src), outPath); err != nil {
		t.Fatal(err)
	}

	cmd := exec.CommandContext(ctx, outPath)
	var stdout bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stdout
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}

	if got, want := stdout.String(), "5\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestUnhandledErrorMain(t *testing.T) {
	src, err := os.ReadFile(filepath.Join("..", "..", "testdata", "unhandled_error.yar"))
	if err != nil {
		t.Fatal(err)
	}

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := Build(ctx, string(src), outPath); err != nil {
		t.Fatal(err)
	}

	cmd := exec.CommandContext(ctx, outPath)
	var stdout bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stdout
	err = cmd.Run()
	if err == nil {
		t.Fatal("expected non-zero exit status")
	}

	exitErr, ok := err.(*exec.ExitError)
	if !ok {
		t.Fatalf("expected ExitError, got %T", err)
	}
	if exitErr.ExitCode() != 1 {
		t.Fatalf("unexpected exit code: %d", exitErr.ExitCode())
	}
	if got, want := stdout.String(), "unhandled error: Boom\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}
