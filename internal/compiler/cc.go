package compiler

import (
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"strings"
)

// findCC returns the C compiler command to use.
// It checks the CC environment variable first, then falls back to "clang".
func findCC() string {
	if cc := os.Getenv("CC"); cc != "" {
		return cc
	}
	return "clang"
}

// invokeCC compiles LLVM IR and the C runtime into a native executable.
func invokeCC(ctx context.Context, target Target, irPath, runtimePath, outputPath string) error {
	cc := findCC()
	args := []string{"-Wno-override-module"}
	if target.Triple != "" {
		args = append(args, "--target="+target.Triple)
	}
	args = append(args, irPath, runtimePath, "-o", outputPath)
	if strings.Contains(target.Triple, "windows") {
		args = append(args, "-lws2_32")
	}
	cmd := exec.CommandContext(ctx, cc, args...)
	output, err := cmd.CombinedOutput()
	if err != nil {
		if errors.Is(err, exec.ErrNotFound) {
			return fmt.Errorf("%s not found; install clang or set the CC environment variable: %w", cc, err)
		}
		return fmt.Errorf("%s failed: %w\n%s", cc, err, strings.TrimSpace(string(output)))
	}
	return nil
}
