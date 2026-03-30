package main

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"time"
	"yar/internal/compiler"
	"yar/internal/deps"
	"yar/internal/diag"
)

func main() {
	os.Exit(run())
}

func run() int {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: yar <command> [arguments]")
		return 2
	}

	command := os.Args[1]
	switch command {
	case "check":
		if len(os.Args) < 3 {
			fmt.Fprintln(os.Stderr, "usage: yar check <file|dir>")
			return 2
		}
		return runCheck(os.Args[2])
	case "emit-ir":
		if len(os.Args) < 3 {
			fmt.Fprintln(os.Stderr, "usage: yar emit-ir <file|dir>")
			return 2
		}
		return runEmitIR(os.Args[2:])
	case "build":
		if len(os.Args) < 3 {
			fmt.Fprintln(os.Stderr, "usage: yar build <file|dir> [-o output]")
			return 2
		}
		return runBuild(os.Args[2:])
	case "run":
		if len(os.Args) < 3 {
			fmt.Fprintln(os.Stderr, "usage: yar run <file|dir>")
			return 2
		}
		return runRun(os.Args[2])
	case "test":
		if len(os.Args) < 3 {
			fmt.Fprintln(os.Stderr, "usage: yar test <file|dir>")
			return 2
		}
		return runTest(os.Args[2])
	case "init":
		return runInit()
	case "add":
		return runAdd(os.Args[2:])
	case "remove":
		return runRemove(os.Args[2:])
	case "fetch":
		return runFetch()
	case "lock":
		return runLock()
	case "update":
		return runUpdate(os.Args[2:])
	default:
		fmt.Fprintf(os.Stderr, "unknown command %q\n", command)
		return 2
	}
}

func runCheck(path string) int {
	_, diagnostics, err := compiler.CompilePath(path, "")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	if len(diagnostics) > 0 {
		printDiagnostics(path, diagnostics)
		return 1
	}
	return 0
}

func runEmitIR(args []string) int {
	var path string
	for i := 0; i < len(args); i++ {
		if args[i] != "" && args[i][0] == '-' {
			fmt.Fprintf(os.Stderr, "unknown emit-ir flag %q\n", args[i])
			return 2
		}
		if path != "" {
			fmt.Fprintln(os.Stderr, "usage: yar emit-ir <file>")
			return 2
		}
		path = args[i]
	}
	if path == "" {
		fmt.Fprintln(os.Stderr, "usage: yar emit-ir <file>")
		return 2
	}

	target, err := compiler.ResolveTarget()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	unit, diagnostics, err := compiler.CompilePath(path, target.Triple)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	if len(diagnostics) > 0 {
		printDiagnostics(path, diagnostics)
		return 1
	}
	fmt.Print(unit.IR)
	return 0
}

func runBuild(args []string) int {
	target, err := compiler.ResolveTarget()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	output := defaultOutputName(target)
	var path string
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "-o":
			if i+1 >= len(args) {
				fmt.Fprintln(os.Stderr, "usage: yar build <file> [-o output]")
				return 2
			}
			output = args[i+1]
			i++
		default:
			if args[i] != "" && args[i][0] == '-' {
				fmt.Fprintf(os.Stderr, "unknown build flag %q\n", args[i])
				return 2
			}
			if path != "" {
				fmt.Fprintln(os.Stderr, "usage: yar build <file> [-o output]")
				return 2
			}
			path = args[i]
		}
	}
	if path == "" {
		fmt.Fprintln(os.Stderr, "usage: yar build <file> [-o output]")
		return 2
	}
	_, diagnostics, err := compiler.CompilePath(path, target.Triple)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	if len(diagnostics) > 0 {
		printDiagnostics(path, diagnostics)
		return 1
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	if err := compiler.BuildPath(ctx, path, target, output); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	return 0
}

func runRun(path string) int {
	_, diagnostics, err := compiler.CompilePath(path, "")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	if len(diagnostics) > 0 {
		printDiagnostics(path, diagnostics)
		return 1
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	if err := compiler.RunPath(ctx, path); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	return 0
}

func runTest(path string) int {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	diagnostics, err := compiler.TestPath(ctx, path)
	if len(diagnostics) > 0 {
		printDiagnostics(path, diagnostics)
		return 1
	}
	if err != nil {
		exitErr := &exec.ExitError{}
		if errors.As(err, &exitErr) {
			return exitErr.ExitCode()
		}
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	return 0
}

// --- Package management commands ---

func runInit() int {
	if _, err := os.Stat(deps.ManifestFile); err == nil {
		fmt.Fprintln(os.Stderr, "yar.toml already exists")
		return 1
	}

	dirName := "myproject"
	if abs, err := filepath.Abs("."); err == nil {
		dirName = filepath.Base(abs)
	}
	if !deps.ValidAlias(dirName) {
		dirName = "myproject"
	}

	m := &deps.Manifest{
		Package:      deps.PackageInfo{Name: dirName},
		Dependencies: make(map[string]deps.Dependency),
	}
	content, err := deps.WriteManifest(m)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	if err := os.WriteFile(deps.ManifestFile, []byte(content), 0o600); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	fmt.Println("created yar.toml")
	return 0
}

func runAdd(args []string) int {
	if len(args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: yar add <alias> <git-url> [--tag=v1.0.0|--rev=abc123|--branch=main]")
		fmt.Fprintln(os.Stderr, "       yar add <alias> --path=<dir>")
		return 2
	}

	alias := args[0]
	if !deps.ValidAlias(alias) {
		fmt.Fprintf(os.Stderr, "invalid alias %q\n", alias)
		return 1
	}

	dep, err := parseDepArgs(args[1:])
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 2
	}

	m, err := readOrCreateManifest()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	m.Dependencies[alias] = dep
	if err := writeManifest(m); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	fmt.Printf("added dependency %q\n", alias)

	// Update lock file if it's a git dependency.
	if !dep.IsLocal() {
		if code := runLock(); code != 0 {
			return code
		}
	}
	return 0
}

func runRemove(args []string) int {
	if len(args) != 1 {
		fmt.Fprintln(os.Stderr, "usage: yar remove <alias>")
		return 2
	}

	alias := args[0]
	m, err := deps.ReadManifest(deps.ManifestFile)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	if _, ok := m.Dependencies[alias]; !ok {
		fmt.Fprintf(os.Stderr, "dependency %q not found\n", alias)
		return 1
	}

	delete(m.Dependencies, alias)
	if err := writeManifest(m); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	fmt.Printf("removed dependency %q\n", alias)

	// Update lock file.
	if code := runLock(); code != 0 {
		return code
	}
	return 0
}

func runFetch() int {
	lockEntries, err := deps.ReadLockFile(deps.LockFile)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	cacheDir, err := deps.CacheDir()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()

	for _, entry := range lockEntries {
		if deps.IsCached(cacheDir, entry.Git, entry.Commit) {
			continue
		}
		fmt.Printf("fetching %s...\n", entry.Name)
		dep := deps.Dependency{
			Git:    entry.Git,
			Tag:    entry.Tag,
			Rev:    entry.Rev,
			Branch: entry.Branch,
		}
		dir, fetchErr := deps.Fetch(ctx, cacheDir, dep, entry.Commit)
		if fetchErr != nil {
			fmt.Fprintf(os.Stderr, "fetching %s: %v\n", entry.Name, fetchErr)
			return 1
		}
		if verifyErr := deps.VerifyHash(dir, entry.Hash); verifyErr != nil {
			fmt.Fprintf(os.Stderr, "verifying %s: %v\n", entry.Name, verifyErr)
			return 1
		}
	}
	fmt.Println("all dependencies fetched")
	return 0
}

func runLock() int {
	m, err := deps.ReadManifest(deps.ManifestFile)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	// Filter to git dependencies only.
	hasGitDeps := false
	for _, dep := range m.Dependencies {
		if !dep.IsLocal() {
			hasGitDeps = true
			break
		}
	}
	if !hasGitDeps {
		// Remove lock file if it exists and there are no git deps.
		_ = os.Remove(deps.LockFile)
		return 0
	}

	cacheDir, err := deps.CacheDir()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()

	rootDir, err := filepath.Abs(".")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	resolved, resolveErr := deps.Resolve(ctx, rootDir, m, cacheDir)
	if resolveErr != nil {
		fmt.Fprintln(os.Stderr, resolveErr)
		return 1
	}

	entries := deps.ToLockEntries(resolved)
	content := deps.WriteLockFile(entries)
	if err := os.WriteFile(deps.LockFile, []byte(content), 0o600); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	fmt.Println("yar.lock updated")
	return 0
}

func runUpdate(_ []string) int {
	// Re-resolve all dependencies from yar.toml.
	return runLock()
}

// --- Helpers ---

func parseDepArgs(args []string) (deps.Dependency, error) {
	var dep deps.Dependency
	for _, arg := range args {
		switch {
		case hasFlag(arg, "--tag="):
			dep.Tag = arg[len("--tag="):]
		case hasFlag(arg, "--rev="):
			dep.Rev = arg[len("--rev="):]
		case hasFlag(arg, "--branch="):
			dep.Branch = arg[len("--branch="):]
		case hasFlag(arg, "--path="):
			dep.Path = arg[len("--path="):]
		default:
			if dep.Git != "" {
				return deps.Dependency{}, fmt.Errorf("unexpected argument %q", arg)
			}
			dep.Git = arg
		}
	}
	if dep.Path != "" {
		if dep.Git != "" || dep.Tag != "" || dep.Rev != "" || dep.Branch != "" {
			return deps.Dependency{}, fmt.Errorf("--path cannot be combined with git options")
		}
		return dep, nil
	}
	if dep.Git == "" {
		return deps.Dependency{}, fmt.Errorf("git URL required")
	}
	if dep.Tag == "" && dep.Rev == "" && dep.Branch == "" {
		return deps.Dependency{}, fmt.Errorf("one of --tag, --rev, or --branch required")
	}
	return dep, nil
}

func hasFlag(arg, prefix string) bool {
	return len(arg) > len(prefix) && arg[:len(prefix)] == prefix
}

func readOrCreateManifest() (*deps.Manifest, error) {
	m, err := deps.ReadManifest(deps.ManifestFile)
	if err == nil {
		return m, nil
	}
	if !errors.Is(err, os.ErrNotExist) {
		return nil, err
	}
	dirName := "myproject"
	if abs, absErr := filepath.Abs("."); absErr == nil {
		dirName = filepath.Base(abs)
	}
	if !deps.ValidAlias(dirName) {
		dirName = "myproject"
	}
	return &deps.Manifest{
		Package:      deps.PackageInfo{Name: dirName},
		Dependencies: make(map[string]deps.Dependency),
	}, nil
}

func writeManifest(m *deps.Manifest) error {
	content, err := deps.WriteManifest(m)
	if err != nil {
		return err
	}
	return os.WriteFile(deps.ManifestFile, []byte(content), 0o600)
}

func defaultOutputName(t compiler.Target) string {
	if t.OS == "windows" {
		return "a.exe"
	}
	return "a.out"
}

func printDiagnostics(path string, diagnostics []diag.Diagnostic) {
	var b bytes.Buffer
	b.WriteString(diag.Format(path, diagnostics))
	if b.Len() > 0 && b.Bytes()[b.Len()-1] != '\n' {
		b.WriteByte('\n')
	}
	_, _ = os.Stderr.Write(b.Bytes())
}
