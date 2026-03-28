package compiler

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"yar/internal/ast"
	"yar/internal/checker"
	"yar/internal/codegen"
	"yar/internal/diag"
	"yar/internal/parser"
	"yar/internal/token"
)

var errPackageUnavailable = errors.New("package unavailable")

func CompilePath(path string) (*Unit, []diag.Diagnostic, error) {
	graph, diags, err := loadPackageGraph(path)
	if err != nil {
		return nil, nil, err
	}
	if len(diags) > 0 {
		return nil, diags, nil
	}

	program, lowerDiags := lowerPackageGraph(graph)
	if len(lowerDiags) > 0 {
		return nil, lowerDiags, nil
	}

	info, checkDiags := checker.Check(program)
	if len(checkDiags) > 0 {
		return nil, checkDiags, nil
	}

	ir, err := codegen.Generate(program, info)
	if err != nil {
		return nil, nil, err
	}

	return &Unit{IR: ir, Info: info}, nil, nil
}

type packageLoader struct {
	rootDir  string
	packages map[string]*ast.Package
	diag     diag.List
}

func loadPackageGraph(path string) (*ast.PackageGraph, []diag.Diagnostic, error) {
	rootDir, entryDir, err := resolveEntryDirs(path)
	if err != nil {
		return nil, nil, err
	}

	loader := &packageLoader{
		rootDir:  rootDir,
		packages: make(map[string]*ast.Package),
	}

	entry, err := loader.loadPackage("", entryDir)
	if err != nil {
		if errors.Is(err, errPackageUnavailable) {
			return nil, loader.diag.Items(), nil
		}
		return nil, nil, err
	}
	if entry != nil && entry.Name != "main" {
		loader.diag.Add(entry.Files[0].Pos(), "package must be main")
	}

	graph := &ast.PackageGraph{
		EntryPath: "",
		Entry:     entry,
		Packages:  loader.packages,
	}
	loader.checkImportCycles(graph)
	return graph, loader.diag.Items(), nil
}

func resolveEntryDirs(path string) (rootDir, entryDir string, err error) {
	cleanPath := filepath.Clean(path)
	if cleanPath == "." {
		return "", "", fmt.Errorf("source path must name a file or directory")
	}

	info, err := os.Stat(cleanPath)
	if err != nil {
		return "", "", err
	}
	if info.IsDir() {
		return cleanPath, cleanPath, nil
	}
	return filepath.Dir(cleanPath), filepath.Dir(cleanPath), nil
}

func (l *packageLoader) loadPackage(importPath, dir string) (*ast.Package, error) {
	if pkg, ok := l.packages[importPath]; ok {
		return pkg, nil
	}

	entries, err := os.ReadDir(dir)
	if err != nil {
		if importPath != "" && errors.Is(err, os.ErrNotExist) {
			return nil, errPackageUnavailable
		}
		return nil, err
	}

	fileNames := make([]string, 0, len(entries))
	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".yar" {
			continue
		}
		fileNames = append(fileNames, entry.Name())
	}
	sort.Strings(fileNames)
	if len(fileNames) == 0 {
		l.diag.Add(token.Position{File: dir, Line: 1, Column: 1}, "package directory %q has no .yar files", dir)
		return nil, errPackageUnavailable
	}

	var files []*ast.Program
	var packageName string
	for _, name := range fileNames {
		filePath := filepath.Join(dir, name)
		src, err := os.ReadFile(filePath)
		if err != nil {
			return nil, err
		}
		program, diags := parser.ParseFile(filePath, string(src))
		l.diag.Append(diags)
		if program == nil {
			continue
		}
		if packageName == "" {
			packageName = program.PackageName
		} else if program.PackageName != packageName {
			l.diag.Add(program.PackagePos, "package %q does not match package %q in %q", program.PackageName, packageName, dir)
		}
		files = append(files, program)
	}

	if len(files) == 0 {
		return nil, errPackageUnavailable
	}

	pkg := &ast.Package{
		Path:  importPath,
		Name:  packageName,
		Files: files,
	}
	l.packages[importPath] = pkg

	seenImports := make(map[string]struct{})
	for _, file := range files {
		pkg.Structs = append(pkg.Structs, file.Structs...)
		pkg.Enums = append(pkg.Enums, file.Enums...)
		pkg.Functions = append(pkg.Functions, file.Functions...)
		for _, decl := range file.Imports {
			if !validImportPath(decl.Path) {
				l.diag.Add(decl.PathPos, "invalid import path %q", decl.Path)
				continue
			}
			if _, ok := seenImports[decl.Path]; ok {
				continue
			}
			seenImports[decl.Path] = struct{}{}
			targetDir := filepath.Join(l.rootDir, filepath.FromSlash(decl.Path))
			target, err := l.loadPackage(decl.Path, targetDir)
			if err != nil {
				if errors.Is(err, errPackageUnavailable) {
					l.diag.Add(decl.PathPos, "import %q could not be loaded", decl.Path)
					continue
				}
				return nil, err
			}
			if target == nil {
				continue
			}
			if target.Name == "main" {
				l.diag.Add(decl.PathPos, "cannot import package main")
				continue
			}
			if want := lastImportSegment(decl.Path); want != target.Name {
				l.diag.Add(decl.PathPos, "import %q must declare package %q, got %q", decl.Path, want, target.Name)
				continue
			}
			pkg.Imports = append(pkg.Imports, ast.PackageImport{Name: target.Name, Path: decl.Path, Decl: decl})
		}
	}

	return pkg, nil
}

func validImportPath(path string) bool {
	if path == "" || strings.HasPrefix(path, "/") || strings.HasPrefix(path, ".") || strings.Contains(path, "//") {
		return false
	}
	for _, part := range strings.Split(path, "/") {
		if part == "" {
			return false
		}
		for i, r := range part {
			if i == 0 {
				if r != '_' && (r < 'A' || r > 'Z') && (r < 'a' || r > 'z') {
					return false
				}
				continue
			}
			if r != '_' && (r < 'A' || r > 'Z') && (r < 'a' || r > 'z') && (r < '0' || r > '9') {
				return false
			}
		}
	}
	return true
}

func lastImportSegment(path string) string {
	parts := strings.Split(path, "/")
	return parts[len(parts)-1]
}

func (l *packageLoader) checkImportCycles(graph *ast.PackageGraph) {
	visited := make(map[string]bool)
	active := make(map[string]bool)
	stack := make([]string, 0, len(graph.Packages))

	var visit func(pkg *ast.Package)
	visit = func(pkg *ast.Package) {
		if pkg == nil {
			return
		}
		if visited[pkg.Path] {
			return
		}
		visited[pkg.Path] = true
		active[pkg.Path] = true
		stack = append(stack, pkg.Path)
		defer func() {
			stack = stack[:len(stack)-1]
			active[pkg.Path] = false
		}()

		for _, imp := range pkg.Imports {
			target := graph.Packages[imp.Path]
			if target == nil {
				continue
			}
			if active[target.Path] {
				cycle := appendCycle(stack, target.Path)
				l.diag.Add(imp.Decl.PathPos, "import cycle: %s", strings.Join(cycle, " -> "))
				continue
			}
			visit(target)
		}
	}

	visit(graph.Entry)
}

func appendCycle(stack []string, target string) []string {
	start := 0
	for i, path := range stack {
		if path == target {
			start = i
			break
		}
	}
	cycle := append([]string{}, stack[start:]...)
	cycle = append(cycle, target)
	for i, path := range cycle {
		if path == "" {
			cycle[i] = "main"
		}
	}
	return cycle
}

type packageLowerer struct {
	graph     *ast.PackageGraph
	structs   map[string]map[string]*ast.StructDecl
	enums     map[string]map[string]*ast.EnumDecl
	functions map[string]map[string]*ast.FunctionDecl
	imports   map[string]map[string]*ast.Package
	diag      diag.List
}

func lowerPackageGraph(graph *ast.PackageGraph) (*ast.Program, []diag.Diagnostic) {
	l := &packageLowerer{
		graph:     graph,
		structs:   make(map[string]map[string]*ast.StructDecl),
		enums:     make(map[string]map[string]*ast.EnumDecl),
		functions: make(map[string]map[string]*ast.FunctionDecl),
		imports:   make(map[string]map[string]*ast.Package),
	}
	l.indexPackages()
	if !l.diag.Empty() {
		return nil, l.diag.Items()
	}

	program := &ast.Program{
		PackagePos:  graph.Entry.Files[0].PackagePos,
		PackageName: "main",
	}

	packageOrder := make([]string, 0, len(graph.Packages))
	for path := range graph.Packages {
		packageOrder = append(packageOrder, path)
	}
	sort.Strings(packageOrder)
	if len(packageOrder) > 0 && packageOrder[0] == "" {
		packageOrder = append(packageOrder[1:], "")
	}
	packageOrder = append([]string{""}, filterNonEmpty(packageOrder)...)

	seen := make(map[string]struct{})
	for _, path := range packageOrder {
		if _, ok := seen[path]; ok {
			continue
		}
		seen[path] = struct{}{}
		pkg := graph.Packages[path]
		if pkg == nil {
			continue
		}
		program.Enums = append(program.Enums, l.lowerEnums(pkg)...)
		program.Structs = append(program.Structs, l.lowerStructs(pkg)...)
		program.Functions = append(program.Functions, l.lowerFunctions(pkg)...)
	}

	if !l.diag.Empty() {
		return nil, l.diag.Items()
	}
	return program, nil
}

func filterNonEmpty(paths []string) []string {
	out := make([]string, 0, len(paths))
	for _, path := range paths {
		if path != "" {
			out = append(out, path)
		}
	}
	return out
}

func (l *packageLowerer) indexPackages() {
	for path, pkg := range l.graph.Packages {
		if pkg == nil {
			continue
		}
		structs := make(map[string]*ast.StructDecl)
		for _, decl := range pkg.Structs {
			if _, ok := structs[decl.Name]; ok {
				l.diag.Add(decl.NamePos, "struct %q is already declared", decl.Name)
				continue
			}
			structs[decl.Name] = decl
		}
		l.structs[path] = structs

		enums := make(map[string]*ast.EnumDecl)
		for _, decl := range pkg.Enums {
			if _, ok := enums[decl.Name]; ok {
				l.diag.Add(decl.NamePos, "enum %q is already declared", decl.Name)
				continue
			}
			enums[decl.Name] = decl
		}
		l.enums[path] = enums

		functions := make(map[string]*ast.FunctionDecl)
		for _, decl := range pkg.Functions {
			if checker.IsBuiltinFunction(decl.Name) {
				l.diag.Add(decl.NamePos, "function %q is already declared", decl.Name)
				continue
			}
			if _, ok := functions[decl.Name]; ok {
				l.diag.Add(decl.NamePos, "function %q is already declared", decl.Name)
				continue
			}
			functions[decl.Name] = decl
		}
		l.functions[path] = functions

		bindings := make(map[string]*ast.Package)
		for _, imp := range pkg.Imports {
			target := l.graph.Packages[imp.Path]
			if target == nil {
				continue
			}
			if prev, ok := bindings[imp.Name]; ok && prev.Path != imp.Path {
				l.diag.Add(imp.Decl.PathPos, "import name %q is already bound to %q", imp.Name, prev.Path)
				continue
			}
			bindings[imp.Name] = target
		}
		l.imports[path] = bindings
	}

	l.validateExportedDeclarations()
}

func (l *packageLowerer) validateExportedDeclarations() {
	for path, pkg := range l.graph.Packages {
		if pkg == nil {
			continue
		}
		for _, decl := range pkg.Structs {
			if !decl.Exported {
				continue
			}
			for _, field := range decl.Fields {
				l.validateExportedLocalTypeRef(path, field.Type, "struct", decl.Name)
			}
		}
		for _, decl := range pkg.Enums {
			if !decl.Exported {
				continue
			}
			for _, enumCase := range decl.Cases {
				for _, field := range enumCase.Fields {
					l.validateExportedLocalTypeRef(path, field.Type, "enum", decl.Name)
				}
			}
		}
		for _, decl := range pkg.Functions {
			if !decl.Exported {
				continue
			}
			for _, param := range decl.Params {
				l.validateExportedLocalTypeRef(path, param.Type, "function", decl.Name)
			}
			l.validateExportedLocalTypeRef(path, decl.Return, "function", decl.Name)
		}
	}
}

func (l *packageLowerer) validateExportedLocalTypeRef(packagePath string, ref ast.TypeRef, ownerKind, ownerName string) {
	if pointer, ok := checker.ParsePointerType(checker.Type(ref.Name)); ok {
		l.validateExportedLocalTypeRef(packagePath, ast.TypeRef{Name: string(pointer.Elem), Pos: ref.Pos}, ownerKind, ownerName)
		return
	}
	if array, ok := checker.ParseArrayType(checker.Type(ref.Name)); ok {
		l.validateExportedLocalTypeRef(packagePath, ast.TypeRef{Name: string(array.Elem), Pos: ref.Pos}, ownerKind, ownerName)
		return
	}
	if slice, ok := checker.ParseSliceType(checker.Type(ref.Name)); ok {
		l.validateExportedLocalTypeRef(packagePath, ast.TypeRef{Name: string(slice.Elem), Pos: ref.Pos}, ownerKind, ownerName)
		return
	}
	if isBuiltinType(ref.Name) || strings.Contains(ref.Name, ".") {
		return
	}
	if decl, ok := l.structs[packagePath][ref.Name]; ok {
		if decl.Exported {
			return
		}
		l.diag.Add(ref.Pos, "exported %s %q cannot use non-exported type %q", ownerKind, ownerName, ref.Name)
		return
	}
	if decl, ok := l.enums[packagePath][ref.Name]; ok {
		if decl.Exported {
			return
		}
		l.diag.Add(ref.Pos, "exported %s %q cannot use non-exported type %q", ownerKind, ownerName, ref.Name)
	}
}

func (l *packageLowerer) lowerStructs(pkg *ast.Package) []*ast.StructDecl {
	decls := make([]*ast.StructDecl, 0, len(pkg.Structs))
	for _, decl := range pkg.Structs {
		fields := make([]ast.StructField, 0, len(decl.Fields))
		for _, field := range decl.Fields {
			fields = append(fields, ast.StructField{
				Name:    field.Name,
				NamePos: field.NamePos,
				Type:    l.rewriteTypeRef(pkg, field.Type),
			})
		}
		decls = append(decls, &ast.StructDecl{
			StructPos: decl.StructPos,
			Exported:  decl.Exported,
			Name:      canonicalDeclName(pkg, decl.Name),
			NamePos:   decl.NamePos,
			Fields:    fields,
		})
	}
	return decls
}

func (l *packageLowerer) lowerEnums(pkg *ast.Package) []*ast.EnumDecl {
	decls := make([]*ast.EnumDecl, 0, len(pkg.Enums))
	for _, decl := range pkg.Enums {
		cases := make([]ast.EnumCaseDecl, 0, len(decl.Cases))
		for _, enumCase := range decl.Cases {
			fields := make([]ast.StructField, 0, len(enumCase.Fields))
			for _, field := range enumCase.Fields {
				fields = append(fields, ast.StructField{
					Name:    field.Name,
					NamePos: field.NamePos,
					Type:    l.rewriteTypeRef(pkg, field.Type),
				})
			}
			cases = append(cases, ast.EnumCaseDecl{
				Name:    enumCase.Name,
				NamePos: enumCase.NamePos,
				Fields:  fields,
			})
		}
		decls = append(decls, &ast.EnumDecl{
			EnumPos:  decl.EnumPos,
			Exported: decl.Exported,
			Name:     canonicalDeclName(pkg, decl.Name),
			NamePos:  decl.NamePos,
			Cases:    cases,
		})
	}
	return decls
}

func (l *packageLowerer) lowerFunctions(pkg *ast.Package) []*ast.FunctionDecl {
	decls := make([]*ast.FunctionDecl, 0, len(pkg.Functions))
	for _, decl := range pkg.Functions {
		params := make([]ast.Param, 0, len(decl.Params))
		for _, param := range decl.Params {
			params = append(params, ast.Param{
				Name:    param.Name,
				NamePos: param.NamePos,
				Type:    l.rewriteTypeRef(pkg, param.Type),
			})
		}
		decls = append(decls, &ast.FunctionDecl{
			Exported:     decl.Exported,
			Name:         canonicalFunctionName(l.graph.Entry, pkg, decl.Name),
			NamePos:      decl.NamePos,
			Params:       params,
			Return:       l.rewriteTypeRef(pkg, decl.Return),
			ReturnIsBang: decl.ReturnIsBang,
			Body:         l.rewriteBlock(pkg, decl.Body),
		})
	}
	return decls
}

func canonicalFunctionName(entry, pkg *ast.Package, name string) string {
	if pkg == entry && name == "main" {
		return "main"
	}
	return canonicalDeclName(pkg, name)
}

func canonicalDeclName(pkg *ast.Package, name string) string {
	prefix := pkg.Name
	if pkg.Path != "" {
		prefix = strings.ReplaceAll(pkg.Path, "/", ".")
	}
	return prefix + "." + name
}

func (l *packageLowerer) rewriteTypeRef(pkg *ast.Package, ref ast.TypeRef) ast.TypeRef {
	if pointer, ok := checker.ParsePointerType(checker.Type(ref.Name)); ok {
		inner := l.rewriteTypeRef(pkg, ast.TypeRef{Name: string(pointer.Elem), Pos: ref.Pos})
		return ast.TypeRef{Name: string(checker.MakePointerType(checker.Type(inner.Name))), Pos: ref.Pos}
	}
	if array, ok := checker.ParseArrayType(checker.Type(ref.Name)); ok {
		inner := l.rewriteTypeRef(pkg, ast.TypeRef{Name: string(array.Elem), Pos: ref.Pos})
		return ast.TypeRef{Name: string(checker.MakeArrayType(array.Len, checker.Type(inner.Name))), Pos: ref.Pos}
	}
	if slice, ok := checker.ParseSliceType(checker.Type(ref.Name)); ok {
		inner := l.rewriteTypeRef(pkg, ast.TypeRef{Name: string(slice.Elem), Pos: ref.Pos})
		return ast.TypeRef{Name: string(checker.MakeSliceType(checker.Type(inner.Name))), Pos: ref.Pos}
	}
	if isBuiltinType(ref.Name) {
		return ref
	}
	parts := strings.SplitN(ref.Name, ".", 2)
	if len(parts) == 2 {
		target := l.imports[pkg.Path][parts[0]]
		if target == nil {
			l.diag.Add(ref.Pos, "unknown import %q", parts[0])
			return ref
		}
		if decl := l.structs[target.Path][parts[1]]; decl != nil {
			if !decl.Exported {
				l.diag.Add(ref.Pos, "package %q does not export type %q", target.Name, parts[1])
				return ref
			}
			return ast.TypeRef{Name: canonicalDeclName(target, parts[1]), Pos: ref.Pos}
		}
		if decl := l.enums[target.Path][parts[1]]; decl != nil {
			if !decl.Exported {
				l.diag.Add(ref.Pos, "package %q does not export enum %q", target.Name, parts[1])
				return ref
			}
			return ast.TypeRef{Name: canonicalDeclName(target, parts[1]), Pos: ref.Pos}
		}
		l.diag.Add(ref.Pos, "package %q has no type %q", target.Name, parts[1])
		return ref
	}
	if _, ok := l.structs[pkg.Path][ref.Name]; ok {
		return ast.TypeRef{Name: canonicalDeclName(pkg, ref.Name), Pos: ref.Pos}
	}
	if _, ok := l.enums[pkg.Path][ref.Name]; ok {
		return ast.TypeRef{Name: canonicalDeclName(pkg, ref.Name), Pos: ref.Pos}
	}
	return ref
}

func isBuiltinType(name string) bool {
	switch name {
	case string(checker.TypeVoid), string(checker.TypeNoReturn), string(checker.TypeBool), string(checker.TypeI32), string(checker.TypeI64), string(checker.TypeStr), string(checker.TypeError):
		return true
	default:
		return false
	}
}

func (l *packageLowerer) rewriteBlock(pkg *ast.Package, block *ast.BlockStmt) *ast.BlockStmt {
	if block == nil {
		return nil
	}
	stmts := make([]ast.Statement, 0, len(block.Stmts))
	for _, stmt := range block.Stmts {
		stmts = append(stmts, l.rewriteStatement(pkg, stmt))
	}
	return &ast.BlockStmt{LBrace: block.LBrace, Stmts: stmts}
}

func (l *packageLowerer) rewriteStatement(pkg *ast.Package, stmt ast.Statement) ast.Statement {
	switch s := stmt.(type) {
	case *ast.BlockStmt:
		return l.rewriteBlock(pkg, s)
	case *ast.LetStmt:
		return &ast.LetStmt{LetPos: s.LetPos, Name: s.Name, NamePos: s.NamePos, Value: l.rewriteExpr(pkg, s.Value)}
	case *ast.VarStmt:
		return &ast.VarStmt{VarPos: s.VarPos, Name: s.Name, NamePos: s.NamePos, Type: l.rewriteTypeRef(pkg, s.Type), Value: l.rewriteExpr(pkg, s.Value)}
	case *ast.AssignStmt:
		return &ast.AssignStmt{Target: l.rewriteExpr(pkg, s.Target), Value: l.rewriteExpr(pkg, s.Value)}
	case *ast.IfStmt:
		return &ast.IfStmt{IfPos: s.IfPos, Cond: l.rewriteExpr(pkg, s.Cond), Then: l.rewriteBlock(pkg, s.Then), Else: l.rewriteStatement(pkg, s.Else)}
	case *ast.ForStmt:
		return &ast.ForStmt{ForPos: s.ForPos, Init: l.rewriteStatement(pkg, s.Init), Cond: l.rewriteExpr(pkg, s.Cond), Post: l.rewriteStatement(pkg, s.Post), Body: l.rewriteBlock(pkg, s.Body)}
	case *ast.MatchStmt:
		arms := make([]ast.MatchArm, 0, len(s.Arms))
		for _, arm := range s.Arms {
			arms = append(arms, ast.MatchArm{
				CasePos:     arm.CasePos,
				EnumType:    l.rewriteTypeRef(pkg, arm.EnumType),
				CaseName:    arm.CaseName,
				CaseNamePos: arm.CaseNamePos,
				BindName:    arm.BindName,
				BindNamePos: arm.BindNamePos,
				BindIgnore:  arm.BindIgnore,
				Body:        l.rewriteBlock(pkg, arm.Body),
			})
		}
		return &ast.MatchStmt{MatchPos: s.MatchPos, Value: l.rewriteExpr(pkg, s.Value), Arms: arms}
	case *ast.BreakStmt:
		return &ast.BreakStmt{BreakPos: s.BreakPos}
	case *ast.ContinueStmt:
		return &ast.ContinueStmt{ContinuePos: s.ContinuePos}
	case *ast.ReturnStmt:
		return &ast.ReturnStmt{ReturnPos: s.ReturnPos, Value: l.rewriteExpr(pkg, s.Value)}
	case *ast.ExprStmt:
		return &ast.ExprStmt{Expr: l.rewriteExpr(pkg, s.Expr)}
	default:
		return stmt
	}
}

func (l *packageLowerer) rewriteExpr(pkg *ast.Package, expr ast.Expression) ast.Expression {
	if expr == nil {
		return nil
	}
	switch e := expr.(type) {
	case *ast.IdentExpr:
		return &ast.IdentExpr{Name: e.Name, NamePos: e.NamePos}
	case *ast.IntLiteral:
		return &ast.IntLiteral{Value: e.Value, LitPos: e.LitPos}
	case *ast.StringLiteral:
		return &ast.StringLiteral{Value: e.Value, LitPos: e.LitPos}
	case *ast.BoolLiteral:
		return &ast.BoolLiteral{Value: e.Value, LitPos: e.LitPos}
	case *ast.NilLiteral:
		return &ast.NilLiteral{LitPos: e.LitPos}
	case *ast.ErrorLiteral:
		return &ast.ErrorLiteral{Name: e.Name, ErrPos: e.ErrPos}
	case *ast.GroupExpr:
		return &ast.GroupExpr{Inner: l.rewriteExpr(pkg, e.Inner)}
	case *ast.CallExpr:
		args := make([]ast.Expression, 0, len(e.Args))
		for _, arg := range e.Args {
			args = append(args, l.rewriteExpr(pkg, arg))
		}
		return &ast.CallExpr{Callee: l.rewriteCallee(pkg, e.Callee), Args: args}
	case *ast.UnaryExpr:
		return &ast.UnaryExpr{Operator: e.Operator, OpPos: e.OpPos, Inner: l.rewriteExpr(pkg, e.Inner)}
	case *ast.BinaryExpr:
		return &ast.BinaryExpr{Left: l.rewriteExpr(pkg, e.Left), Operator: e.Operator, OpPos: e.OpPos, Right: l.rewriteExpr(pkg, e.Right)}
	case *ast.SelectorExpr:
		if rewritten, ok := l.rewriteEnumCaseSelector(pkg, e); ok {
			return rewritten
		}
		return &ast.SelectorExpr{Inner: l.rewriteExpr(pkg, e.Inner), DotPos: e.DotPos, Name: e.Name, NamePos: e.NamePos}
	case *ast.IndexExpr:
		return &ast.IndexExpr{Inner: l.rewriteExpr(pkg, e.Inner), LBracketPos: e.LBracketPos, Index: l.rewriteExpr(pkg, e.Index)}
	case *ast.SliceExpr:
		return &ast.SliceExpr{Inner: l.rewriteExpr(pkg, e.Inner), LBracketPos: e.LBracketPos, Start: l.rewriteExpr(pkg, e.Start), ColonPos: e.ColonPos, End: l.rewriteExpr(pkg, e.End)}
	case *ast.StructLiteralExpr:
		fields := make([]ast.StructLiteralField, 0, len(e.Fields))
		for _, field := range e.Fields {
			fields = append(fields, ast.StructLiteralField{Name: field.Name, NamePos: field.NamePos, Value: l.rewriteExpr(pkg, field.Value)})
		}
		litType := l.rewriteEnumCaseTypeRef(pkg, e.Type)
		if litType.Name == "" {
			litType = l.rewriteTypeRef(pkg, e.Type)
		}
		return &ast.StructLiteralExpr{Type: litType, LBrace: e.LBrace, Fields: fields}
	case *ast.ArrayLiteralExpr:
		elements := make([]ast.Expression, 0, len(e.Elements))
		for _, element := range e.Elements {
			elements = append(elements, l.rewriteExpr(pkg, element))
		}
		return &ast.ArrayLiteralExpr{Type: l.rewriteTypeRef(pkg, e.Type), LBrace: e.LBrace, Elements: elements}
	case *ast.SliceLiteralExpr:
		elements := make([]ast.Expression, 0, len(e.Elements))
		for _, element := range e.Elements {
			elements = append(elements, l.rewriteExpr(pkg, element))
		}
		return &ast.SliceLiteralExpr{Type: l.rewriteTypeRef(pkg, e.Type), LBrace: e.LBrace, Elements: elements}
	case *ast.PropagateExpr:
		return &ast.PropagateExpr{Inner: l.rewriteExpr(pkg, e.Inner), QuestionPos: e.QuestionPos}
	case *ast.HandleExpr:
		return &ast.HandleExpr{Inner: l.rewriteExpr(pkg, e.Inner), OrPos: e.OrPos, ErrName: e.ErrName, ErrPos: e.ErrPos, Handler: l.rewriteBlock(pkg, e.Handler)}
	default:
		return expr
	}
}

func (l *packageLowerer) rewriteEnumCaseSelector(pkg *ast.Package, expr *ast.SelectorExpr) (ast.Expression, bool) {
	parts, positions, ok := selectorPath(expr)
	if !ok || (len(parts) != 2 && len(parts) != 3) {
		return nil, false
	}

	var target *ast.Package
	var enumName, caseName string
	if len(parts) == 2 {
		target = pkg
		enumName = parts[0]
		caseName = parts[1]
	} else {
		target = l.imports[pkg.Path][parts[0]]
		if target == nil {
			return nil, false
		}
		enumName = parts[1]
		caseName = parts[2]
	}

	decl := l.enums[target.Path][enumName]
	if decl == nil {
		return nil, false
	}
	if target != pkg && !decl.Exported {
		l.diag.Add(positions[len(parts)-2], "package %q does not export enum %q", target.Name, enumName)
	}
	if !enumHasCase(decl, caseName) {
		l.diag.Add(positions[len(parts)-1], "enum %q has no case %q", enumName, caseName)
	}
	return &ast.SelectorExpr{
		Inner:   &ast.IdentExpr{Name: canonicalDeclName(target, enumName), NamePos: positions[len(parts)-2]},
		DotPos:  expr.DotPos,
		Name:    caseName,
		NamePos: positions[len(parts)-1],
	}, true
}

func (l *packageLowerer) rewriteEnumCaseTypeRef(pkg *ast.Package, ref ast.TypeRef) ast.TypeRef {
	parts := strings.Split(ref.Name, ".")
	if len(parts) != 2 && len(parts) != 3 {
		return ast.TypeRef{}
	}

	var target *ast.Package
	var enumName, caseName string
	if len(parts) == 2 {
		target = pkg
		enumName = parts[0]
		caseName = parts[1]
	} else {
		target = l.imports[pkg.Path][parts[0]]
		if target == nil {
			return ast.TypeRef{}
		}
		enumName = parts[1]
		caseName = parts[2]
	}

	decl := l.enums[target.Path][enumName]
	if decl == nil {
		return ast.TypeRef{}
	}
	if target != pkg && !decl.Exported {
		l.diag.Add(ref.Pos, "package %q does not export enum %q", target.Name, enumName)
	}
	if !enumHasCase(decl, caseName) {
		l.diag.Add(ref.Pos, "enum %q has no case %q", enumName, caseName)
	}
	return ast.TypeRef{Name: canonicalDeclName(target, enumName) + "." + caseName, Pos: ref.Pos}
}

func enumHasCase(decl *ast.EnumDecl, caseName string) bool {
	for _, enumCase := range decl.Cases {
		if enumCase.Name == caseName {
			return true
		}
	}
	return false
}

func selectorPath(expr ast.Expression) ([]string, []token.Position, bool) {
	switch e := expr.(type) {
	case *ast.IdentExpr:
		return []string{e.Name}, []token.Position{e.NamePos}, true
	case *ast.SelectorExpr:
		parts, positions, ok := selectorPath(e.Inner)
		if !ok {
			return nil, nil, false
		}
		return append(parts, e.Name), append(positions, e.NamePos), true
	default:
		return nil, nil, false
	}
}

func (l *packageLowerer) rewriteCallee(pkg *ast.Package, callee ast.Expression) ast.Expression {
	if ident, ok := callee.(*ast.IdentExpr); ok {
		if ident.Name == "main" && pkg == l.graph.Entry {
			return &ast.IdentExpr{Name: "main", NamePos: ident.NamePos}
		}
		if _, ok := l.functions[pkg.Path][ident.Name]; ok {
			return &ast.IdentExpr{Name: canonicalDeclName(pkg, ident.Name), NamePos: ident.NamePos}
		}
		return &ast.IdentExpr{Name: ident.Name, NamePos: ident.NamePos}
	}
	selector, ok := callee.(*ast.SelectorExpr)
	if !ok {
		return l.rewriteExpr(pkg, callee)
	}
	inner, ok := selector.Inner.(*ast.IdentExpr)
	if !ok {
		return l.rewriteExpr(pkg, callee)
	}
	target := l.imports[pkg.Path][inner.Name]
	if target == nil {
		return l.rewriteExpr(pkg, callee)
	}
	decl := l.functions[target.Path][selector.Name]
	if decl == nil {
		l.diag.Add(selector.NamePos, "package %q has no function %q", target.Name, selector.Name)
		return &ast.IdentExpr{Name: selector.Name, NamePos: selector.NamePos}
	}
	if !decl.Exported {
		l.diag.Add(selector.NamePos, "package %q does not export function %q", target.Name, selector.Name)
		return &ast.IdentExpr{Name: selector.Name, NamePos: selector.NamePos}
	}
	return &ast.IdentExpr{Name: canonicalDeclName(target, selector.Name), NamePos: selector.NamePos}
}
