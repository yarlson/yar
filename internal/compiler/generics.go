package compiler

import (
	"strings"

	"yar/internal/ast"
	"yar/internal/checker"
	"yar/internal/diag"
)

type genericMonomorphizer struct {
	diag                  diag.List
	genericStructs        map[string]*ast.StructDecl
	genericFunctions      map[string]*ast.FunctionDecl
	nonGenericFunctions   map[string]struct{}
	nonGenericNamedTypes  map[string]struct{}
	structInstantiating   map[string]bool
	functionInstantiating map[string]bool
	output                *ast.Program
}

func monomorphizeProgram(program *ast.Program) (*ast.Program, []diag.Diagnostic) {
	m := &genericMonomorphizer{
		genericStructs:        make(map[string]*ast.StructDecl),
		genericFunctions:      make(map[string]*ast.FunctionDecl),
		nonGenericFunctions:   make(map[string]struct{}),
		nonGenericNamedTypes:  make(map[string]struct{}),
		structInstantiating:   make(map[string]bool),
		functionInstantiating: make(map[string]bool),
		output: &ast.Program{
			PackagePos:  program.PackagePos,
			PackageName: program.PackageName,
			Imports:     append([]ast.ImportDecl(nil), program.Imports...),
			Enums:       append([]*ast.EnumDecl(nil), program.Enums...),
		},
	}

	for _, decl := range program.Enums {
		m.nonGenericNamedTypes[decl.Name] = struct{}{}
	}
	for _, decl := range program.Structs {
		if len(decl.TypeParams) > 0 {
			m.genericStructs[decl.Name] = decl
			continue
		}
		m.nonGenericNamedTypes[decl.Name] = struct{}{}
	}
	for _, decl := range program.Functions {
		if len(decl.TypeParams) > 0 {
			m.genericFunctions[decl.Name] = decl
			continue
		}
		m.nonGenericFunctions[decl.Name] = struct{}{}
	}

	for _, decl := range program.Structs {
		if len(decl.TypeParams) > 0 {
			continue
		}
		m.output.Structs = append(m.output.Structs, m.rewriteStructDecl(decl, nil))
	}
	for _, decl := range program.Functions {
		if len(decl.TypeParams) > 0 {
			continue
		}
		m.output.Functions = append(m.output.Functions, m.rewriteFunctionDecl(decl, nil))
	}

	return m.output, m.diag.Items()
}

func (m *genericMonomorphizer) rewriteStructDecl(decl *ast.StructDecl, subst map[string]ast.TypeRef) *ast.StructDecl {
	fields := make([]ast.StructField, 0, len(decl.Fields))
	for _, field := range decl.Fields {
		fields = append(fields, ast.StructField{
			Name:    field.Name,
			NamePos: field.NamePos,
			Type:    m.rewriteTypeRef(field.Type, subst),
		})
	}
	return &ast.StructDecl{
		StructPos: decl.StructPos,
		Exported:  decl.Exported,
		Name:      decl.Name,
		NamePos:   decl.NamePos,
		Fields:    fields,
	}
}

func (m *genericMonomorphizer) rewriteFunctionDecl(decl *ast.FunctionDecl, subst map[string]ast.TypeRef) *ast.FunctionDecl {
	var receiver *ast.ReceiverDecl
	if decl.Receiver != nil {
		receiver = &ast.ReceiverDecl{
			Name:    decl.Receiver.Name,
			NamePos: decl.Receiver.NamePos,
			Type:    m.rewriteTypeRef(decl.Receiver.Type, subst),
		}
	}
	params := make([]ast.Param, 0, len(decl.Params))
	for _, param := range decl.Params {
		params = append(params, ast.Param{
			Name:    param.Name,
			NamePos: param.NamePos,
			Type:    m.rewriteTypeRef(param.Type, subst),
		})
	}
	return &ast.FunctionDecl{
		Exported:     decl.Exported,
		Name:         decl.Name,
		NamePos:      decl.NamePos,
		Receiver:     receiver,
		Params:       params,
		Return:       m.rewriteTypeRef(decl.Return, subst),
		ReturnIsBang: decl.ReturnIsBang,
		Body:         m.rewriteBlock(decl.Body, subst),
	}
}

func (m *genericMonomorphizer) rewriteBlock(block *ast.BlockStmt, subst map[string]ast.TypeRef) *ast.BlockStmt {
	if block == nil {
		return nil
	}
	stmts := make([]ast.Statement, 0, len(block.Stmts))
	for _, stmt := range block.Stmts {
		stmts = append(stmts, m.rewriteStatement(stmt, subst))
	}
	return &ast.BlockStmt{LBrace: block.LBrace, Stmts: stmts}
}

func (m *genericMonomorphizer) rewriteStatement(stmt ast.Statement, subst map[string]ast.TypeRef) ast.Statement {
	switch s := stmt.(type) {
	case *ast.BlockStmt:
		return m.rewriteBlock(s, subst)
	case *ast.LetStmt:
		return &ast.LetStmt{LetPos: s.LetPos, Name: s.Name, NamePos: s.NamePos, Value: m.rewriteExpr(s.Value, subst)}
	case *ast.VarStmt:
		return &ast.VarStmt{VarPos: s.VarPos, Name: s.Name, NamePos: s.NamePos, Type: m.rewriteTypeRef(s.Type, subst), Value: m.rewriteExpr(s.Value, subst)}
	case *ast.AssignStmt:
		return &ast.AssignStmt{Target: m.rewriteExpr(s.Target, subst), Value: m.rewriteExpr(s.Value, subst)}
	case *ast.IfStmt:
		return &ast.IfStmt{IfPos: s.IfPos, Cond: m.rewriteExpr(s.Cond, subst), Then: m.rewriteBlock(s.Then, subst), Else: m.rewriteStatement(s.Else, subst)}
	case *ast.ForStmt:
		return &ast.ForStmt{ForPos: s.ForPos, Init: m.rewriteStatement(s.Init, subst), Cond: m.rewriteExpr(s.Cond, subst), Post: m.rewriteStatement(s.Post, subst), Body: m.rewriteBlock(s.Body, subst)}
	case *ast.BreakStmt:
		return &ast.BreakStmt{BreakPos: s.BreakPos}
	case *ast.ContinueStmt:
		return &ast.ContinueStmt{ContinuePos: s.ContinuePos}
	case *ast.ReturnStmt:
		return &ast.ReturnStmt{ReturnPos: s.ReturnPos, Value: m.rewriteExpr(s.Value, subst)}
	case *ast.MatchStmt:
		arms := make([]ast.MatchArm, 0, len(s.Arms))
		for _, arm := range s.Arms {
			arms = append(arms, ast.MatchArm{
				CasePos:     arm.CasePos,
				EnumType:    m.rewriteTypeRef(arm.EnumType, subst),
				CaseName:    arm.CaseName,
				CaseNamePos: arm.CaseNamePos,
				BindName:    arm.BindName,
				BindNamePos: arm.BindNamePos,
				BindIgnore:  arm.BindIgnore,
				Body:        m.rewriteBlock(arm.Body, subst),
			})
		}
		return &ast.MatchStmt{MatchPos: s.MatchPos, Value: m.rewriteExpr(s.Value, subst), Arms: arms}
	case *ast.ExprStmt:
		return &ast.ExprStmt{Expr: m.rewriteExpr(s.Expr, subst)}
	default:
		return stmt
	}
}

func (m *genericMonomorphizer) rewriteExpr(expr ast.Expression, subst map[string]ast.TypeRef) ast.Expression {
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
		return &ast.GroupExpr{Inner: m.rewriteExpr(e.Inner, subst)}
	case *ast.FunctionLiteralExpr:
		params := make([]ast.Param, 0, len(e.Params))
		for _, param := range e.Params {
			params = append(params, ast.Param{
				Name:    param.Name,
				NamePos: param.NamePos,
				Type:    m.rewriteTypeRef(param.Type, subst),
			})
		}
		return &ast.FunctionLiteralExpr{
			FnPos:        e.FnPos,
			Params:       params,
			Return:       m.rewriteTypeRef(e.Return, subst),
			ReturnIsBang: e.ReturnIsBang,
			Body:         m.rewriteBlock(e.Body, subst),
		}
	case *ast.UnaryExpr:
		return &ast.UnaryExpr{Operator: e.Operator, OpPos: e.OpPos, Inner: m.rewriteExpr(e.Inner, subst)}
	case *ast.BinaryExpr:
		return &ast.BinaryExpr{Left: m.rewriteExpr(e.Left, subst), Operator: e.Operator, OpPos: e.OpPos, Right: m.rewriteExpr(e.Right, subst)}
	case *ast.SelectorExpr:
		return &ast.SelectorExpr{Inner: m.rewriteExpr(e.Inner, subst), DotPos: e.DotPos, Name: e.Name, NamePos: e.NamePos}
	case *ast.IndexExpr:
		return &ast.IndexExpr{Inner: m.rewriteExpr(e.Inner, subst), LBracketPos: e.LBracketPos, Index: m.rewriteExpr(e.Index, subst)}
	case *ast.SliceExpr:
		return &ast.SliceExpr{Inner: m.rewriteExpr(e.Inner, subst), LBracketPos: e.LBracketPos, Start: m.rewriteExpr(e.Start, subst), ColonPos: e.ColonPos, End: m.rewriteExpr(e.End, subst)}
	case *ast.CallExpr:
		return m.rewriteCallExpr(e, subst)
	case *ast.TypeApplicationExpr:
		m.diag.Add(e.LBracketPos, "type arguments are only supported on generic function calls")
		return m.rewriteExpr(e.Inner, subst)
	case *ast.StructLiteralExpr:
		fields := make([]ast.StructLiteralField, 0, len(e.Fields))
		for _, field := range e.Fields {
			fields = append(fields, ast.StructLiteralField{
				Name:    field.Name,
				NamePos: field.NamePos,
				Value:   m.rewriteExpr(field.Value, subst),
			})
		}
		return &ast.StructLiteralExpr{Type: m.rewriteTypeRef(e.Type, subst), LBrace: e.LBrace, Fields: fields}
	case *ast.ArrayLiteralExpr:
		elements := make([]ast.Expression, 0, len(e.Elements))
		for _, element := range e.Elements {
			elements = append(elements, m.rewriteExpr(element, subst))
		}
		return &ast.ArrayLiteralExpr{Type: m.rewriteTypeRef(e.Type, subst), LBrace: e.LBrace, Elements: elements}
	case *ast.SliceLiteralExpr:
		elements := make([]ast.Expression, 0, len(e.Elements))
		for _, element := range e.Elements {
			elements = append(elements, m.rewriteExpr(element, subst))
		}
		return &ast.SliceLiteralExpr{Type: m.rewriteTypeRef(e.Type, subst), LBrace: e.LBrace, Elements: elements}
	case *ast.MapLiteralExpr:
		pairs := make([]ast.MapLiteralPair, 0, len(e.Pairs))
		for _, pair := range e.Pairs {
			pairs = append(pairs, ast.MapLiteralPair{
				Key:      m.rewriteExpr(pair.Key, subst),
				KeyPos:   pair.KeyPos,
				Value:    m.rewriteExpr(pair.Value, subst),
				ValuePos: pair.ValuePos,
			})
		}
		return &ast.MapLiteralExpr{Type: m.rewriteTypeRef(e.Type, subst), LBrace: e.LBrace, Pairs: pairs}
	case *ast.PropagateExpr:
		return &ast.PropagateExpr{Inner: m.rewriteExpr(e.Inner, subst), QuestionPos: e.QuestionPos}
	case *ast.HandleExpr:
		return &ast.HandleExpr{Inner: m.rewriteExpr(e.Inner, subst), OrPos: e.OrPos, ErrName: e.ErrName, ErrPos: e.ErrPos, Handler: m.rewriteBlock(e.Handler, subst)}
	default:
		return expr
	}
}

func (m *genericMonomorphizer) rewriteCallExpr(call *ast.CallExpr, subst map[string]ast.TypeRef) ast.Expression {
	args := make([]ast.Expression, 0, len(call.Args))
	for _, arg := range call.Args {
		args = append(args, m.rewriteExpr(arg, subst))
	}

	if applied, ok := call.Callee.(*ast.TypeApplicationExpr); ok {
		ident, ok := applied.Inner.(*ast.IdentExpr)
		if !ok {
			m.diag.Add(applied.LBracketPos, "type arguments are only supported on named functions")
			return &ast.CallExpr{Callee: m.rewriteExpr(applied.Inner, subst), Args: args}
		}
		typeArgs := make([]ast.TypeRef, 0, len(applied.TypeArgs))
		for _, arg := range applied.TypeArgs {
			typeArgs = append(typeArgs, m.rewriteTypeRef(arg, subst))
		}
		if decl := m.genericFunctions[ident.Name]; decl != nil {
			if len(typeArgs) != len(decl.TypeParams) {
				m.diag.Add(applied.LBracketPos, "generic function %q expects %d type arguments, got %d", ident.Name, len(decl.TypeParams), len(typeArgs))
				return &ast.CallExpr{Callee: &ast.IdentExpr{Name: ident.Name, NamePos: ident.NamePos}, Args: args}
			}
			name := m.instantiateFunction(decl, typeArgs)
			return &ast.CallExpr{Callee: &ast.IdentExpr{Name: name, NamePos: ident.NamePos}, Args: args}
		}
		if _, ok := m.nonGenericFunctions[ident.Name]; ok || checker.IsBuiltinFunction(ident.Name) {
			m.diag.Add(applied.LBracketPos, "function %q is not generic", ident.Name)
			return &ast.CallExpr{Callee: &ast.IdentExpr{Name: ident.Name, NamePos: ident.NamePos}, Args: args}
		}
		m.diag.Add(ident.NamePos, "unknown function %q", ident.Name)
		return &ast.CallExpr{Callee: &ast.IdentExpr{Name: ident.Name, NamePos: ident.NamePos}, Args: args}
	}

	if ident, ok := call.Callee.(*ast.IdentExpr); ok {
		if _, ok := m.genericFunctions[ident.Name]; ok {
			m.diag.Add(ident.NamePos, "generic function %q requires explicit type arguments", ident.Name)
		}
	}

	return &ast.CallExpr{Callee: m.rewriteExpr(call.Callee, subst), Args: args}
}

func (m *genericMonomorphizer) rewriteTypeRef(ref ast.TypeRef, subst map[string]ast.TypeRef) ast.TypeRef {
	switch ref.Kind {
	case ast.PointerTypeRef, ast.SliceTypeRef, ast.ArrayTypeRef:
		out := ref
		if ref.Elem != nil {
			elem := m.rewriteTypeRef(*ref.Elem, subst)
			out.Elem = &elem
		}
		return out
	case ast.MapTypeRef:
		out := ref
		if ref.Key != nil {
			key := m.rewriteTypeRef(*ref.Key, subst)
			out.Key = &key
		}
		if ref.Value != nil {
			value := m.rewriteTypeRef(*ref.Value, subst)
			out.Value = &value
		}
		return out
	case ast.FunctionTypeRef:
		out := ref
		if len(ref.Params) > 0 {
			out.Params = make([]ast.TypeRef, 0, len(ref.Params))
			for _, param := range ref.Params {
				out.Params = append(out.Params, m.rewriteTypeRef(param, subst))
			}
		}
		if ref.Return != nil {
			ret := m.rewriteTypeRef(*ref.Return, subst)
			out.Return = &ret
		}
		return out
	}

	if replacement, ok := subst[ref.Name]; ok {
		if len(ref.TypeArgs) > 0 {
			m.diag.Add(ref.Pos, "type parameter %q cannot take type arguments", ref.Name)
		}
		return cloneTypeRef(replacement)
	}

	if len(ref.TypeArgs) == 0 {
		if _, ok := m.genericStructs[ref.Name]; ok {
			m.diag.Add(ref.Pos, "generic type %q requires explicit type arguments", ref.Name)
		}
		return ast.TypeRef{Name: ref.Name, Pos: ref.Pos}
	}

	typeArgs := make([]ast.TypeRef, 0, len(ref.TypeArgs))
	for _, arg := range ref.TypeArgs {
		typeArgs = append(typeArgs, m.rewriteTypeRef(arg, subst))
	}

	if decl := m.genericStructs[ref.Name]; decl != nil {
		if len(typeArgs) != len(decl.TypeParams) {
			m.diag.Add(ref.Pos, "generic type %q expects %d type arguments, got %d", ref.Name, len(decl.TypeParams), len(typeArgs))
			return ast.TypeRef{Name: ref.Name, Pos: ref.Pos}
		}
		return ast.TypeRef{Name: m.instantiateStruct(decl, typeArgs), Pos: ref.Pos}
	}

	if checker.IsBuiltinFunction(ref.Name) || isBuiltinType(ref.Name) {
		m.diag.Add(ref.Pos, "type %q is not generic", ref.Name)
		return ast.TypeRef{Name: ref.Name, Pos: ref.Pos}
	}
	if _, ok := m.nonGenericNamedTypes[ref.Name]; ok {
		m.diag.Add(ref.Pos, "type %q is not generic", ref.Name)
		return ast.TypeRef{Name: ref.Name, Pos: ref.Pos}
	}
	m.diag.Add(ref.Pos, "unknown type %q", ref.String())
	return ast.TypeRef{Name: ref.Name, Pos: ref.Pos}
}

func (m *genericMonomorphizer) instantiateStruct(decl *ast.StructDecl, typeArgs []ast.TypeRef) string {
	name := instantiatedName(decl.Name, typeArgs)
	if m.structInstantiating[name] {
		return name
	}
	for _, existing := range m.output.Structs {
		if existing.Name == name {
			return name
		}
	}

	m.structInstantiating[name] = true
	subst := makeTypeSubstitution(decl.TypeParams, typeArgs)
	fields := make([]ast.StructField, 0, len(decl.Fields))
	for _, field := range decl.Fields {
		fields = append(fields, ast.StructField{
			Name:    field.Name,
			NamePos: field.NamePos,
			Type:    m.rewriteTypeRef(field.Type, subst),
		})
	}
	m.output.Structs = append(m.output.Structs, &ast.StructDecl{
		StructPos: decl.StructPos,
		Exported:  decl.Exported,
		Name:      name,
		NamePos:   decl.NamePos,
		Fields:    fields,
	})
	delete(m.structInstantiating, name)
	m.nonGenericNamedTypes[name] = struct{}{}
	return name
}

func (m *genericMonomorphizer) instantiateFunction(decl *ast.FunctionDecl, typeArgs []ast.TypeRef) string {
	name := instantiatedName(decl.Name, typeArgs)
	if m.functionInstantiating[name] {
		return name
	}
	for _, existing := range m.output.Functions {
		if existing.Name == name {
			return name
		}
	}

	m.functionInstantiating[name] = true
	subst := makeTypeSubstitution(decl.TypeParams, typeArgs)
	var receiver *ast.ReceiverDecl
	if decl.Receiver != nil {
		receiver = &ast.ReceiverDecl{
			Name:    decl.Receiver.Name,
			NamePos: decl.Receiver.NamePos,
			Type:    m.rewriteTypeRef(decl.Receiver.Type, subst),
		}
	}
	params := make([]ast.Param, 0, len(decl.Params))
	for _, param := range decl.Params {
		params = append(params, ast.Param{
			Name:    param.Name,
			NamePos: param.NamePos,
			Type:    m.rewriteTypeRef(param.Type, subst),
		})
	}
	m.output.Functions = append(m.output.Functions, &ast.FunctionDecl{
		Exported:     decl.Exported,
		Name:         name,
		NamePos:      decl.NamePos,
		Receiver:     receiver,
		Params:       params,
		Return:       m.rewriteTypeRef(decl.Return, subst),
		ReturnIsBang: decl.ReturnIsBang,
		Body:         m.rewriteBlock(decl.Body, subst),
	})
	delete(m.functionInstantiating, name)
	m.nonGenericFunctions[name] = struct{}{}
	return name
}

func makeTypeSubstitution(params []ast.TypeParam, args []ast.TypeRef) map[string]ast.TypeRef {
	if len(params) == 0 {
		return nil
	}
	out := make(map[string]ast.TypeRef, len(params))
	for i, param := range params {
		out[param.Name] = cloneTypeRef(args[i])
	}
	return out
}

func cloneTypeRef(ref ast.TypeRef) ast.TypeRef {
	out := ref
	if ref.Elem != nil {
		elem := cloneTypeRef(*ref.Elem)
		out.Elem = &elem
	}
	if ref.Key != nil {
		key := cloneTypeRef(*ref.Key)
		out.Key = &key
	}
	if ref.Value != nil {
		value := cloneTypeRef(*ref.Value)
		out.Value = &value
	}
	if len(ref.Params) > 0 {
		out.Params = make([]ast.TypeRef, 0, len(ref.Params))
		for _, param := range ref.Params {
			out.Params = append(out.Params, cloneTypeRef(param))
		}
	}
	if ref.Return != nil {
		ret := cloneTypeRef(*ref.Return)
		out.Return = &ret
	}
	if len(ref.TypeArgs) > 0 {
		out.TypeArgs = make([]ast.TypeRef, 0, len(ref.TypeArgs))
		for _, arg := range ref.TypeArgs {
			out.TypeArgs = append(out.TypeArgs, cloneTypeRef(arg))
		}
	}
	return out
}

func instantiatedName(base string, args []ast.TypeRef) string {
	parts := make([]string, 0, len(args))
	for _, arg := range args {
		parts = append(parts, arg.String())
	}
	return base + "[" + strings.Join(parts, ",") + "]"
}
