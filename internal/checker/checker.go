package checker

import (
	"fmt"
	"sort"
	"strconv"
	"strings"

	"yar/internal/ast"
	"yar/internal/diag"
	"yar/internal/token"
)

type Type string

const (
	TypeInvalid    Type = ""
	TypeVoid       Type = "void"
	TypeNoReturn   Type = "noreturn"
	TypeBool       Type = "bool"
	TypeI32        Type = "i32"
	TypeI64        Type = "i64"
	TypeStr        Type = "str"
	TypeError      Type = "error"
	TypeNil        Type = "nil"
	TypeUntypedInt Type = "untyped-int"
)

type ExprType struct {
	Base      Type
	Errorable bool
}

type Signature struct {
	Name      string
	Package   string
	FullName  string
	Params    []Type
	Return    Type
	Errorable bool
	Builtin   bool
	Exported  bool
}

type StructField struct {
	Name string
	Type Type
}

type StructInfo struct {
	Name     string
	Package  string
	FullName string
	Exported bool
	Fields   []StructField
}

func (s StructInfo) Field(name string) (StructField, int, bool) {
	for i, field := range s.Fields {
		if field.Name == name {
			return field, i, true
		}
	}
	return StructField{}, -1, false
}

type EnumCaseInfo struct {
	Name        string
	Tag         int
	PayloadType Type
	Fields      []StructField
}

func (c EnumCaseInfo) Field(name string) (StructField, int, bool) {
	for i, field := range c.Fields {
		if field.Name == name {
			return field, i, true
		}
	}
	return StructField{}, -1, false
}

type EnumInfo struct {
	Name     string
	Package  string
	FullName string
	Exported bool
	Cases    []EnumCaseInfo
}

func (e EnumInfo) Case(name string) (EnumCaseInfo, int, bool) {
	for i, enumCase := range e.Cases {
		if enumCase.Name == name {
			return enumCase, i, true
		}
	}
	return EnumCaseInfo{}, -1, false
}

type Info struct {
	Functions     map[*ast.FunctionDecl]Signature
	Calls         map[*ast.CallExpr]Signature
	ExprTypes     map[ast.Expression]ExprType
	Locals        map[ast.Node]Type
	Structs       map[string]StructInfo
	Enums         map[string]EnumInfo
	ErrorCodes    map[string]int
	OrderedErrors []string
}

type Checker struct {
	diag      diag.List
	functions map[string]Signature
	structs   map[string]*ast.StructDecl
	enums     map[string]*ast.EnumDecl
	info      Info
	current   *functionContext
}

type functionContext struct {
	signature Signature
	scopes    []map[string]Type
	loopDepth int
}

type coercedIntegers struct {
	Left   ExprType
	Right  ExprType
	Result Type
}

type ArrayType struct {
	Len  int
	Elem Type
}

type SliceType struct {
	Elem Type
}

type PointerType struct {
	Elem Type
}

func MakeArrayType(length int, elem Type) Type {
	return Type(fmt.Sprintf("[%d]%s", length, elem))
}

func MakeSliceType(elem Type) Type {
	return Type("[]" + string(elem))
}

func MakePointerType(elem Type) Type {
	return Type("*" + string(elem))
}

func ParseArrayType(typ Type) (ArrayType, bool) {
	text := string(typ)
	if !strings.HasPrefix(text, "[") {
		return ArrayType{}, false
	}
	end := strings.IndexByte(text, ']')
	if end < 0 {
		return ArrayType{}, false
	}
	length, err := strconv.Atoi(text[1:end])
	if err != nil {
		return ArrayType{}, false
	}
	elem := Type(text[end+1:])
	if elem == TypeInvalid {
		return ArrayType{}, false
	}
	return ArrayType{Len: length, Elem: elem}, true
}

func ParseSliceType(typ Type) (SliceType, bool) {
	text := string(typ)
	if !strings.HasPrefix(text, "[]") {
		return SliceType{}, false
	}
	elem := Type(text[2:])
	if elem == TypeInvalid {
		return SliceType{}, false
	}
	return SliceType{Elem: elem}, true
}

func ParsePointerType(typ Type) (PointerType, bool) {
	text := string(typ)
	if !strings.HasPrefix(text, "*") {
		return PointerType{}, false
	}
	elem := Type(text[1:])
	if elem == TypeInvalid {
		return PointerType{}, false
	}
	return PointerType{Elem: elem}, true
}

func IsBuiltinFunction(name string) bool {
	switch name {
	case "print", "print_int", "panic", "len", "append":
		return true
	default:
		return false
	}
}

func Check(program *ast.Program) (Info, []diag.Diagnostic) {
	c := &Checker{
		functions: map[string]Signature{
			"print": {
				Name:     "print",
				FullName: "print",
				Params:   []Type{TypeStr},
				Return:   TypeVoid,
				Builtin:  true,
			},
			"print_int": {
				Name:     "print_int",
				FullName: "print_int",
				Params:   []Type{TypeI32},
				Return:   TypeVoid,
				Builtin:  true,
			},
			"panic": {
				Name:     "panic",
				FullName: "panic",
				Params:   []Type{TypeStr},
				Return:   TypeNoReturn,
				Builtin:  true,
			},
			"append": {
				Name:     "append",
				FullName: "append",
				Params:   []Type{TypeInvalid, TypeInvalid},
				Return:   TypeInvalid,
				Builtin:  true,
			},
		},
		structs: make(map[string]*ast.StructDecl),
		enums:   make(map[string]*ast.EnumDecl),
		info: Info{
			Functions:  make(map[*ast.FunctionDecl]Signature),
			Calls:      make(map[*ast.CallExpr]Signature),
			ExprTypes:  make(map[ast.Expression]ExprType),
			Locals:     make(map[ast.Node]Type),
			Structs:    make(map[string]StructInfo),
			Enums:      make(map[string]EnumInfo),
			ErrorCodes: make(map[string]int),
		},
	}

	c.checkProgram(program)
	return c.info, c.diag.Items()
}

func (c *Checker) checkProgram(program *ast.Program) {
	if program == nil {
		c.diag.Add(token.Position{Line: 1, Column: 1}, "missing program")
		return
	}
	if program.PackageName != "main" {
		c.diag.Add(program.Pos(), "package must be main")
	}

	for _, decl := range program.Structs {
		if _, exists := c.structs[decl.Name]; exists {
			c.diag.Add(decl.NamePos, "struct %q is already declared", decl.Name)
			continue
		}
		if _, exists := c.enums[decl.Name]; exists {
			c.diag.Add(decl.NamePos, "type %q is already declared", decl.Name)
			continue
		}
		c.structs[decl.Name] = decl
	}
	for _, decl := range program.Enums {
		if _, exists := c.enums[decl.Name]; exists {
			c.diag.Add(decl.NamePos, "enum %q is already declared", decl.Name)
			continue
		}
		if _, exists := c.structs[decl.Name]; exists {
			c.diag.Add(decl.NamePos, "type %q is already declared", decl.Name)
			continue
		}
		c.enums[decl.Name] = decl
	}

	for _, decl := range program.Structs {
		if _, exists := c.info.Structs[decl.Name]; exists {
			continue
		}
		info := StructInfo{Name: decl.Name}
		seenFields := make(map[string]struct{})
		for _, field := range decl.Fields {
			if _, exists := seenFields[field.Name]; exists {
				c.diag.Add(field.NamePos, "field %q is already declared in struct %q", field.Name, decl.Name)
				continue
			}
			seenFields[field.Name] = struct{}{}

			fieldType := c.resolveTypeRef(field.Type)
			if fieldType == TypeVoid || fieldType == TypeNoReturn {
				c.diag.Add(field.Type.Pos, "field %q cannot use type %q", field.Name, field.Type.Name)
				continue
			}
			info.Fields = append(info.Fields, StructField{
				Name: field.Name,
				Type: fieldType,
			})
		}
		c.info.Structs[decl.Name] = info
	}

	for _, decl := range program.Enums {
		if _, exists := c.info.Enums[decl.Name]; exists {
			continue
		}
		info := EnumInfo{Name: decl.Name}
		seenCases := make(map[string]struct{})
		for i, enumCase := range decl.Cases {
			if _, exists := seenCases[enumCase.Name]; exists {
				c.diag.Add(enumCase.NamePos, "case %q is already declared in enum %q", enumCase.Name, decl.Name)
				continue
			}
			seenCases[enumCase.Name] = struct{}{}

			caseInfo := EnumCaseInfo{
				Name: enumCase.Name,
				Tag:  i,
			}
			if len(enumCase.Fields) > 0 {
				payloadName := enumPayloadTypeName(decl.Name, enumCase.Name)
				payloadInfo := StructInfo{Name: string(payloadName)}
				seenFields := make(map[string]struct{})
				for _, field := range enumCase.Fields {
					if _, exists := seenFields[field.Name]; exists {
						c.diag.Add(field.NamePos, "field %q is already declared in enum case %q", field.Name, enumCase.Name)
						continue
					}
					seenFields[field.Name] = struct{}{}

					fieldType := c.resolveTypeRef(field.Type)
					if fieldType == TypeVoid || fieldType == TypeNoReturn {
						c.diag.Add(field.Type.Pos, "field %q cannot use type %q", field.Name, field.Type.Name)
						continue
					}
					payloadInfo.Fields = append(payloadInfo.Fields, StructField{
						Name: field.Name,
						Type: fieldType,
					})
				}
				c.info.Structs[string(payloadName)] = payloadInfo
				caseInfo.PayloadType = payloadName
				caseInfo.Fields = payloadInfo.Fields
			}
			info.Cases = append(info.Cases, caseInfo)
		}
		c.info.Enums[decl.Name] = info
	}

	c.checkTypeCycles()

	for _, fn := range program.Functions {
		if IsBuiltinFunction(fn.Name) {
			c.diag.Add(fn.NamePos, "function %q is already declared", fn.Name)
			continue
		}
		if _, exists := c.functions[fn.Name]; exists {
			c.diag.Add(fn.NamePos, "function %q is already declared", fn.Name)
			continue
		}
		sig := Signature{
			Name:      fn.Name,
			FullName:  fn.Name,
			Return:    c.resolveTypeRef(fn.Return),
			Errorable: fn.ReturnIsBang,
			Exported:  fn.Exported,
		}
		if sig.Return == TypeNoReturn && sig.Errorable {
			c.diag.Add(fn.Return.Pos, "noreturn functions cannot also be errorable")
		}
		if sig.Return == TypeError && sig.Errorable {
			c.diag.Add(fn.Return.Pos, "error functions cannot also be errorable")
		}
		for _, param := range fn.Params {
			sig.Params = append(sig.Params, c.resolveTypeRef(param.Type))
		}
		c.functions[fn.Name] = sig
		c.info.Functions[fn] = sig
	}

	mainSig, ok := c.functions["main"]
	if !ok || mainSig.Builtin {
		c.diag.Add(program.Pos(), "missing main function")
	} else if mainSig.Return != TypeI32 {
		c.diag.Add(program.Pos(), "main must return i32 or !i32")
	}

	for _, fn := range program.Functions {
		sig, ok := c.info.Functions[fn]
		if !ok {
			continue
		}
		c.checkFunction(fn, sig)
	}

	var ordered []string
	for name := range c.info.ErrorCodes {
		ordered = append(ordered, name)
	}
	sort.Strings(ordered)
	c.info.OrderedErrors = ordered
	for i, name := range ordered {
		c.info.ErrorCodes[name] = i + 1
	}
}

func (c *Checker) checkTypeCycles() {
	visiting := make(map[string]bool)
	visited := make(map[string]bool)

	var visit func(name string)
	visit = func(name string) {
		if visited[name] {
			return
		}
		if visiting[name] {
			if decl, ok := c.structs[name]; ok {
				c.diag.Add(decl.NamePos, "struct %q cannot contain itself recursively", name)
			} else if decl, ok := c.enums[name]; ok {
				c.diag.Add(decl.NamePos, "enum %q cannot contain itself recursively", name)
			}
			return
		}
		visiting[name] = true
		if info, ok := c.info.Structs[name]; ok {
			for _, field := range info.Fields {
				for _, dep := range c.typeDependencies(field.Type) {
					visit(dep)
				}
			}
		}
		if info, ok := c.info.Enums[name]; ok {
			for _, enumCase := range info.Cases {
				for _, field := range enumCase.Fields {
					for _, dep := range c.typeDependencies(field.Type) {
						visit(dep)
					}
				}
			}
		}
		visiting[name] = false
		visited[name] = true
	}

	for name := range c.structs {
		visit(name)
	}
	for name := range c.enums {
		visit(name)
	}
}

func (c *Checker) typeDependencies(typ Type) []string {
	if array, ok := ParseArrayType(typ); ok {
		return c.typeDependencies(array.Elem)
	}
	if _, ok := ParseSliceType(typ); ok {
		return nil
	}
	if _, ok := ParsePointerType(typ); ok {
		return nil
	}
	if _, ok := c.info.Structs[string(typ)]; ok {
		return []string{string(typ)}
	}
	if _, ok := c.info.Enums[string(typ)]; ok {
		return []string{string(typ)}
	}
	return nil
}

func (c *Checker) checkFunction(fn *ast.FunctionDecl, sig Signature) {
	ctx := &functionContext{
		signature: sig,
		scopes:    []map[string]Type{{}},
	}
	c.current = ctx
	defer func() {
		c.current = nil
	}()

	for i, param := range fn.Params {
		paramType := sig.Params[i]
		if paramType == TypeVoid || paramType == TypeNoReturn || paramType == TypeInvalid {
			c.diag.Add(param.Type.Pos, "parameter %q cannot use type %q", param.Name, param.Type.Name)
			continue
		}
		if _, exists := ctx.scopes[0][param.Name]; exists {
			c.diag.Add(param.NamePos, "duplicate parameter %q", param.Name)
			continue
		}
		ctx.scopes[0][param.Name] = paramType
	}

	c.checkBlock(fn.Body)
	if sig.Return == TypeVoid {
		return
	}
	if !c.blockDefinitelyReturns(fn.Body) {
		if sig.Return == TypeNoReturn {
			c.diag.Add(fn.NamePos, "function %q must not fall through", fn.Name)
			return
		}
		c.diag.Add(fn.NamePos, "function %q must return a value on all paths", fn.Name)
	}
}

func (c *Checker) checkBlock(block *ast.BlockStmt) {
	c.pushScope()
	defer c.popScope()
	for _, stmt := range block.Stmts {
		c.checkStatement(stmt)
	}
}

func (c *Checker) checkBlockWithErrorBinding(block *ast.BlockStmt, name string) {
	c.pushScope()
	defer c.popScope()
	c.bindLocal(name, TypeError)
	for _, stmt := range block.Stmts {
		c.checkStatement(stmt)
	}
}

func (c *Checker) checkStatement(stmt ast.Statement) {
	switch s := stmt.(type) {
	case *ast.BlockStmt:
		c.checkBlock(s)
	case *ast.LetStmt:
		value := c.checkExpression(s.Value)
		value = c.requireNonErrorableValue(s.Value, value, "errorable value cannot be bound to a local")
		if value.Base == TypeInvalid {
			return
		}
		if value.Base == TypeNil {
			c.diag.Add(s.Value.Pos(), "cannot infer type from nil without a pointer context")
			return
		}
		if value.Base == TypeUntypedInt {
			defaultType := c.defaultUntypedIntegerType(s.Value)
			if defaultType == TypeInvalid {
				c.diag.Add(s.Value.Pos(), "integer literal does not fit a supported integer type")
				return
			}
			value = c.coerceUntypedInteger(s.Value, value, defaultType)
		}
		if c.scopeOwns(s.Name) {
			c.diag.Add(s.NamePos, "local %q is already declared in this scope", s.Name)
			return
		}
		c.bindLocal(s.Name, value.Base)
		c.info.Locals[s] = value.Base
	case *ast.VarStmt:
		declaredType := c.resolveTypeRef(s.Type)
		if declaredType == TypeVoid || declaredType == TypeNoReturn || declaredType == TypeInvalid {
			c.diag.Add(s.Type.Pos, "local %q cannot use type %q", s.Name, s.Type.Name)
			return
		}
		if c.scopeOwns(s.Name) {
			c.diag.Add(s.NamePos, "local %q is already declared in this scope", s.Name)
			return
		}
		if s.Value != nil {
			value := c.checkExpression(s.Value)
			value = c.requireNonErrorableValue(s.Value, value, "errorable value cannot be bound to a local")
			if value.Base == TypeInvalid {
				return
			}
			value = c.coerceValue(s.Value, value, declaredType)
			if value.Base != declaredType {
				c.diag.Add(s.Value.Pos(), "cannot assign %s to %s", value.Base, declaredType)
				return
			}
		}
		c.bindLocal(s.Name, declaredType)
		c.info.Locals[s] = declaredType
	case *ast.AssignStmt:
		targetType := c.checkAssignmentTarget(s.Target)
		if targetType == TypeInvalid {
			return
		}
		value := c.checkExpression(s.Value)
		value = c.requireNonErrorableValue(s.Value, value, "errorable value cannot be assigned directly")
		if value.Base == TypeInvalid {
			return
		}
		value = c.coerceValue(s.Value, value, targetType)
		if value.Base != targetType {
			c.diag.Add(s.Value.Pos(), "cannot assign %s to %s", value.Base, targetType)
		}
	case *ast.IfStmt:
		c.checkCondition(s.Cond, "if condition must be bool", "if condition cannot be errorable")
		c.checkBlock(s.Then)
		if s.Else != nil {
			c.checkStatement(s.Else)
		}
	case *ast.ForStmt:
		c.checkFor(s)
	case *ast.MatchStmt:
		c.checkMatch(s)
	case *ast.BreakStmt:
		if c.current.loopDepth == 0 {
			c.diag.Add(s.BreakPos, "break can only be used inside a loop")
		}
	case *ast.ContinueStmt:
		if c.current.loopDepth == 0 {
			c.diag.Add(s.ContinuePos, "continue can only be used inside a loop")
		}
	case *ast.ReturnStmt:
		c.checkReturn(s)
	case *ast.ExprStmt:
		exprType := c.checkExpression(s.Expr)
		if exprType.Errorable {
			c.diag.Add(s.Expr.Pos(), "errorable value cannot be used as a statement")
		}
	default:
		c.diag.Add(stmt.Pos(), "unsupported statement")
	}
}

func (c *Checker) checkFor(stmt *ast.ForStmt) {
	c.pushScope()
	defer c.popScope()

	if stmt.Init != nil {
		c.checkForClauseStatement(stmt.Init)
	}
	if stmt.Cond == nil {
		c.diag.Add(stmt.ForPos, "for loop requires a condition")
	} else {
		c.checkCondition(stmt.Cond, "for condition must be bool", "for condition cannot be errorable")
	}
	if stmt.Post != nil {
		c.checkForClauseStatement(stmt.Post)
	}

	c.current.loopDepth++
	c.checkBlock(stmt.Body)
	c.current.loopDepth--
}

func (c *Checker) checkMatch(stmt *ast.MatchStmt) {
	value := c.checkExpression(stmt.Value)
	if value.Errorable {
		c.diag.Add(stmt.Value.Pos(), "match value cannot be errorable")
		return
	}

	enumInfo, ok := c.info.Enums[string(value.Base)]
	if !ok {
		c.diag.Add(stmt.Value.Pos(), "match requires an enum value")
		return
	}

	seen := make(map[string]struct{})
	for _, arm := range stmt.Arms {
		armEnum := c.resolveTypeRef(arm.EnumType)
		if armEnum != value.Base {
			c.diag.Add(arm.EnumType.Pos, "match arm must use enum %q", enumInfo.Name)
		}

		enumCase, _, ok := enumInfo.Case(arm.CaseName)
		if !ok {
			c.diag.Add(arm.CaseNamePos, "enum %q has no case %q", enumInfo.Name, arm.CaseName)
			c.checkBlock(arm.Body)
			continue
		}
		if _, exists := seen[enumCase.Name]; exists {
			c.diag.Add(arm.CaseNamePos, "duplicate match arm for %q", enumCase.Name)
		}
		seen[enumCase.Name] = struct{}{}

		switch {
		case len(enumCase.Fields) == 0 && (arm.BindName != "" || arm.BindIgnore):
			c.diag.Add(arm.BindNamePos, "plain enum case %q cannot bind a payload", enumCase.Name)
			c.checkBlock(arm.Body)
		case len(enumCase.Fields) > 0 && arm.BindName != "" && !arm.BindIgnore:
			c.pushScope()
			c.bindLocal(arm.BindName, enumCase.PayloadType)
			for _, nested := range arm.Body.Stmts {
				c.checkStatement(nested)
			}
			c.popScope()
		default:
			c.checkBlock(arm.Body)
		}
	}

	if len(seen) == len(enumInfo.Cases) {
		return
	}
	missing := make([]string, 0, len(enumInfo.Cases)-len(seen))
	for _, enumCase := range enumInfo.Cases {
		if _, ok := seen[enumCase.Name]; ok {
			continue
		}
		missing = append(missing, enumInfo.Name+"."+enumCase.Name)
	}
	c.diag.Add(stmt.MatchPos, "match on %q is not exhaustive; missing %s", enumInfo.Name, strings.Join(missing, ", "))
}

func (c *Checker) checkForClauseStatement(stmt ast.Statement) {
	switch stmt.(type) {
	case *ast.LetStmt, *ast.VarStmt, *ast.AssignStmt, *ast.ExprStmt:
		c.checkStatement(stmt)
	default:
		c.diag.Add(stmt.Pos(), "for clause must be a declaration, assignment, or expression")
	}
}

func (c *Checker) checkCondition(expr ast.Expression, typeMessage, errorMessage string) {
	cond := c.checkExpression(expr)
	if cond.Errorable {
		c.diag.Add(expr.Pos(), "%s", errorMessage)
	}
	if cond.Base != TypeBool {
		c.diag.Add(expr.Pos(), "%s", typeMessage)
	}
}

func (c *Checker) checkAssignmentTarget(target ast.Expression) Type {
	typ, ok := c.checkAddressableExpr(target, false)
	if ok {
		return typ
	}
	c.diag.Add(target.Pos(), "invalid assignment target")
	return TypeInvalid
}

func (c *Checker) checkReturn(stmt *ast.ReturnStmt) {
	sig := c.current.signature
	if sig.Return == TypeNoReturn {
		c.diag.Add(stmt.ReturnPos, "noreturn functions cannot return")
		return
	}
	if stmt.Value == nil {
		if sig.Errorable && sig.Return == TypeVoid {
			return
		}
		if sig.Return == TypeVoid && !sig.Errorable {
			return
		}
		c.diag.Add(stmt.ReturnPos, "return value is required")
		return
	}

	if errLit, ok := stmt.Value.(*ast.ErrorLiteral); ok {
		if !sig.Errorable && sig.Return != TypeError {
			c.diag.Add(errLit.Pos(), "cannot return %s from function returning %s", formatErrorName(errLit.Name), sig.Return)
			return
		}
		c.useErrorName(errLit.Name)
		c.info.ExprTypes[stmt.Value] = ExprType{Base: TypeError}
		return
	}

	value := c.checkExpression(stmt.Value)
	if value.Errorable {
		if sig.Errorable && value.Base == sig.Return {
			return
		}
		c.diag.Add(stmt.Value.Pos(), "return cannot use an errorable value directly")
		return
	}
	if value.Base == TypeNoReturn || value.Base == TypeVoid {
		c.diag.Add(stmt.Value.Pos(), "return requires a value")
		return
	}
	value = c.coerceValue(stmt.Value, value, sig.Return)
	if value.Base != sig.Return {
		c.diag.Add(stmt.Value.Pos(), "cannot return %s from function returning %s", value.Base, sig.Return)
	}
}

func (c *Checker) checkExpression(expr ast.Expression) ExprType {
	if expr == nil {
		return ExprType{Base: TypeInvalid}
	}
	switch e := expr.(type) {
	case *ast.IdentExpr:
		typ, ok := c.lookupLocal(e.Name)
		if !ok {
			c.diag.Add(e.NamePos, "unknown local %q", e.Name)
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: typ}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.IntLiteral:
		et := ExprType{Base: TypeUntypedInt}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.StringLiteral:
		et := ExprType{Base: TypeStr}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.BoolLiteral:
		et := ExprType{Base: TypeBool}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.NilLiteral:
		et := ExprType{Base: TypeNil}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.ErrorLiteral:
		c.diag.Add(e.Pos(), "%s is only valid in a return statement", formatErrorName(e.Name))
		return ExprType{Base: TypeInvalid}
	case *ast.GroupExpr:
		et := c.checkExpression(e.Inner)
		c.info.ExprTypes[expr] = et
		return et
	case *ast.UnaryExpr:
		return c.checkUnary(expr, e)
	case *ast.PropagateExpr:
		return c.checkPropagate(expr, e)
	case *ast.HandleExpr:
		return c.checkHandle(expr, e)
	case *ast.BinaryExpr:
		return c.checkBinary(expr, e)
	case *ast.SelectorExpr:
		return c.checkSelector(expr, e)
	case *ast.IndexExpr:
		return c.checkIndex(expr, e)
	case *ast.SliceExpr:
		return c.checkSlice(expr, e)
	case *ast.StructLiteralExpr:
		return c.checkStructLiteral(expr, e)
	case *ast.ArrayLiteralExpr:
		return c.checkArrayLiteral(expr, e)
	case *ast.SliceLiteralExpr:
		return c.checkSliceLiteral(expr, e)
	case *ast.CallExpr:
		return c.checkCall(expr, e)
	default:
		c.diag.Add(expr.Pos(), "unsupported expression")
		return ExprType{Base: TypeInvalid}
	}
}

func (c *Checker) checkUnary(expr ast.Expression, unary *ast.UnaryExpr) ExprType {
	switch unary.Operator {
	case token.Amp:
		typ, ok := c.checkAddressableExpr(unary.Inner, true)
		if !ok {
			c.diag.Add(unary.OpPos, "address-of requires an addressable operand or composite literal")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: MakePointerType(typ)}
		c.info.ExprTypes[expr] = et
		return et
	case token.Star:
		inner := c.checkExpression(unary.Inner)
		if inner.Errorable {
			c.diag.Add(unary.OpPos, "dereference cannot use an errorable operand")
			return ExprType{Base: TypeInvalid}
		}
		pointer, ok := ParsePointerType(inner.Base)
		if !ok {
			c.diag.Add(unary.OpPos, "dereference requires a pointer operand")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: pointer.Elem}
		c.info.ExprTypes[expr] = et
		return et
	case token.Minus:
		inner := c.checkExpression(unary.Inner)
		if inner.Errorable {
			c.diag.Add(unary.OpPos, "unary operators cannot use errorable operands")
			return ExprType{Base: TypeInvalid}
		}
		if !isIntegerType(inner.Base) {
			c.diag.Add(unary.OpPos, "unary - requires an integer operand")
			return ExprType{Base: TypeInvalid}
		}
		c.info.ExprTypes[expr] = inner
		return inner
	case token.Bang:
		inner := c.checkExpression(unary.Inner)
		if inner.Errorable {
			c.diag.Add(unary.OpPos, "unary operators cannot use errorable operands")
			return ExprType{Base: TypeInvalid}
		}
		if inner.Base != TypeBool {
			c.diag.Add(unary.OpPos, "unary ! requires a bool operand")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeBool}
		c.info.ExprTypes[expr] = et
		return et
	default:
		c.diag.Add(unary.OpPos, "unsupported unary operator")
		return ExprType{Base: TypeInvalid}
	}
}

func (c *Checker) checkSelector(expr ast.Expression, selector *ast.SelectorExpr) ExprType {
	if ident, ok := selector.Inner.(*ast.IdentExpr); ok {
		if _, exists := c.lookupLocal(ident.Name); !exists {
			if enumInfo, ok := c.info.Enums[ident.Name]; ok {
				enumCase, _, ok := enumInfo.Case(selector.Name)
				if !ok {
					c.diag.Add(selector.NamePos, "enum %q has no case %q", enumInfo.Name, selector.Name)
					return ExprType{Base: TypeInvalid}
				}
				if len(enumCase.Fields) > 0 {
					c.diag.Add(selector.NamePos, "payload case %q requires a constructor body", selector.Name)
					return ExprType{Base: TypeInvalid}
				}
				et := ExprType{Base: Type(enumInfo.Name)}
				c.info.ExprTypes[expr] = et
				return et
			}
		}
	}

	inner := c.checkExpression(selector.Inner)
	if inner.Errorable {
		c.diag.Add(selector.DotPos, "field access cannot use an errorable value")
		return ExprType{Base: TypeInvalid}
	}

	info, ok := c.info.Structs[string(inner.Base)]
	if !ok {
		c.diag.Add(selector.DotPos, "field access requires a struct value")
		return ExprType{Base: TypeInvalid}
	}

	field, _, ok := info.Field(selector.Name)
	if !ok {
		c.diag.Add(selector.NamePos, "struct %q has no field %q", info.Name, selector.Name)
		return ExprType{Base: TypeInvalid}
	}

	et := ExprType{Base: field.Type}
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) checkIndex(expr ast.Expression, index *ast.IndexExpr) ExprType {
	inner := c.checkExpression(index.Inner)
	if inner.Errorable {
		c.diag.Add(index.LBracketPos, "indexing cannot use an errorable value")
		return ExprType{Base: TypeInvalid}
	}

	elemType, ok := sequenceElementType(inner.Base)
	if !ok {
		c.diag.Add(index.LBracketPos, "indexing requires an array or slice value")
		return ExprType{Base: TypeInvalid}
	}

	indexType := c.checkExpression(index.Index)
	if indexType.Errorable {
		c.diag.Add(index.Index.Pos(), "index expression cannot be errorable")
		return ExprType{Base: TypeInvalid}
	}
	if !isIntegerType(indexType.Base) {
		c.diag.Add(index.Index.Pos(), "index expression must be an integer")
		return ExprType{Base: TypeInvalid}
	}
	if indexType.Base == TypeUntypedInt {
		indexType = c.coerceUntypedInteger(index.Index, indexType, TypeI32)
		if indexType.Base == TypeUntypedInt {
			c.diag.Add(index.Index.Pos(), "index expression must fit in i32")
			return ExprType{Base: TypeInvalid}
		}
	}

	et := ExprType{Base: elemType}
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) checkSlice(expr ast.Expression, slice *ast.SliceExpr) ExprType {
	inner := c.checkExpression(slice.Inner)
	if inner.Errorable {
		c.diag.Add(slice.LBracketPos, "slicing cannot use an errorable value")
		return ExprType{Base: TypeInvalid}
	}

	sliceType, ok := ParseSliceType(inner.Base)
	if !ok {
		c.diag.Add(slice.LBracketPos, "slicing requires a slice value")
		return ExprType{Base: TypeInvalid}
	}

	if !c.checkSliceBound(slice.Start) || !c.checkSliceBound(slice.End) {
		return ExprType{Base: TypeInvalid}
	}

	et := ExprType{Base: MakeSliceType(sliceType.Elem)}
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) checkStructLiteral(expr ast.Expression, lit *ast.StructLiteralExpr) ExprType {
	if enumInfo, enumCase, ok := c.lookupEnumCaseType(lit.Type.Name); ok {
		if len(enumCase.Fields) == 0 {
			c.diag.Add(lit.Type.Pos, "plain enum case %q cannot use a constructor body", enumCase.Name)
			return ExprType{Base: TypeInvalid}
		}

		seen := make(map[string]struct{})
		for _, field := range lit.Fields {
			if _, exists := seen[field.Name]; exists {
				c.diag.Add(field.NamePos, "field %q is already initialized", field.Name)
				continue
			}
			seen[field.Name] = struct{}{}

			fieldInfo, _, ok := enumCase.Field(field.Name)
			if !ok {
				c.diag.Add(field.NamePos, "enum case %q has no field %q", enumCase.Name, field.Name)
				continue
			}
			value := c.checkExpression(field.Value)
			value = c.requireNonErrorableValue(field.Value, value, "errorable value cannot be used in an enum constructor")
			if value.Base == TypeInvalid {
				continue
			}
			value = c.coerceValue(field.Value, value, fieldInfo.Type)
			if value.Base != fieldInfo.Type {
				c.diag.Add(field.Value.Pos(), "cannot assign %s to %s", value.Base, fieldInfo.Type)
			}
		}

		et := ExprType{Base: Type(enumInfo.Name)}
		c.info.ExprTypes[expr] = et
		return et
	}

	typ := c.resolveTypeRef(lit.Type)
	info, ok := c.info.Structs[string(typ)]
	if !ok {
		c.diag.Add(lit.Type.Pos, "struct literal requires a struct type")
		return ExprType{Base: TypeInvalid}
	}

	seen := make(map[string]struct{})
	for _, field := range lit.Fields {
		if _, exists := seen[field.Name]; exists {
			c.diag.Add(field.NamePos, "field %q is already initialized", field.Name)
			continue
		}
		seen[field.Name] = struct{}{}

		fieldInfo, _, ok := info.Field(field.Name)
		if !ok {
			c.diag.Add(field.NamePos, "struct %q has no field %q", info.Name, field.Name)
			continue
		}
		value := c.checkExpression(field.Value)
		value = c.requireNonErrorableValue(field.Value, value, "errorable value cannot be used in a struct literal")
		if value.Base == TypeInvalid {
			continue
		}
		value = c.coerceValue(field.Value, value, fieldInfo.Type)
		if value.Base != fieldInfo.Type {
			c.diag.Add(field.Value.Pos(), "cannot assign %s to %s", value.Base, fieldInfo.Type)
		}
	}

	et := ExprType{Base: typ}
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) checkArrayLiteral(expr ast.Expression, lit *ast.ArrayLiteralExpr) ExprType {
	typ := c.resolveTypeRef(lit.Type)
	array, ok := ParseArrayType(typ)
	if !ok {
		c.diag.Add(lit.Type.Pos, "array literal requires an array type")
		return ExprType{Base: TypeInvalid}
	}
	if len(lit.Elements) > array.Len {
		c.diag.Add(lit.Pos(), "array literal has too many elements for %s", typ)
	}
	for _, element := range lit.Elements {
		value := c.checkExpression(element)
		value = c.requireNonErrorableValue(element, value, "errorable value cannot be used in an array literal")
		if value.Base == TypeInvalid {
			continue
		}
		value = c.coerceValue(element, value, array.Elem)
		if value.Base != array.Elem {
			c.diag.Add(element.Pos(), "cannot assign %s to %s", value.Base, array.Elem)
		}
	}

	et := ExprType{Base: typ}
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) checkSliceLiteral(expr ast.Expression, lit *ast.SliceLiteralExpr) ExprType {
	typ := c.resolveTypeRef(lit.Type)
	sliceType, ok := ParseSliceType(typ)
	if !ok {
		c.diag.Add(lit.Type.Pos, "slice literal requires a slice type")
		return ExprType{Base: TypeInvalid}
	}
	for _, element := range lit.Elements {
		value := c.checkExpression(element)
		value = c.requireNonErrorableValue(element, value, "errorable value cannot be used in a slice literal")
		if value.Base == TypeInvalid {
			continue
		}
		value = c.coerceValue(element, value, sliceType.Elem)
		if value.Base != sliceType.Elem {
			c.diag.Add(element.Pos(), "cannot assign %s to %s", value.Base, sliceType.Elem)
		}
	}

	et := ExprType{Base: typ}
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) checkCall(expr ast.Expression, call *ast.CallExpr) ExprType {
	name, namePos, ok := callName(call.Callee)
	if ok && name == "len" {
		if len(call.Args) != 1 {
			c.diag.Add(namePos, "function %q expects 1 arguments, got %d", name, len(call.Args))
			return ExprType{Base: TypeInvalid}
		}
		argType := c.checkExpression(call.Args[0])
		if argType.Errorable {
			c.diag.Add(call.Args[0].Pos(), "errorable value cannot be passed as an argument")
			return ExprType{Base: TypeInvalid}
		}
		if !isSequenceType(argType.Base) {
			c.diag.Add(call.Args[0].Pos(), "len requires an array or slice argument")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeI32}
		c.info.Calls[call] = Signature{Name: name, FullName: name, Params: []Type{TypeInvalid}, Return: TypeI32, Builtin: true}
		c.info.ExprTypes[expr] = et
		return et
	}
	if ok && name == "append" {
		if len(call.Args) != 2 {
			c.diag.Add(namePos, "function %q expects 2 arguments, got %d", name, len(call.Args))
			return ExprType{Base: TypeInvalid}
		}
		sliceArg := c.checkExpression(call.Args[0])
		if sliceArg.Errorable {
			c.diag.Add(call.Args[0].Pos(), "errorable value cannot be passed as an argument")
			return ExprType{Base: TypeInvalid}
		}
		sliceType, ok := ParseSliceType(sliceArg.Base)
		if !ok {
			c.diag.Add(call.Args[0].Pos(), "append requires a slice as its first argument")
			return ExprType{Base: TypeInvalid}
		}
		valueArg := c.checkExpression(call.Args[1])
		if valueArg.Errorable {
			c.diag.Add(call.Args[1].Pos(), "errorable value cannot be passed as an argument")
			return ExprType{Base: TypeInvalid}
		}
		if valueArg.Base == TypeNoReturn || valueArg.Base == TypeVoid {
			c.diag.Add(call.Args[1].Pos(), "argument 2 to %q requires a value", name)
			return ExprType{Base: TypeInvalid}
		}
		valueArg = c.coerceValue(call.Args[1], valueArg, sliceType.Elem)
		if valueArg.Base != sliceType.Elem {
			c.diag.Add(call.Args[1].Pos(), "argument 2 to %q must be %s, got %s", name, sliceType.Elem, valueArg.Base)
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: sliceArg.Base}
		c.info.Calls[call] = Signature{Name: name, FullName: name, Params: []Type{sliceArg.Base, sliceType.Elem}, Return: sliceArg.Base, Builtin: true}
		c.info.ExprTypes[expr] = et
		return et
	}

	if !ok {
		c.diag.Add(call.Pos(), "call target must be a function name")
		return ExprType{Base: TypeInvalid}
	}

	sig, ok := c.functions[name]
	if !ok {
		c.diag.Add(namePos, "unknown function %q", name)
		return ExprType{Base: TypeInvalid}
	}
	if len(call.Args) != len(sig.Params) {
		c.diag.Add(namePos, "function %q expects %d arguments, got %d", name, len(sig.Params), len(call.Args))
	}
	for i, arg := range call.Args {
		argType := c.checkExpression(arg)
		if argType.Errorable {
			c.diag.Add(arg.Pos(), "errorable value cannot be passed as an argument")
			continue
		}
		if argType.Base == TypeNoReturn || argType.Base == TypeVoid {
			c.diag.Add(arg.Pos(), "argument %d to %q requires a value", i+1, name)
			continue
		}
		if i >= len(sig.Params) {
			continue
		}
		argType = c.coerceValue(arg, argType, sig.Params[i])
		if argType.Base != sig.Params[i] {
			c.diag.Add(arg.Pos(), "argument %d to %q must be %s, got %s", i+1, name, sig.Params[i], argType.Base)
		}
	}
	et := ExprType{Base: sig.Return, Errorable: sig.Errorable}
	c.info.Calls[call] = sig
	c.info.ExprTypes[expr] = et
	return et
}

func callName(expr ast.Expression) (string, token.Position, bool) {
	ident, ok := expr.(*ast.IdentExpr)
	if !ok {
		return "", token.Position{}, false
	}
	return ident.Name, ident.NamePos, true
}

func (c *Checker) checkBinary(expr ast.Expression, binary *ast.BinaryExpr) ExprType {
	left := c.checkExpression(binary.Left)
	right := c.checkExpression(binary.Right)
	if left.Errorable || right.Errorable {
		c.diag.Add(binary.OpPos, "binary operators cannot use errorable operands")
		return ExprType{Base: TypeInvalid}
	}

	switch binary.Operator {
	case token.AmpAmp, token.PipePipe:
		if left.Base != TypeBool || right.Base != TypeBool {
			c.diag.Add(binary.OpPos, "logical operators require bool operands")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeBool}
		c.info.ExprTypes[expr] = et
		return et
	case token.Plus, token.Minus, token.Star, token.Slash, token.Percent:
		coerced, ok := c.coerceBinaryIntegers(binary.Left, left, binary.Right, right)
		if !ok {
			c.diag.Add(binary.OpPos, "arithmetic operators require matching integer operands")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: coerced.Result}
		c.info.ExprTypes[expr] = et
		return et
	case token.Less, token.LessEqual, token.Greater, token.GreaterEqual:
		_, ok := c.coerceBinaryIntegers(binary.Left, left, binary.Right, right)
		if !ok {
			c.diag.Add(binary.OpPos, "relational operators require matching integer operands")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeBool}
		c.info.ExprTypes[expr] = et
		return et
	case token.EqualEqual, token.BangEqual:
		if isIntegerType(left.Base) || isIntegerType(right.Base) {
			_, ok := c.coerceBinaryIntegers(binary.Left, left, binary.Right, right)
			if !ok {
				c.diag.Add(binary.OpPos, "comparison operands must have the same type")
				return ExprType{Base: TypeInvalid}
			}
			et := ExprType{Base: TypeBool}
			c.info.ExprTypes[expr] = et
			return et
		}
		if left.Base == TypeNil && isPointerType(right.Base) {
			left = c.coerceValue(binary.Left, left, right.Base)
		}
		if right.Base == TypeNil && isPointerType(left.Base) {
			right = c.coerceValue(binary.Right, right, left.Base)
		}
		if isPointerType(left.Base) || isPointerType(right.Base) {
			if left.Base != right.Base || !isPointerType(left.Base) {
				c.diag.Add(binary.OpPos, "comparison operands must have the same type")
				return ExprType{Base: TypeInvalid}
			}
			et := ExprType{Base: TypeBool}
			c.info.ExprTypes[expr] = et
			return et
		}
		if left.Base != right.Base {
			c.diag.Add(binary.OpPos, "comparison operands must have the same type")
			return ExprType{Base: TypeInvalid}
		}
		if _, ok := c.info.Enums[string(left.Base)]; ok {
			c.diag.Add(binary.OpPos, "comparison is not supported for enum values in v0.4")
			return ExprType{Base: TypeInvalid}
		}
		if left.Base != TypeBool {
			c.diag.Add(binary.OpPos, "comparison is only supported for bool, integers, and pointers in v0.2")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeBool}
		c.info.ExprTypes[expr] = et
		return et
	default:
		c.diag.Add(binary.OpPos, "unsupported operator")
		return ExprType{Base: TypeInvalid}
	}
}

func (c *Checker) resolveTypeRef(ref ast.TypeRef) Type {
	switch Type(ref.Name) {
	case TypeVoid, TypeNoReturn, TypeBool, TypeI32, TypeI64, TypeStr, TypeError:
		return Type(ref.Name)
	}

	if pointer, ok := ParsePointerType(Type(ref.Name)); ok {
		elemType := c.resolveTypeRef(ast.TypeRef{Name: string(pointer.Elem), Pos: ref.Pos})
		if elemType == TypeVoid || elemType == TypeNoReturn || elemType == TypeInvalid {
			c.diag.Add(ref.Pos, "pointer target type %q is not allowed", pointer.Elem)
			return TypeInvalid
		}
		return MakePointerType(elemType)
	}

	if text, ok := parseSliceTypeName(ref.Name); ok {
		elemType := c.resolveTypeRef(ast.TypeRef{Name: text.Elem, Pos: ref.Pos})
		if elemType == TypeVoid || elemType == TypeNoReturn || elemType == TypeInvalid {
			c.diag.Add(ref.Pos, "slice element type %q is not allowed", text.Elem)
			return TypeInvalid
		}
		return MakeSliceType(elemType)
	}

	if text, ok := parseArrayTypeName(ref.Name); ok {
		if text.Len < 0 || text.Len > 2147483647 {
			c.diag.Add(ref.Pos, "array length %d is out of range", text.Len)
			return TypeInvalid
		}
		elemType := c.resolveTypeRef(ast.TypeRef{Name: text.Elem, Pos: ref.Pos})
		if elemType == TypeVoid || elemType == TypeNoReturn || elemType == TypeInvalid {
			c.diag.Add(ref.Pos, "array element type %q is not allowed", text.Elem)
			return TypeInvalid
		}
		return MakeArrayType(text.Len, elemType)
	}

	if _, ok := c.structs[ref.Name]; ok {
		return Type(ref.Name)
	}
	if _, ok := c.enums[ref.Name]; ok {
		return Type(ref.Name)
	}

	c.diag.Add(ref.Pos, "unknown type %q", ref.Name)
	return TypeInvalid
}

func (c *Checker) pushScope() {
	c.current.scopes = append(c.current.scopes, map[string]Type{})
}

func (c *Checker) popScope() {
	c.current.scopes = c.current.scopes[:len(c.current.scopes)-1]
}

func (c *Checker) bindLocal(name string, typ Type) {
	c.current.scopes[len(c.current.scopes)-1][name] = typ
}

func (c *Checker) scopeOwns(name string) bool {
	_, ok := c.current.scopes[len(c.current.scopes)-1][name]
	return ok
}

func (c *Checker) lookupLocal(name string) (Type, bool) {
	for i := len(c.current.scopes) - 1; i >= 0; i-- {
		if typ, ok := c.current.scopes[i][name]; ok {
			return typ, true
		}
	}
	return TypeInvalid, false
}

func (c *Checker) useErrorName(name string) {
	if _, ok := c.info.ErrorCodes[name]; ok {
		return
	}
	c.info.ErrorCodes[name] = 0
}

func (c *Checker) blockDefinitelyReturns(block *ast.BlockStmt) bool {
	for _, stmt := range block.Stmts {
		if c.stmtDefinitelyReturns(stmt) {
			return true
		}
	}
	return false
}

func (c *Checker) stmtDefinitelyReturns(stmt ast.Statement) bool {
	switch s := stmt.(type) {
	case *ast.ReturnStmt:
		return true
	case *ast.BlockStmt:
		return c.blockDefinitelyReturns(s)
	case *ast.ExprStmt:
		exprType, ok := c.info.ExprTypes[s.Expr]
		return ok && exprType.Base == TypeNoReturn
	case *ast.IfStmt:
		if s.Else == nil {
			return false
		}
		return c.blockDefinitelyReturns(s.Then) && c.stmtDefinitelyReturns(s.Else)
	case *ast.MatchStmt:
		if len(s.Arms) == 0 {
			return false
		}
		for _, arm := range s.Arms {
			if !c.blockDefinitelyReturns(arm.Body) {
				return false
			}
		}
		return true
	default:
		return false
	}
}

func (c *Checker) blockTerminatesControlFlow(block *ast.BlockStmt) bool {
	for _, stmt := range block.Stmts {
		if c.stmtTerminatesControlFlow(stmt) {
			return true
		}
	}
	return false
}

func (c *Checker) stmtTerminatesControlFlow(stmt ast.Statement) bool {
	switch s := stmt.(type) {
	case *ast.ReturnStmt, *ast.BreakStmt, *ast.ContinueStmt:
		return true
	case *ast.BlockStmt:
		return c.blockTerminatesControlFlow(s)
	case *ast.ExprStmt:
		exprType, ok := c.info.ExprTypes[s.Expr]
		return ok && exprType.Base == TypeNoReturn
	case *ast.IfStmt:
		if s.Else == nil {
			return false
		}
		return c.blockTerminatesControlFlow(s.Then) && c.stmtTerminatesControlFlow(s.Else)
	case *ast.MatchStmt:
		if len(s.Arms) == 0 {
			return false
		}
		for _, arm := range s.Arms {
			if !c.blockTerminatesControlFlow(arm.Body) {
				return false
			}
		}
		return true
	default:
		return false
	}
}

func (c *Checker) checkPropagate(expr ast.Expression, propagate *ast.PropagateExpr) ExprType {
	inner := c.checkExpression(propagate.Inner)
	if inner.Errorable {
		if !c.currentCanPropagateError() {
			c.diag.Add(propagate.QuestionPos, "cannot use ? in a function that cannot return an error")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: inner.Base}
		c.info.ExprTypes[expr] = et
		return et
	}
	if inner.Base == TypeError {
		if !c.currentCanPropagateError() {
			c.diag.Add(propagate.QuestionPos, "cannot use ? in a function that cannot return an error")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeVoid}
		c.info.ExprTypes[expr] = et
		return et
	}
	c.diag.Add(propagate.QuestionPos, "? requires an errorable expression or error value")
	return ExprType{Base: TypeInvalid}
}

func (c *Checker) checkHandle(expr ast.Expression, handle *ast.HandleExpr) ExprType {
	inner := c.checkExpression(handle.Inner)
	switch {
	case inner.Errorable && inner.Base != TypeVoid:
		c.checkBlockWithErrorBinding(handle.Handler, handle.ErrName)
		if !c.blockTerminatesControlFlow(handle.Handler) {
			c.diag.Add(handle.OrPos, "or handler for a value result must terminate control flow")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: inner.Base}
		c.info.ExprTypes[expr] = et
		return et
	case inner.Errorable:
		c.checkBlockWithErrorBinding(handle.Handler, handle.ErrName)
		et := ExprType{Base: TypeVoid}
		c.info.ExprTypes[expr] = et
		return et
	case inner.Base == TypeError:
		c.checkBlockWithErrorBinding(handle.Handler, handle.ErrName)
		et := ExprType{Base: TypeVoid}
		c.info.ExprTypes[expr] = et
		return et
	default:
		c.diag.Add(handle.OrPos, "or requires an errorable expression or error value")
		return ExprType{Base: TypeInvalid}
	}
}

func formatErrorName(name string) string {
	return fmt.Sprintf("error.%s", name)
}

func isPointerType(typ Type) bool {
	_, ok := ParsePointerType(typ)
	return ok
}

func sequenceElementType(typ Type) (Type, bool) {
	if array, ok := ParseArrayType(typ); ok {
		return array.Elem, true
	}
	if slice, ok := ParseSliceType(typ); ok {
		return slice.Elem, true
	}
	return TypeInvalid, false
}

func isSequenceType(typ Type) bool {
	_, ok := sequenceElementType(typ)
	return ok
}

func isIntegerType(typ Type) bool {
	return typ == TypeI32 || typ == TypeI64 || typ == TypeUntypedInt
}

func (c *Checker) checkAddressableExpr(expr ast.Expression, allowCompositeLiteral bool) (Type, bool) {
	switch e := expr.(type) {
	case *ast.IdentExpr:
		typ, ok := c.lookupLocal(e.Name)
		if !ok {
			c.diag.Add(e.NamePos, "unknown local %q", e.Name)
			return TypeInvalid, false
		}
		return typ, true
	case *ast.GroupExpr:
		typ, ok := c.checkAddressableExpr(e.Inner, allowCompositeLiteral)
		if ok {
			c.info.ExprTypes[expr] = ExprType{Base: typ}
		}
		return typ, ok
	case *ast.SelectorExpr:
		base, ok := c.checkAddressableExpr(e.Inner, false)
		if !ok {
			return TypeInvalid, false
		}
		info, ok := c.info.Structs[string(base)]
		if !ok {
			c.diag.Add(e.DotPos, "field access requires a struct value")
			return TypeInvalid, false
		}
		field, _, ok := info.Field(e.Name)
		if !ok {
			c.diag.Add(e.NamePos, "struct %q has no field %q", info.Name, e.Name)
			return TypeInvalid, false
		}
		return field.Type, true
	case *ast.IndexExpr:
		base, ok := c.checkAddressableExpr(e.Inner, false)
		if !ok {
			return TypeInvalid, false
		}
		elemType, ok := sequenceElementType(base)
		if !ok {
			c.diag.Add(e.LBracketPos, "indexing requires an array or slice value")
			return TypeInvalid, false
		}
		indexType := c.checkExpression(e.Index)
		if indexType.Errorable {
			c.diag.Add(e.Index.Pos(), "index expression cannot be errorable")
			return TypeInvalid, false
		}
		if !isIntegerType(indexType.Base) {
			c.diag.Add(e.Index.Pos(), "index expression must be an integer")
			return TypeInvalid, false
		}
		if indexType.Base == TypeUntypedInt {
			indexType = c.coerceUntypedInteger(e.Index, indexType, TypeI32)
			if indexType.Base == TypeUntypedInt {
				c.diag.Add(e.Index.Pos(), "index expression must fit in i32")
				return TypeInvalid, false
			}
		}
		return elemType, true
	case *ast.UnaryExpr:
		if e.Operator != token.Star {
			return TypeInvalid, false
		}
		inner := c.checkExpression(e.Inner)
		if inner.Errorable {
			c.diag.Add(e.OpPos, "dereference cannot use an errorable operand")
			return TypeInvalid, false
		}
		pointer, ok := ParsePointerType(inner.Base)
		if !ok {
			c.diag.Add(e.OpPos, "dereference requires a pointer operand")
			return TypeInvalid, false
		}
		return pointer.Elem, true
	case *ast.StructLiteralExpr:
		if !allowCompositeLiteral {
			return TypeInvalid, false
		}
		typ := c.checkStructLiteral(expr, e)
		return typ.Base, typ.Base != TypeInvalid
	case *ast.ArrayLiteralExpr:
		if !allowCompositeLiteral {
			return TypeInvalid, false
		}
		typ := c.checkArrayLiteral(expr, e)
		return typ.Base, typ.Base != TypeInvalid
	case *ast.SliceLiteralExpr:
		if !allowCompositeLiteral {
			return TypeInvalid, false
		}
		typ := c.checkSliceLiteral(expr, e)
		return typ.Base, typ.Base != TypeInvalid
	default:
		return TypeInvalid, false
	}
}

func (c *Checker) checkSliceBound(expr ast.Expression) bool {
	boundType := c.checkExpression(expr)
	if boundType.Errorable {
		c.diag.Add(expr.Pos(), "slice bounds cannot be errorable")
		return false
	}
	if !isIntegerType(boundType.Base) {
		c.diag.Add(expr.Pos(), "slice bounds must be integers")
		return false
	}
	if boundType.Base == TypeUntypedInt {
		boundType = c.coerceUntypedInteger(expr, boundType, TypeI32)
		if boundType.Base == TypeUntypedInt {
			c.diag.Add(expr.Pos(), "slice bounds must fit in i32")
			return false
		}
	}
	return true
}

func (c *Checker) requireNonErrorableValue(expr ast.Expression, exprType ExprType, errorMessage string) ExprType {
	if exprType.Errorable {
		c.diag.Add(expr.Pos(), "%s", errorMessage)
		return ExprType{Base: TypeInvalid}
	}
	if exprType.Base == TypeInvalid || exprType.Base == TypeVoid || exprType.Base == TypeNoReturn {
		c.diag.Add(expr.Pos(), "declaration requires a value")
		return ExprType{Base: TypeInvalid}
	}
	return exprType
}

func (c *Checker) coerceUntypedInteger(expr ast.Expression, exprType ExprType, target Type) ExprType {
	if exprType.Base != TypeUntypedInt {
		return exprType
	}
	if target != TypeI32 && target != TypeI64 {
		return exprType
	}
	if !c.intLiteralFits(expr, target) {
		return exprType
	}
	c.setExprType(expr, ExprType{Base: target})
	return ExprType{Base: target}
}

func (c *Checker) coerceValue(expr ast.Expression, exprType ExprType, target Type) ExprType {
	if exprType.Base == TypeNil {
		if !isPointerType(target) {
			return exprType
		}
		coerced := ExprType{Base: target}
		c.setExprType(expr, coerced)
		return coerced
	}
	return c.coerceUntypedInteger(expr, exprType, target)
}

func (c *Checker) coerceBinaryIntegers(leftExpr ast.Expression, left ExprType, rightExpr ast.Expression, right ExprType) (coercedIntegers, bool) {
	out := coercedIntegers{
		Left:  left,
		Right: right,
	}
	if !isIntegerType(left.Base) || !isIntegerType(right.Base) {
		return out, false
	}
	switch {
	case left.Base == TypeUntypedInt && right.Base == TypeUntypedInt:
		target := TypeI32
		if !c.intLiteralFits(leftExpr, TypeI32) || !c.intLiteralFits(rightExpr, TypeI32) {
			target = TypeI64
		}
		if !c.intLiteralFits(leftExpr, target) || !c.intLiteralFits(rightExpr, target) {
			return out, false
		}
		out.Left = c.coerceUntypedInteger(leftExpr, out.Left, target)
		out.Right = c.coerceUntypedInteger(rightExpr, out.Right, target)
		out.Result = target
		return out, true
	case left.Base == TypeUntypedInt:
		if !c.intLiteralFits(leftExpr, right.Base) {
			return out, false
		}
		out.Left = c.coerceUntypedInteger(leftExpr, out.Left, right.Base)
		out.Result = right.Base
		return out, true
	case right.Base == TypeUntypedInt:
		if !c.intLiteralFits(rightExpr, left.Base) {
			return out, false
		}
		out.Right = c.coerceUntypedInteger(rightExpr, out.Right, left.Base)
		out.Result = left.Base
		return out, true
	case left.Base == right.Base:
		out.Result = left.Base
		return out, true
	default:
		return out, false
	}
}

func (c *Checker) setExprType(expr ast.Expression, exprType ExprType) {
	c.info.ExprTypes[expr] = exprType
	if group, ok := expr.(*ast.GroupExpr); ok {
		c.setExprType(group.Inner, exprType)
	}
}

func (c *Checker) intLiteralFits(expr ast.Expression, target Type) bool {
	literal, ok := unwrapIntLiteral(expr)
	if !ok {
		return false
	}
	switch target {
	case TypeI32:
		return literal.Value >= -2147483648 && literal.Value <= 2147483647
	case TypeI64:
		return true
	default:
		return false
	}
}

func unwrapIntLiteral(expr ast.Expression) (*ast.IntLiteral, bool) {
	switch e := expr.(type) {
	case *ast.IntLiteral:
		return e, true
	case *ast.GroupExpr:
		return unwrapIntLiteral(e.Inner)
	case *ast.UnaryExpr:
		if e.Operator != token.Minus {
			return nil, false
		}
		inner, ok := unwrapIntLiteral(e.Inner)
		if !ok {
			return nil, false
		}
		return &ast.IntLiteral{Value: -inner.Value, LitPos: e.OpPos}, true
	default:
		return nil, false
	}
}

func (c *Checker) defaultUntypedIntegerType(expr ast.Expression) Type {
	switch {
	case c.intLiteralFits(expr, TypeI32):
		return TypeI32
	case c.intLiteralFits(expr, TypeI64):
		return TypeI64
	default:
		return TypeInvalid
	}
}

func (c *Checker) currentCanPropagateError() bool {
	if c.current == nil {
		return false
	}
	return c.current.signature.Errorable || c.current.signature.Return == TypeError
}

func enumPayloadTypeName(enumName, caseName string) Type {
	return Type(enumName + "." + caseName)
}

func (c *Checker) lookupEnumCaseType(name string) (EnumInfo, EnumCaseInfo, bool) {
	idx := strings.LastIndex(name, ".")
	if idx <= 0 {
		return EnumInfo{}, EnumCaseInfo{}, false
	}
	enumName := name[:idx]
	caseName := name[idx+1:]
	enumInfo, ok := c.info.Enums[enumName]
	if !ok {
		return EnumInfo{}, EnumCaseInfo{}, false
	}
	enumCase, _, ok := enumInfo.Case(caseName)
	if !ok {
		return EnumInfo{}, EnumCaseInfo{}, false
	}
	return enumInfo, enumCase, true
}

type parsedArrayType struct {
	Len  int
	Elem string
}

type parsedSliceType struct {
	Elem string
}

func parseArrayTypeName(name string) (parsedArrayType, bool) {
	if !strings.HasPrefix(name, "[") {
		return parsedArrayType{}, false
	}
	end := strings.IndexByte(name, ']')
	if end < 0 {
		return parsedArrayType{}, false
	}
	length, err := strconv.Atoi(name[1:end])
	if err != nil {
		return parsedArrayType{}, false
	}
	elem := name[end+1:]
	if elem == "" {
		return parsedArrayType{}, false
	}
	return parsedArrayType{Len: length, Elem: elem}, true
}

func parseSliceTypeName(name string) (parsedSliceType, bool) {
	if !strings.HasPrefix(name, "[]") {
		return parsedSliceType{}, false
	}
	elem := name[2:]
	if elem == "" {
		return parsedSliceType{}, false
	}
	return parsedSliceType{Elem: elem}, true
}
