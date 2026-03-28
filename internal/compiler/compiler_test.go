package compiler

import (
	"bytes"
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"yar/internal/diag"
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

func TestV02FixtureProgram(t *testing.T) {
	t.Parallel()

	src, err := os.ReadFile(filepath.Join("..", "..", "testdata", "structs_and_loops.yar"))
	if err != nil {
		t.Fatal(err)
	}

	output, err := buildAndRun(t, string(src))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "B\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestBoolOperatorFixtureProgram(t *testing.T) {
	t.Parallel()

	src, err := os.ReadFile(filepath.Join("..", "..", "testdata", "bool_operators.yar"))
	if err != nil {
		t.Fatal(err)
	}

	output, err := buildAndRun(t, string(src))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "and-left\nor-left\ndone\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestSliceFixtureProgram(t *testing.T) {
	t.Parallel()

	src, err := os.ReadFile(filepath.Join("..", "..", "testdata", "slices.yar"))
	if err != nil {
		t.Fatal(err)
	}

	output, err := buildAndRun(t, string(src))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "3\n9\n1\n2\n4\n4\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
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

func TestBuildAndRunV02Program(t *testing.T) {
	t.Parallel()

	src := `
package main

struct User {
	id i32
	name str
}

fn get_user(id i32) !User {
	if id <= 0 {
		return error.InvalidUserID
	} else {
		return User{id: id, name: "fallback"}
	}
}

fn main() !i32 {
	var winner User
	users := [3]User{
		User{id: 1, name: "alice"},
		User{id: 2, name: "bob"},
		User{id: 3, name: "eve"},
	}

	for i := 0; i < len(users); i = i + 1 {
		user := users[i]
		if !(user.id % 2 == 0) {
			continue
		} else {
			winner = user
			break
		}
	}

	if winner.id == 0 {
		winner = get_user(2)?
	}

	winner.name = "B"
	print(winner.name)
	print("\n")
	return 0
}
`

	output, err := buildAndRun(t, src)
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "B\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestBuildAndRunErrorableStructHandling(t *testing.T) {
	t.Parallel()

	src := `
package main

struct User {
	id i32
	name str
}

fn get_user(id i32) !User {
	if id <= 0 {
		return error.InvalidUserID
	}
	return User{id: id, name: "ok"}
}

fn main() i32 {
	user := get_user(0) or |err| {
		print("missing\n")
		return 0
	}
	print(user.name)
	print("\n")
	return 1
}
`

	output, err := buildAndRun(t, src)
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "missing\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestSliceIndexOutOfRangePanics(t *testing.T) {
	t.Parallel()

	src := `
package main

fn main() i32 {
	values := []i32{1}
	print_int(values[1])
	return 0
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
	var output bytes.Buffer
	cmd.Stdout = &output
	cmd.Stderr = &output
	err := cmd.Run()
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
	if got, want := output.String(), "runtime failure: slice index out of range\n"; got != want {
		t.Fatalf("unexpected panic output: got %q want %q", got, want)
	}
}

func TestCompilePathMultiFilePackage(t *testing.T) {
	t.Parallel()

	unit, diags, err := CompilePath(filepath.Join("..", "..", "testdata", "imports_ok", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}
	if unit.IR == "" {
		t.Fatal("expected LLVM IR")
	}
	if !strings.Contains(unit.IR, "@yar.lexer.classify") {
		t.Fatalf("expected imported package function in IR:\n%s", unit.IR)
	}
}

func TestBuildAndRunImportFixtureProgram(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "imports_ok", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "ok\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestCompilePathRejectsUnexportedImport(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	writeSourceFile(t, filepath.Join(root, "main.yar"), `package main

import "lib"

fn main() i32 {
	return lib.hidden()
}
`)
	writeSourceFile(t, filepath.Join(root, "lib", "lib.yar"), `package lib

fn hidden() i32 {
	return 0
}
`)

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if len(diags) == 0 {
		t.Fatal("expected diagnostics")
	}
	if got := joinDiagnosticMessages(diags); !strings.Contains(got, "package \"lib\" does not export function \"hidden\"") {
		t.Fatalf("unexpected diagnostics: %s", got)
	}
}

func TestCompilePathRejectsImportCycle(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	writeSourceFile(t, filepath.Join(root, "main.yar"), `package main

import "a"

fn main() i32 {
	return a.value()
}
`)
	writeSourceFile(t, filepath.Join(root, "a", "a.yar"), `package a

import "b"

pub fn value() i32 {
	return b.value()
}
`)
	writeSourceFile(t, filepath.Join(root, "b", "b.yar"), `package b

import "a"

pub fn value() i32 {
	return 0
}
`)

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if len(diags) == 0 {
		t.Fatal("expected diagnostics")
	}
	if got := joinDiagnosticMessages(diags); !strings.Contains(got, "import cycle") {
		t.Fatalf("unexpected diagnostics: %s", got)
	}
}

func TestCompilePathRejectsExportedFunctionUsingHiddenType(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	writeSourceFile(t, filepath.Join(root, "main.yar"), `package main

import "lib"

fn main() i32 {
	return 0
}
`)
	writeSourceFile(t, filepath.Join(root, "lib", "lib.yar"), `package lib

struct hidden {
	id i32
}

pub fn make() hidden {
	return hidden{id: 1}
}
`)

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if len(diags) == 0 {
		t.Fatal("expected diagnostics")
	}
	if got := joinDiagnosticMessages(diags); !strings.Contains(got, "exported function \"make\" cannot use non-exported type \"hidden\"") {
		t.Fatalf("unexpected diagnostics: %s", got)
	}
}

func TestCompilePathRejectsExportedStructUsingHiddenType(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	writeSourceFile(t, filepath.Join(root, "main.yar"), `package main

import "lib"

fn main() i32 {
	return 0
}
`)
	writeSourceFile(t, filepath.Join(root, "lib", "lib.yar"), `package lib

struct hidden {
	id i32
}

pub struct Wrapper {
	inner hidden
}
`)

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if len(diags) == 0 {
		t.Fatal("expected diagnostics")
	}
	if got := joinDiagnosticMessages(diags); !strings.Contains(got, "exported struct \"Wrapper\" cannot use non-exported type \"hidden\"") {
		t.Fatalf("unexpected diagnostics: %s", got)
	}
}

func TestCompilePathRejectsBuiltinFunctionShadowing(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	writeSourceFile(t, filepath.Join(root, "main.yar"), `package main

fn append(values []i32, value i32) []i32 {
	return values
}

fn main() i32 {
	return 0
}
`)

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if len(diags) == 0 {
		t.Fatal("expected diagnostics")
	}
	if got := joinDiagnosticMessages(diags); !strings.Contains(got, "function \"append\" is already declared") {
		t.Fatalf("unexpected diagnostics: %s", got)
	}
}

func TestCompilePathReportsMissingImportAsDiagnostic(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	writeSourceFile(t, filepath.Join(root, "main.yar"), `package main

import "missing"

fn main() i32 {
	return 0
}
`)

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"))
	if err != nil {
		t.Fatalf("expected diagnostics, got error: %v", err)
	}
	if len(diags) == 0 {
		t.Fatal("expected diagnostics")
	}
	if got := joinDiagnosticMessages(diags); !strings.Contains(got, "import \"missing\" could not be loaded") {
		t.Fatalf("unexpected diagnostics: %s", got)
	}
}

func TestBuildPathAllowsDistinctPackageNamesThatPreviouslyCollided(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	writeSourceFile(t, filepath.Join(root, "main.yar"), `package main

import "a/b_c"
import "a_b/c"

fn main() i32 {
	left := b_c.make()
	right := c.make()
	if left.value == 1 && right.value == 2 {
		return 0
	}
	return 1
}
`)
	writeSourceFile(t, filepath.Join(root, "a", "b_c", "lib.yar"), `package b_c

pub struct Pair {
	value i32
}

pub fn make() Pair {
	return Pair{value: 1}
}
`)
	writeSourceFile(t, filepath.Join(root, "a_b", "c", "lib.yar"), `package c

pub struct Pair {
	value i32
}

pub fn make() Pair {
	return Pair{value: 2}
}
`)

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := BuildPath(ctx, filepath.Join(root, "main.yar"), outPath); err != nil {
		t.Fatal(err)
	}

	cmd := exec.CommandContext(ctx, outPath)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
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

func buildAndRunPath(t *testing.T, path string) (string, error) {
	t.Helper()

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := BuildPath(ctx, path, outPath); err != nil {
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

func writeSourceFile(t *testing.T, path, src string) {
	t.Helper()

	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(src), 0o600); err != nil {
		t.Fatal(err)
	}
}

func joinDiagnosticMessages(diags []diag.Diagnostic) string {
	parts := make([]string, 0, len(diags))
	for _, diagnostic := range diags {
		parts = append(parts, diagnostic.Message)
	}
	return strings.Join(parts, "\n")
}
