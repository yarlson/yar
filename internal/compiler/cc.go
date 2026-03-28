package compiler

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"strings"
)

// exeSuffix returns ".exe" on Windows, "" elsewhere.
func exeSuffix() string {
	if runtime.GOOS == "windows" {
		return ".exe"
	}
	return ""
}

// findCC returns the C compiler command to use.
// It checks the CC environment variable first, then falls back to "clang".
func findCC() string {
	if cc := os.Getenv("CC"); cc != "" {
		return cc
	}
	return "clang"
}

// invokeCC compiles LLVM IR and the C runtime into a native executable.
func invokeCC(ctx context.Context, irPath, runtimePath, outputPath string) error {
	cc := findCC()
	cmd := exec.CommandContext(ctx, cc, "-Wno-override-module", irPath, runtimePath, "-o", outputPath)
	output, err := cmd.CombinedOutput()
	if err != nil {
		if errors.Is(err, exec.ErrNotFound) {
			return fmt.Errorf("%s not found; install clang or set the CC environment variable: %w", cc, err)
		}
		return fmt.Errorf("%s failed: %w\n%s", cc, err, strings.TrimSpace(string(output)))
	}
	return nil
}
