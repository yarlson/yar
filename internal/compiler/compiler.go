package compiler

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"yar/internal/checker"
	"yar/internal/codegen"
	"yar/internal/diag"
	"yar/internal/parser"

	yarRuntime "yar/internal/runtime"
)

type Unit struct {
	IR   string
	Info checker.Info
}

func Compile(src, targetTriple string) (*Unit, []diag.Diagnostic, error) {
	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		return nil, parseDiags, nil
	}

	program, genericDiags := monomorphizeProgram(program)
	if len(genericDiags) > 0 {
		return nil, genericDiags, nil
	}

	info, checkDiags := checker.Check(program)
	if len(checkDiags) > 0 {
		return nil, checkDiags, nil
	}

	ir, err := codegen.Generate(program, info, targetTriple)
	if err != nil {
		return nil, nil, err
	}

	return &Unit{
		IR:   ir,
		Info: info,
	}, nil, nil
}

func Build(ctx context.Context, src string, target Target, outputPath string) error {
	unit, diags, err := Compile(src, target.Triple)
	if err != nil {
		return err
	}
	if len(diags) > 0 {
		return fmt.Errorf("diagnostics available")
	}

	tmpDir, err := os.MkdirTemp("", "yar-build-*")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmpDir)

	irPath := filepath.Join(tmpDir, "main.ll")
	runtimePath := filepath.Join(tmpDir, "runtime.c")
	//nolint:gosec // irPath is derived from an internal temporary directory, not user input.
	if err := os.WriteFile(irPath, []byte(unit.IR), 0o600); err != nil {
		return err
	}
	if err := os.WriteFile(runtimePath, []byte(yarRuntime.Source()), 0o600); err != nil {
		return err
	}

	return invokeCC(ctx, target, irPath, runtimePath, outputPath)
}

func BuildPath(ctx context.Context, path string, target Target, outputPath string) error {
	unit, diags, err := CompilePath(path, target.Triple)
	if err != nil {
		return err
	}
	if len(diags) > 0 {
		return fmt.Errorf("diagnostics available")
	}

	tmpDir, err := os.MkdirTemp("", "yar-build-*")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmpDir)

	irPath := filepath.Join(tmpDir, "main.ll")
	runtimePath := filepath.Join(tmpDir, "runtime.c")
	if err := os.WriteFile(irPath, []byte(unit.IR), 0o600); err != nil {
		return err
	}
	if err := os.WriteFile(runtimePath, []byte(yarRuntime.Source()), 0o600); err != nil {
		return err
	}

	return invokeCC(ctx, target, irPath, runtimePath, outputPath)
}

func Run(ctx context.Context, src string) error {
	target, err := ResolveTarget()
	if err != nil {
		return err
	}
	if target.IsCross() {
		return fmt.Errorf("cannot execute cross-compiled binary (target %s/%s)", target.OS, target.Arch)
	}

	tmpDir, err := os.MkdirTemp("", "yar-run-*")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmpDir)

	outputPath := filepath.Join(tmpDir, "program"+target.ExeSuffix())
	if err := Build(ctx, src, target, outputPath); err != nil {
		return err
	}

	cmd := exec.CommandContext(ctx, outputPath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.Stdin = os.Stdin
	return cmd.Run()
}

func RunPath(ctx context.Context, path string) error {
	target, err := ResolveTarget()
	if err != nil {
		return err
	}
	if target.IsCross() {
		return fmt.Errorf("cannot execute cross-compiled binary (target %s/%s)", target.OS, target.Arch)
	}

	tmpDir, err := os.MkdirTemp("", "yar-run-*")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmpDir)

	outputPath := filepath.Join(tmpDir, "program"+target.ExeSuffix())
	if err := BuildPath(ctx, path, target, outputPath); err != nil {
		return err
	}

	cmd := exec.CommandContext(ctx, outputPath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.Stdin = os.Stdin
	return cmd.Run()
}
