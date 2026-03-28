package main

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"time"
	"yar/internal/compiler"
	"yar/internal/diag"
)

func main() {
	os.Exit(run())
}

func run() int {
	if len(os.Args) < 3 {
		fmt.Fprintln(os.Stderr, "usage: yar <check|emit-ir|build|run> <file> [-o output]")
		return 2
	}

	command := os.Args[1]
	switch command {
	case "check":
		return runCheck(os.Args[2])
	case "emit-ir":
		return runEmitIR(os.Args[2])
	case "build":
		return runBuild(os.Args[2:])
	case "run":
		return runRun(os.Args[2])
	default:
		fmt.Fprintf(os.Stderr, "unknown command %q\n", command)
		return 2
	}
}

func runCheck(path string) int {
	_, diagnostics, err := compiler.CompilePath(path)
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

func runEmitIR(path string) int {
	unit, diagnostics, err := compiler.CompilePath(path)
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
	output := "a.out"
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
	_, diagnostics, err := compiler.CompilePath(path)
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
	if err := compiler.BuildPath(ctx, path, output); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	return 0
}

func runRun(path string) int {
	_, diagnostics, err := compiler.CompilePath(path)
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

func printDiagnostics(path string, diagnostics []diag.Diagnostic) {
	var b bytes.Buffer
	b.WriteString(diag.Format(path, diagnostics))
	if b.Len() > 0 && b.Bytes()[b.Len()-1] != '\n' {
		b.WriteByte('\n')
	}
	_, _ = os.Stderr.Write(b.Bytes())
}
