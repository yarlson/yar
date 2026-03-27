package compiler

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

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

func Compile(src string) (*Unit, []diag.Diagnostic, error) {
	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		return nil, parseDiags, nil
	}

	info, checkDiags := checker.Check(program)
	if len(checkDiags) > 0 {
		return nil, checkDiags, nil
	}

	ir, err := codegen.Generate(program, info)
	if err != nil {
		return nil, nil, err
	}

	return &Unit{
		IR:   ir,
		Info: info,
	}, nil, nil
}

func Build(ctx context.Context, src string, outputPath string) error {
	unit, diags, err := Compile(src)
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
	if err := os.WriteFile(irPath, []byte(unit.IR), 0o644); err != nil {
		return err
	}
	if err := os.WriteFile(runtimePath, []byte(yarRuntime.Source()), 0o644); err != nil {
		return err
	}

	cmd := exec.CommandContext(ctx, "clang", "-Wno-override-module", irPath, runtimePath, "-o", outputPath)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("clang failed: %w\n%s", err, strings.TrimSpace(string(output)))
	}
	return nil
}

func Run(ctx context.Context, src string) error {
	tmpDir, err := os.MkdirTemp("", "yar-run-*")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmpDir)

	outputPath := filepath.Join(tmpDir, "program")
	if err := Build(ctx, src, outputPath); err != nil {
		return err
	}

	cmd := exec.CommandContext(ctx, outputPath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.Stdin = os.Stdin
	return cmd.Run()
}
