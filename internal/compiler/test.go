package compiler

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	"yar/internal/ast"
	"yar/internal/checker"
	"yar/internal/codegen"
	"yar/internal/diag"
	"yar/internal/parser"
	yarRuntime "yar/internal/runtime"
)

type testFunction struct {
	name string
}

func discoverTestFunctions(graph *ast.PackageGraph) []testFunction {
	if graph.Entry == nil {
		return nil
	}
	var tests []testFunction
	for _, file := range graph.Entry.Files {
		if !isTestFile(file) {
			continue
		}
		for _, fn := range file.Functions {
			if !isTestFunction(fn) {
				continue
			}
			tests = append(tests, testFunction{name: fn.Name})
		}
	}
	return tests
}

func isTestFile(program *ast.Program) bool {
	return strings.HasSuffix(program.PackagePos.File, "_test.yar")
}

func isTestFunction(fn *ast.FunctionDecl) bool {
	if !strings.HasPrefix(fn.Name, "test_") {
		return false
	}
	if fn.Receiver != nil {
		return false
	}
	if len(fn.TypeParams) > 0 {
		return false
	}
	if len(fn.Params) != 1 {
		return false
	}
	param := fn.Params[0]
	if param.Type.Kind != ast.PointerTypeRef {
		return false
	}
	if param.Type.Elem == nil || param.Type.Elem.Kind != ast.NamedTypeRef {
		return false
	}
	if param.Type.Elem.Name != "testing.T" {
		return false
	}
	if fn.Return.Name != "void" {
		return false
	}
	if fn.ReturnIsBang {
		return false
	}
	return true
}

func generateTestMain(pkgName string, tests []testFunction) string {
	var b strings.Builder
	b.WriteString("package " + pkgName + "\n\n")
	b.WriteString("import \"testing\"\n\n")
	b.WriteString("fn main() i32 {\n")
	b.WriteString("    passed := 0\n")
	b.WriteString("    failed := 0\n\n")

	for i, t := range tests {
		v := fmt.Sprintf("t%d", i)
		fmt.Fprintf(&b, "    %s := &testing.T{name: \"%s\", failed: false, messages: []str{}}\n", v, t.name)
		fmt.Fprintf(&b, "    %s(%s)\n", t.name, v)
		fmt.Fprintf(&b, "    if (*%s).failed {\n", v)
		fmt.Fprintf(&b, "        print(\"FAIL: %s\\n\")\n", t.name)
		fmt.Fprintf(&b, "        i%d := 0\n", i)
		fmt.Fprintf(&b, "        for i%d < len((*%s).messages) {\n", i, v)
		fmt.Fprintf(&b, "            print(\"    \" + (*%s).messages[i%d] + \"\\n\")\n", v, i)
		fmt.Fprintf(&b, "            i%d = i%d + 1\n", i, i)
		b.WriteString("        }\n")
		b.WriteString("        failed = failed + 1\n")
		b.WriteString("    } else {\n")
		fmt.Fprintf(&b, "        print(\"PASS: %s\\n\")\n", t.name)
		b.WriteString("        passed = passed + 1\n")
		b.WriteString("    }\n\n")
	}

	b.WriteString("    print(\"\\n\" + to_str(passed) + \" passed, \" + to_str(failed) + \" failed\\n\")\n")
	b.WriteString("    if failed > 0 {\n")
	b.WriteString("        return 1\n")
	b.WriteString("    }\n")
	b.WriteString("    return 0\n")
	b.WriteString("}\n")

	return b.String()
}

func CompileTestPath(path, targetTriple string) (*Unit, []diag.Diagnostic, error) {
	graph, diags, err := loadPackageGraph(path, true)
	if err != nil {
		return nil, nil, err
	}
	if len(diags) > 0 {
		return nil, diags, nil
	}

	tests := discoverTestFunctions(graph)
	if len(tests) == 0 {
		return nil, nil, fmt.Errorf("no test functions found")
	}

	pkgName := graph.Entry.Name
	runnerSrc := generateTestMain(pkgName, tests)

	runnerAST, parseDiags := parser.Parse(runnerSrc)
	if len(parseDiags) > 0 {
		return nil, nil, fmt.Errorf("internal error: generated test runner failed to parse: %v", parseDiags)
	}

	// Remove any existing main() from the entry package files and aggregated functions.
	for _, file := range graph.Entry.Files {
		filtered := make([]*ast.FunctionDecl, 0, len(file.Functions))
		for _, fn := range file.Functions {
			if fn.Name == "main" && fn.Receiver == nil {
				continue
			}
			filtered = append(filtered, fn)
		}
		file.Functions = filtered
	}
	{
		filtered := make([]*ast.FunctionDecl, 0, len(graph.Entry.Functions))
		for _, fn := range graph.Entry.Functions {
			if fn.Name == "main" && fn.Receiver == nil {
				continue
			}
			filtered = append(filtered, fn)
		}
		graph.Entry.Functions = filtered
	}

	// Add the generated runner as a file in the entry package.
	graph.Entry.Files = append(graph.Entry.Files, runnerAST)

	// Ensure the runner's imports are in the entry package's import list and
	// the package graph. The test file already imports testing, so it should
	// be in graph.Packages. Register as entry package imports if not present.
	entryImports := make(map[string]struct{})
	for _, imp := range graph.Entry.Imports {
		entryImports[imp.Path] = struct{}{}
	}
	for _, imp := range runnerAST.Imports {
		if _, ok := entryImports[imp.Path]; ok {
			continue
		}
		if pkg, ok := graph.Packages[imp.Path]; ok {
			graph.Entry.Imports = append(graph.Entry.Imports, ast.PackageImport{
				Name: pkg.Name,
				Path: imp.Path,
				Decl: imp,
			})
		}
	}
	// Also add runner functions to entry package functions.
	graph.Entry.Functions = append(graph.Entry.Functions, runnerAST.Functions...)

	program, lowerDiags := lowerPackageGraph(graph)
	if len(lowerDiags) > 0 {
		return nil, lowerDiags, nil
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

	return &Unit{IR: ir, Info: info}, nil, nil
}

func TestPath(ctx context.Context, path string) ([]diag.Diagnostic, error) {
	target, err := ResolveTarget()
	if err != nil {
		return nil, err
	}
	if target.IsCross() {
		return nil, fmt.Errorf("cannot execute cross-compiled test binary (target %s/%s)", target.OS, target.Arch)
	}

	unit, diags, compileErr := CompileTestPath(path, target.Triple)
	if compileErr != nil {
		return nil, compileErr
	}
	if len(diags) > 0 {
		return diags, nil
	}

	tmpDir, err := os.MkdirTemp("", "yar-test-*")
	if err != nil {
		return nil, err
	}
	defer os.RemoveAll(tmpDir)

	irPath := filepath.Join(tmpDir, "main.ll")
	runtimePath := filepath.Join(tmpDir, "runtime.c")
	if err := os.WriteFile(irPath, []byte(unit.IR), 0o600); err != nil {
		return nil, err
	}
	if err := os.WriteFile(runtimePath, []byte(yarRuntime.Source()), 0o600); err != nil {
		return nil, err
	}

	outputPath := filepath.Join(tmpDir, "test"+target.ExeSuffix())
	if err := invokeCC(ctx, target, irPath, runtimePath, outputPath); err != nil {
		return nil, err
	}

	cmd := exec.CommandContext(ctx, outputPath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return nil, cmd.Run()
}
