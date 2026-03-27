package compiler

import (
	"bytes"
	"context"
	"errors"
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

	exitErr := &exec.ExitError{}
	ok := errors.As(err, &exitErr)
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

func TestI64Program(t *testing.T) {
	t.Parallel()

	src, err := os.ReadFile(filepath.Join("..", "..", "testdata", "i64.yar"))
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
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
}

func TestPanicProgram(t *testing.T) {
	src, err := os.ReadFile(filepath.Join("..", "..", "testdata", "panic.yar"))
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
	var output bytes.Buffer
	cmd.Stdout = &output
	cmd.Stderr = &output
	err = cmd.Run()
	if err == nil {
		t.Fatal("expected non-zero exit status")
	}

	exitErr := &exec.ExitError{}
	if !errors.As(err, &exitErr) {
		t.Fatalf("expected ExitError, got %T", err)
	}
	if exitErr.ExitCode() != 1 {
		t.Fatalf("unexpected exit code: %d", exitErr.ExitCode())
	}
	if got, want := output.String(), "boom\n"; got != want {
		t.Fatalf("unexpected panic output: got %q want %q", got, want)
	}
}

func TestBuildAndRunPropagateSugar(t *testing.T) {
	t.Parallel()

	src := `
package main

fn divide(a i32, b i32) !i32 {
	if b == 0 {
		return error.DivideByZero
	}
	return a / b
}

fn main() !i32 {
	x := divide(10, 2)?
	print_int(x)
	print("\n")
	return 0
}
`

	output, err := buildAndRun(t, src)
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "5\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestBuildAndRunHandleSugar(t *testing.T) {
	t.Parallel()

	src := `
package main

fn divide(a i32, b i32) !i32 {
	if b == 0 {
		return error.DivideByZero
	}
	return a / b
}

fn main() i32 {
	x := divide(10, 0) or |err| {
		return 7
	}
	return x
}
`

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := Build(ctx, src, outPath); err != nil {
		t.Fatal(err)
	}

	cmd := exec.CommandContext(ctx, outPath)
	err := cmd.Run()
	if err == nil {
		t.Fatal("expected non-zero exit status")
	}

	exitErr := &exec.ExitError{}
	if !errors.As(err, &exitErr) {
		t.Fatalf("expected ExitError, got %T", err)
	}
	if exitErr.ExitCode() != 7 {
		t.Fatalf("unexpected exit code: %d", exitErr.ExitCode())
	}
}

func buildAndRun(t *testing.T, src string) (string, error) {
	t.Helper()

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := Build(ctx, src, outPath); err != nil {
		return "", err
	}

	cmd := exec.CommandContext(ctx, outPath)
	var output bytes.Buffer
	cmd.Stdout = &output
	cmd.Stderr = &output
	if err := cmd.Run(); err != nil {
		return "", err
	}
	return output.String(), nil
}
