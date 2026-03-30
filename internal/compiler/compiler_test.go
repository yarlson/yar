package compiler

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"testing"
	"time"

	"yar/internal/diag"
)

func TestCompile(t *testing.T) {
	src, err := os.ReadFile(fixturePath("divide"))
	if err != nil {
		t.Fatal(err)
	}

	unit, diags, err := Compile(string(src), "")
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
	src, err := os.ReadFile(fixturePath("divide"))
	if err != nil {
		t.Fatal(err)
	}

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := Build(ctx, string(src), hostTarget(), outPath); err != nil {
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
	src, err := os.ReadFile(fixturePath("unhandled_error"))
	if err != nil {
		t.Fatal(err)
	}

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := Build(ctx, string(src), hostTarget(), outPath); err != nil {
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

	src, err := os.ReadFile(fixturePath("i64"))
	if err != nil {
		t.Fatal(err)
	}

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := Build(ctx, string(src), hostTarget(), outPath); err != nil {
		t.Fatal(err)
	}

	cmd := exec.CommandContext(ctx, outPath)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
}

func TestPanicProgram(t *testing.T) {
	src, err := os.ReadFile(fixturePath("panic"))
	if err != nil {
		t.Fatal(err)
	}

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := Build(ctx, string(src), hostTarget(), outPath); err != nil {
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

	src, err := os.ReadFile(fixturePath("structs_and_loops"))
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

	src, err := os.ReadFile(fixturePath("bool_operators"))
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

	src, err := os.ReadFile(fixturePath("slices"))
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

func TestPointerFixtureProgram(t *testing.T) {
	t.Parallel()

	src, err := os.ReadFile(fixturePath("pointers"))
	if err != nil {
		t.Fatal(err)
	}

	output, err := buildAndRun(t, string(src))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "3\n2\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestGenericFixtureProgram(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "generics", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "7\nok\n2\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestClosureFixtureProgram(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "closures", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "13\n9\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestCompilePathSupportsImportedGenerics(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "generics_imports", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "hello\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestEnumFixtureProgram(t *testing.T) {
	t.Parallel()

	src, err := os.ReadFile(fixturePath("enums"))
	if err != nil {
		t.Fatal(err)
	}

	output, err := buildAndRun(t, string(src))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "name\nmain\nident\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestMapFixtureProgram(t *testing.T) {
	t.Parallel()

	src, err := os.ReadFile(fixturePath("maps"))
	if err != nil {
		t.Fatal(err)
	}

	output, err := buildAndRun(t, string(src))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "2\nhas hello\nno missing\n2\n42\n2\nten\n1\ncaught\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestMapKeysFixtureProgram(t *testing.T) {
	t.Parallel()

	src, err := os.ReadFile(fixturePath("maps_keys"))
	if err != nil {
		t.Fatal(err)
	}

	output, err := buildAndRun(t, string(src))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "3\n3\n3\n1\n1\n1\n0\n2\n"; got != want {
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
	print(to_str(x))
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

	if err := Build(ctx, src, hostTarget(), outPath); err != nil {
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
	print(to_str(values[1]))
	return 0
}
`

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := Build(ctx, src, hostTarget(), outPath); err != nil {
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

	unit, diags, err := CompilePath(filepath.Join("..", "..", "testdata", "imports_ok", "main.yar"), "")
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

func TestBuildAndRunLocalDepsProgram(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "deps_local"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "7\n30\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestBuildAndRunMethodFixtureProgram(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "methods", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "5\nada\neve\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestBuildAndRunInterfaceProgram(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "interfaces", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "hi ada\n5\n8\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestGarbageCollectionFixtureProgram(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	path := filepath.Join("..", "..", "testdata", "garbage_collection", "main.yar")
	if err := BuildPath(ctx, path, hostTarget(), outPath); err != nil {
		t.Fatal(err)
	}

	cmd := exec.CommandContext(ctx, outPath)
	cmd.Env = append(os.Environ(), "YAR_GC_HEAP_TARGET_BYTES=65536")
	var output bytes.Buffer
	cmd.Stdout = &output
	cmd.Stderr = &output
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
	if got, want := output.String(), "142000\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestBuildPathAllSamplePrograms(t *testing.T) {
	samples, err := sampleProgramPaths()
	if err != nil {
		t.Fatal(err)
	}

	for _, sample := range samples {
		sample := sample
		t.Run(sample, func(t *testing.T) {
			tmpDir := t.TempDir()
			outPath := filepath.Join(tmpDir, "program")
			ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
			defer cancel()

			if err := BuildPath(ctx, sample, hostTarget(), outPath); err != nil {
				t.Fatalf("build sample %q: %v", sample, err)
			}
		})
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

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"), "")
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

func TestCompilePathRejectsUnexportedMethod(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	writeSourceFile(t, filepath.Join(root, "main.yar"), `package main

import "lib"

fn main() i32 {
	box := lib.make_box(3)
	return box.secret()
}
`)
	writeSourceFile(t, filepath.Join(root, "lib", "lib.yar"), `package lib

pub struct Box {
	value i32
}

pub fn make_box(value i32) Box {
	return Box{value: value}
}

fn (b Box) secret() i32 {
	return b.value
}
`)

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"), "")
	if err != nil {
		t.Fatal(err)
	}
	if len(diags) == 0 {
		t.Fatal("expected diagnostics")
	}
	if got := joinDiagnosticMessages(diags); !strings.Contains(got, "package \"lib\" does not export method \"secret\"") {
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

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"), "")
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

func TestCompilePathSupportsImportedEnums(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	writeSourceFile(t, filepath.Join(root, "main.yar"), `package main

import "lib"

fn main() i32 {
	expr := lib.make_name()
	match expr {
	case lib.Expr.Int(_) {
		return 1
	}
	case lib.Expr.Name(v) {
		print(v.text)
		print("\n")
	}
	}

	kind := lib.kind()
	match kind {
	case lib.TokenKind.Ident {
		return 0
	}
	case lib.TokenKind.Int {
		return 2
	}
	}
}
`)
	writeSourceFile(t, filepath.Join(root, "lib", "lib.yar"), `package lib

pub enum TokenKind {
	Ident
	Int
}

pub enum Expr {
	Int { value i32 }
	Name { text str }
}

pub fn make_name() Expr {
	return Expr.Name{text: "ok"}
}

pub fn kind() TokenKind {
	return TokenKind.Ident
}
`)

	output, err := buildAndRunPath(t, filepath.Join(root, "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "ok\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
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

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"), "")
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

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"), "")
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

func TestCompilePathRejectsExportedFunctionUsingHiddenEnumType(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	writeSourceFile(t, filepath.Join(root, "main.yar"), `package main

import "lib"

fn main() i32 {
	return 0
}
`)
	writeSourceFile(t, filepath.Join(root, "lib", "lib.yar"), `package lib

enum hidden {
	A
}

pub fn make() hidden {
	return hidden.A
}
`)

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"), "")
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

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"), "")
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

	_, diags, err := CompilePath(filepath.Join(root, "main.yar"), "")
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

	if err := BuildPath(ctx, filepath.Join(root, "main.yar"), hostTarget(), outPath); err != nil {
		t.Fatal(err)
	}

	cmd := exec.CommandContext(ctx, outPath)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
}

func TestStdlibStringsFixtureProgram(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "stdlib_strings", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "strings ok\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestStdlibUTF8FixtureProgram(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "stdlib_utf8", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "utf8 ok\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestStdlibConvFixtureProgram(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "stdlib_conv", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "conv ok\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestStdlibStringsExtFixtureProgram(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "stdlib_strings_ext", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "strings_ext ok\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestStdlibSortFixtureProgram(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "stdlib_sort", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "sort ok\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestCompilePathLowersHostFilesystemDecls(t *testing.T) {
	t.Parallel()

	unit, diags, err := CompilePath(filepath.Join("..", "..", "testdata", "stdlib_fs_path", "main.yar"), "")
	if err != nil {
		t.Fatal(err)
	}
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	for _, want := range []string{
		"declare i32 @yar_fs_read_file(%yar.str, ptr)",
		"declare i32 @yar_fs_write_file(%yar.str, %yar.str)",
		"declare i32 @yar_fs_read_dir(%yar.str, ptr)",
		"declare i32 @yar_fs_stat(%yar.str, ptr)",
		"declare i32 @yar_fs_mkdir_all(%yar.str)",
		"declare i32 @yar_fs_remove_all(%yar.str)",
		"declare i32 @yar_fs_temp_dir(%yar.str, ptr)",
	} {
		if !strings.Contains(unit.IR, want) {
			t.Fatalf("expected %q in IR:\n%s", want, unit.IR)
		}
	}
}

func TestCompilePathLowersHostProcessDecls(t *testing.T) {
	t.Parallel()

	unit, diags, err := CompilePath(filepath.Join("..", "..", "testdata", "stdlib_process_env", "main.yar"), "")
	if err != nil {
		t.Fatal(err)
	}
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	for _, want := range []string{
		"declare void @yar_set_args(i32, ptr)",
		"declare void @yar_process_args(ptr)",
		"declare i32 @yar_process_run(ptr, ptr)",
		"declare i32 @yar_process_run_inherit(ptr, ptr)",
		"declare i32 @yar_env_lookup(%yar.str, ptr)",
		"declare void @yar_eprint(ptr, i64)",
		"define i32 @main(i32 %argc, ptr %argv)",
		"call void @yar_set_args(i32 %argc, ptr %argv)",
	} {
		if !strings.Contains(unit.IR, want) {
			t.Fatalf("expected %q in IR:\n%s", want, unit.IR)
		}
	}
}

func TestStdlibFSPathFixtureProgram(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "stdlib_fs_path", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "fs_path ok\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestStdlibProcessEnvFixtureProgram(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	captureScript := filepath.Join(root, "capture.sh")
	inheritScript := filepath.Join(root, "inherit.sh")
	writeSourceFile(t, captureScript, "#!/bin/sh\nprintf 'captured stdout\\n'\nprintf 'captured stderr\\n' >&2\nexit 7\n")
	writeSourceFile(t, inheritScript, "#!/bin/sh\nprintf 'inherit stdout\\n'\nprintf 'inherit stderr\\n' >&2\nexit 3\n")
	if err := os.Chmod(captureScript, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(inheritScript, 0o755); err != nil {
		t.Fatal(err)
	}

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := BuildPath(ctx, filepath.Join("..", "..", "testdata", "stdlib_process_env", "main.yar"), hostTarget(), outPath); err != nil {
		t.Fatal(err)
	}

	cmd := exec.CommandContext(ctx, outPath, captureScript, inheritScript)
	cmd.Env = append(os.Environ(), "YAR_PROCESS_ENV_TEST=env ok")
	var output bytes.Buffer
	cmd.Stdout = &output
	cmd.Stderr = &output
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}

	for _, want := range []string{
		"stdio stderr\n",
		"inherit stdout\n",
		"inherit stderr\n",
		"process_env ok\n",
	} {
		if !strings.Contains(output.String(), want) {
			t.Fatalf("expected output to contain %q, got %q", want, output.String())
		}
	}
}

func TestUnhandledHostFilesystemErrorMain(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	missingPath := filepath.Join(root, "missing.txt")
	writeSourceFile(t, filepath.Join(root, "main.yar"), fmt.Sprintf(`package main

import "fs"

fn main() !i32 {
	fs.read_file(%q)?
	return 0
}
`, missingPath))

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := BuildPath(ctx, filepath.Join(root, "main.yar"), hostTarget(), outPath); err != nil {
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
	if got, want := output.String(), "unhandled error: NotFound\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestUnhandledHostProcessErrorMain(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	missingPath := filepath.Join(root, "missing-executable")
	writeSourceFile(t, filepath.Join(root, "main.yar"), fmt.Sprintf(`package main

import "process"

fn main() !i32 {
	process.run([]str{%q})?
	return 0
}
`, missingPath))

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := BuildPath(ctx, filepath.Join(root, "main.yar"), hostTarget(), outPath); err != nil {
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
	if got, want := output.String(), "unhandled error: NotFound\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestUnhandledHostProcessInvalidArgumentMain(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	writeSourceFile(t, filepath.Join(root, "main.yar"), `package main

import "process"

fn main() !i32 {
	process.run([]str{})?
	return 0
}
`)

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := BuildPath(ctx, filepath.Join(root, "main.yar"), hostTarget(), outPath); err != nil {
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
	if got, want := output.String(), "unhandled error: InvalidArgument\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestStringOpsFixtureProgram(t *testing.T) {
	t.Parallel()

	output, err := buildAndRunPath(t, filepath.Join("..", "..", "testdata", "string_ops", "main.yar"))
	if err != nil {
		t.Fatal(err)
	}
	if got, want := output, "eq ok\nne ok\nall ok\n"; got != want {
		t.Fatalf("unexpected program output: got %q want %q", got, want)
	}
}

func TestStringIndexOutOfRangePanics(t *testing.T) {
	t.Parallel()

	src := `
package main

fn main() i32 {
	s := "hi"
	print(to_str(s[2]))
	return 0
}
`

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := Build(ctx, src, hostTarget(), outPath); err != nil {
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
	if got, want := output.String(), "runtime failure: string index out of range\n"; got != want {
		t.Fatalf("unexpected panic output: got %q want %q", got, want)
	}
}

func TestStringSliceOutOfRangePanics(t *testing.T) {
	t.Parallel()

	src := `
package main

fn main() i32 {
	s := "hi"
	sub := s[0:3]
	print(sub)
	return 0
}
`

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := Build(ctx, src, hostTarget(), outPath); err != nil {
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
	if got, want := output.String(), "runtime failure: slice range out of bounds\n"; got != want {
		t.Fatalf("unexpected panic output: got %q want %q", got, want)
	}
}

func TestEmptyStringConcat(t *testing.T) {
	t.Parallel()

	src := `
package main

fn main() i32 {
	a := ""
	b := ""
	c := a + b
	if len(c) != 0 {
		return 1
	}
	d := a + "hello"
	if d != "hello" {
		return 2
	}
	e := "world" + b
	if e != "world" {
		return 3
	}
	return 0
}
`

	output, err := buildAndRun(t, src)
	if err != nil {
		t.Fatal(err)
	}
	if output != "" {
		t.Fatalf("unexpected output: %q", output)
	}
}

func TestLocalPackageShadowsStdlib(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	writeSourceFile(t, filepath.Join(root, "main.yar"), `package main

import "strings"

fn main() i32 {
	if strings.hello() != 42 {
		return 1
	}
	return 0
}
`)
	writeSourceFile(t, filepath.Join(root, "strings", "strings.yar"), `package strings

pub fn hello() i32 {
	return 42
}
`)

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := BuildPath(ctx, filepath.Join(root, "main.yar"), hostTarget(), outPath); err != nil {
		t.Fatal(err)
	}

	cmd := exec.CommandContext(ctx, outPath)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
}

func TestBuildReportsHelpfulErrorWhenCCNotFound(t *testing.T) {
	t.Setenv("CC", "nonexistent-compiler-binary")

	src := `package main

fn main() i32 {
	return 0
}
`
	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	err := Build(ctx, src, hostTarget(), outPath)
	if err == nil {
		t.Fatal("expected error when CC is not found")
	}
	if !strings.Contains(err.Error(), "not found") {
		t.Fatalf("expected 'not found' in error, got: %s", err)
	}
	if !strings.Contains(err.Error(), "CC") {
		t.Fatalf("expected 'CC' hint in error, got: %s", err)
	}
}

func TestBuildRespectsCC(t *testing.T) {
	clangPath, err := exec.LookPath("clang")
	if err != nil {
		t.Skip("clang not available")
	}
	t.Setenv("CC", clangPath)

	src := `package main

fn main() i32 {
	return 0
}
`
	output, err := buildAndRun(t, src)
	if err != nil {
		t.Fatal(err)
	}
	if output != "" {
		t.Fatalf("unexpected output: %q", output)
	}
}

func TestInternalBuiltinRejectedInUserCode(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		body string
	}{
		{"i32_to_i64", "x := i32_to_i64(1)\n\t_ := x"},
		{"i64_to_i32", "x := i64_to_i32(1)\n\t_ := x"},
		{"chr", "x := chr(65)\n\t_ := x"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			root := t.TempDir()
			src := "package main\n\nfn main() i32 {\n\t" + tt.body + "\n\treturn 0\n}\n"
			writeSourceFile(t, filepath.Join(root, "main.yar"), src)

			_, diags, err := CompilePath(filepath.Join(root, "main.yar"), "")
			if err != nil {
				t.Fatalf("expected diagnostics, got error: %v", err)
			}
			if len(diags) == 0 {
				t.Fatal("expected diagnostics for internal builtin usage")
			}
			if got := joinDiagnosticMessages(diags); !strings.Contains(got, "internal to the standard library") {
				t.Fatalf("unexpected diagnostics: %s", got)
			}
		})
	}
}

func TestCompileGenericDiagnostics(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		src    string
		substr string
	}{
		{
			name: "missing type args on generic function",
			src: `
package main

fn id[T](value T) T {
	return value
}

fn main() i32 {
	x := id(1)
	return x
}
`,
			substr: "generic function \"id\" requires explicit type arguments",
		},
		{
			name: "wrong type args on generic function",
			src: `
package main

fn id[T](value T) T {
	return value
}

fn main() i32 {
	x := id[i32, i64](1)
	return x
}
`,
			substr: "generic function \"id\" expects 1 type arguments, got 2",
		},
		{
			name: "instantiation type checks substituted body",
			src: `
package main

fn zero[T]() T {
	return 0
}

fn main() i32 {
	msg := zero[str]()
	print(msg)
	return 0
}
`,
			substr: "cannot return untyped-int from function returning str",
		},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			_, diags, err := Compile(tc.src, "")
			if err != nil {
				t.Fatalf("expected diagnostics, got error: %v", err)
			}
			if len(diags) == 0 {
				t.Fatal("expected diagnostics")
			}
			if got := joinDiagnosticMessages(diags); !strings.Contains(got, tc.substr) {
				t.Fatalf("unexpected diagnostics: %s", got)
			}
		})
	}
}

func buildAndRun(t *testing.T, src string) (string, error) {
	t.Helper()

	tmpDir := t.TempDir()
	outPath := filepath.Join(tmpDir, "program")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := Build(ctx, src, hostTarget(), outPath); err != nil {
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

	if err := BuildPath(ctx, path, hostTarget(), outPath); err != nil {
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

func fixturePath(name string) string {
	return filepath.Join("..", "..", "testdata", name, "main.yar")
}

func sampleProgramPaths() ([]string, error) {
	paths, err := filepath.Glob(filepath.Join("..", "..", "testdata", "*", "main.yar"))
	if err != nil {
		return nil, err
	}
	sort.Strings(paths)
	return paths, nil
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

func TestCarriageReturnEscape(t *testing.T) {
	src := "package main\nfn main() i32 {\n\ts := \"a\\rb\"\n\tif len(s) != 3 { return 1 }\n\treturn 0\n}\n"
	output, err := buildAndRun(t, src)
	if err != nil {
		t.Fatalf("unexpected error: %v\noutput: %s", err, output)
	}
}

func TestNullEscape(t *testing.T) {
	src := "package main\nfn main() i32 {\n\ts := \"a\\0b\"\n\tif len(s) != 3 { return 1 }\n\treturn 0\n}\n"
	output, err := buildAndRun(t, src)
	if err != nil {
		t.Fatalf("unexpected error: %v\noutput: %s", err, output)
	}
}

func TestCharLiteral(t *testing.T) {
	src := `package main
fn main() i32 {
	if 'a' != 97 { return 1 }
	if '\n' != 10 { return 2 }
	if '\t' != 9 { return 3 }
	if '\\' != 92 { return 4 }
	if '\'' != 39 { return 5 }
	var x i32 = 'z'
	if x != 122 { return 6 }
	return 0
}
`
	output, err := buildAndRun(t, src)
	if err != nil {
		t.Fatalf("unexpected error: %v\noutput: %s", err, output)
	}
}

func TestMatchElseArm(t *testing.T) {
	src := `package main

enum Color {
	Red
	Green
	Blue
}

fn describe(c Color) i32 {
	match c {
	case Color.Red { return 1 }
	else { return 0 }
	}
}

fn main() i32 {
	if describe(Color.Red) != 1 { return 1 }
	if describe(Color.Green) != 0 { return 2 }
	if describe(Color.Blue) != 0 { return 3 }
	return 0
}
`
	output, err := buildAndRun(t, src)
	if err != nil {
		t.Fatalf("unexpected error: %v\noutput: %s", err, output)
	}
}

func TestMatchElseOnlyArm(t *testing.T) {
	src := `package main

enum Direction {
	Up
	Down
}

fn classify(d Direction) i32 {
	match d {
	else { return 42 }
	}
}

fn main() i32 {
	if classify(Direction.Up) != 42 { return 1 }
	if classify(Direction.Down) != 42 { return 2 }
	return 0
}
`
	output, err := buildAndRun(t, src)
	if err != nil {
		t.Fatalf("unexpected error: %v\noutput: %s", err, output)
	}
}

func TestImplicitPointerDeref(t *testing.T) {
	src := `package main

struct Point {
	x i32
	y i32
}

fn set_x(p *Point, val i32) void {
	p.x = val
}

fn get_x(p *Point) i32 {
	return p.x
}

fn main() i32 {
	p := &Point{x: 1, y: 2}
	if p.x != 1 { return 1 }
	if p.y != 2 { return 2 }
	set_x(p, 10)
	if get_x(p) != 10 { return 3 }
	return 0
}
`
	output, err := buildAndRun(t, src)
	if err != nil {
		t.Fatalf("unexpected error: %v\noutput: %s", err, output)
	}
}

func TestUntypedIntBinaryI64(t *testing.T) {
	src := `package main
fn main() i32 {
	var x i64 = 0 - 1
	if x != 0 - i32_to_i64(1) { return 1 }
	var y i64 = 2 + 3
	if y != i32_to_i64(5) { return 2 }
	z := 0 - 1
	if z != 0 - 1 { return 3 }
	return 0
}
`
	output, err := buildAndRun(t, src)
	if err != nil {
		t.Fatalf("unexpected error: %v\noutput: %s", err, output)
	}
}

func TestStringBuilder(t *testing.T) {
	src := `package main
fn main() i32 {
	b := sb_new()
	sb_write(b, "hello")
	sb_write(b, " ")
	sb_write(b, "world")
	result := sb_string(b)
	if len(result) != 11 { return 1 }

	// Build in a loop
	b2 := sb_new()
	var i i32 = 0
	for i < 100 {
		sb_write(b2, "x")
		i = i + 1
	}
	result2 := sb_string(b2)
	if len(result2) != 100 { return 2 }

	return 0
}
`
	output, err := buildAndRun(t, src)
	if err != nil {
		t.Fatalf("unexpected error: %v\noutput: %s", err, output)
	}
}
