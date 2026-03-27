package checker

import (
	"fmt"
	"sort"
	"yar/internal/ast"
	"yar/internal/diag"
	"yar/internal/token"
)

type Type string

const (
	TypeInvalid Type = ""
	TypeVoid    Type = "void"
	TypeBool    Type = "bool"
	TypeI32     Type = "i32"
	TypeStr     Type = "str"
	TypeError   Type = "error"
)

type ExprType struct {
	Base      Type
	Errorable bool
}

type Signature struct {
	Name      string
	Params    []Type
	Return    Type
	Errorable bool
	Builtin   bool
}

type Info struct {
	Functions     map[*ast.FunctionDecl]Signature
	ExprTypes     map[ast.Expression]ExprType
	Locals        map[ast.Node]Type
	ErrorCodes    map[string]int
	OrderedErrors []string
}

type Checker struct {
	diag      diag.List
	functions map[string]Signature
	info      Info
	current   *functionContext
}

type functionContext struct {
	signature Signature
	scopes    []map[string]Type
}

func Check(program *ast.Program) (Info, []diag.Diagnostic) {
	c := &Checker{
		functions: map[string]Signature{
			"print": {
				Name:    "print",
				Params:  []Type{TypeStr},
				Return:  TypeVoid,
				Builtin: true,
			},
			"print_int": {
				Name:    "print_int",
				Params:  []Type{TypeI32},
				Return:  TypeVoid,
				Builtin: true,
			},
		},
		info: Info{
			Functions:  make(map[*ast.FunctionDecl]Signature),
			ExprTypes:  make(map[ast.Expression]ExprType),
			Locals:     make(map[ast.Node]Type),
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

	for _, fn := range program.Functions {
		if _, exists := c.functions[fn.Name]; exists {
			c.diag.Add(fn.NamePos, "function %q is already declared", fn.Name)
			continue
		}
		sig := Signature{
			Name:      fn.Name,
			Return:    c.resolveTypeRef(fn.Return),
			Errorable: fn.ReturnIsBang,
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
		if paramType == TypeVoid || paramType == TypeInvalid {
			c.diag.Add(param.Type.Pos, "parameter %q cannot use type %q", param.Name, param.Type.Name)
			continue
		}
		if _, exists := ctx.scopes[0][param.Name]; exists {
			c.diag.Add(param.NamePos, "duplicate parameter %q", param.Name)
			continue
		}
		ctx.scopes[0][param.Name] = paramType
		c.info.Locals[fn.Body] = TypeVoid
	}

	c.checkBlock(fn.Body)
	if sig.Return != TypeVoid && !blockDefinitelyReturns(fn.Body) {
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

func (c *Checker) checkStatement(stmt ast.Statement) {
	switch s := stmt.(type) {
	case *ast.BlockStmt:
		c.checkBlock(s)
	case *ast.LetStmt:
		value := c.checkExpression(s.Value)
		if value.Errorable {
			c.diag.Add(s.Value.Pos(), "errorable value must be handled with catch")
			return
		}
		if value.Base == TypeInvalid || value.Base == TypeVoid {
			c.diag.Add(s.Value.Pos(), "let binding requires a value")
			return
		}
		if c.scopeOwns(s.Name) {
			c.diag.Add(s.NamePos, "local %q is already declared in this scope", s.Name)
			return
		}
		c.bindLocal(s.Name, value.Base)
		c.info.Locals[s] = value.Base
	case *ast.AssignStmt:
		targetType, ok := c.lookupLocal(s.Name)
		if !ok {
			c.diag.Add(s.NamePos, "unknown local %q", s.Name)
			return
		}
		value := c.checkExpression(s.Value)
		if value.Errorable {
			c.diag.Add(s.Value.Pos(), "errorable value must be handled with catch")
			return
		}
		if value.Base != targetType {
			c.diag.Add(s.Value.Pos(), "cannot assign %s to %s", value.Base, targetType)
		}
	case *ast.IfStmt:
		cond := c.checkExpression(s.Cond)
		if cond.Errorable {
			c.diag.Add(s.Cond.Pos(), "if condition cannot be errorable")
		}
		if cond.Base != TypeBool {
			c.diag.Add(s.Cond.Pos(), "if condition must be bool")
		}
		c.checkBlock(s.Then)
	case *ast.ReturnStmt:
		c.checkReturn(s)
	case *ast.ExprStmt:
		exprType := c.checkExpression(s.Expr)
		if exprType.Errorable {
			c.diag.Add(s.Expr.Pos(), "errorable value must be handled with catch")
		}
	default:
		c.diag.Add(stmt.Pos(), "unsupported statement")
	}
}

func (c *Checker) checkReturn(stmt *ast.ReturnStmt) {
	sig := c.current.signature
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
		if !sig.Errorable {
			c.diag.Add(errLit.Pos(), "cannot return %s from non-errorable function", formatErrorName(errLit.Name))
			return
		}
		c.useErrorName(errLit.Name)
		c.info.ExprTypes[stmt.Value] = ExprType{Base: TypeError}
		return
	}

	value := c.checkExpression(stmt.Value)
	if value.Errorable {
		c.diag.Add(stmt.Value.Pos(), "return cannot use an unhandled errorable value")
		return
	}
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
		et := ExprType{Base: TypeI32}
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
	case *ast.ErrorLiteral:
		c.diag.Add(e.Pos(), "%s is only valid in a return statement", formatErrorName(e.Name))
		return ExprType{Base: TypeInvalid}
	case *ast.GroupExpr:
		et := c.checkExpression(e.Inner)
		c.info.ExprTypes[expr] = et
		return et
	case *ast.BinaryExpr:
		return c.checkBinary(expr, e)
	case *ast.CallExpr:
		sig, ok := c.functions[e.Name]
		if !ok {
			c.diag.Add(e.NamePos, "unknown function %q", e.Name)
			return ExprType{Base: TypeInvalid}
		}
		if len(e.Args) != len(sig.Params) {
			c.diag.Add(e.NamePos, "function %q expects %d arguments, got %d", e.Name, len(sig.Params), len(e.Args))
		}
		for i, arg := range e.Args {
			argType := c.checkExpression(arg)
			if argType.Errorable {
				c.diag.Add(arg.Pos(), "errorable value must be handled with catch before passing it")
				continue
			}
			if i < len(sig.Params) && argType.Base != sig.Params[i] {
				c.diag.Add(arg.Pos(), "argument %d to %q must be %s, got %s", i+1, e.Name, sig.Params[i], argType.Base)
			}
		}
		et := ExprType{Base: sig.Return, Errorable: sig.Errorable}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.CatchExpr:
		targetType := c.checkExpression(e.Target)
		if !targetType.Errorable {
			c.diag.Add(e.Target.Pos(), "catch can only handle an errorable expression")
			return ExprType{Base: TypeInvalid}
		}
		c.checkBlock(e.Block)
		if !blockDefinitelyReturns(e.Block) {
			c.diag.Add(e.CatchPos, "catch block must return on all paths")
		}
		et := ExprType{Base: targetType.Base}
		c.info.ExprTypes[expr] = et
		return et
	default:
		c.diag.Add(expr.Pos(), "unsupported expression")
		return ExprType{Base: TypeInvalid}
	}
}

func (c *Checker) checkBinary(expr ast.Expression, binary *ast.BinaryExpr) ExprType {
	left := c.checkExpression(binary.Left)
	right := c.checkExpression(binary.Right)
	if left.Errorable || right.Errorable {
		c.diag.Add(binary.OpPos, "binary operators cannot use errorable operands")
		return ExprType{Base: TypeInvalid}
	}

	switch binary.Operator {
	case token.Plus, token.Minus, token.Star, token.Slash:
		if left.Base != TypeI32 || right.Base != TypeI32 {
			c.diag.Add(binary.OpPos, "arithmetic operators require i32 operands")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeI32}
		c.info.ExprTypes[expr] = et
		return et
	case token.EqualEqual, token.BangEqual:
		if left.Base != right.Base {
			c.diag.Add(binary.OpPos, "comparison operands must have the same type")
			return ExprType{Base: TypeInvalid}
		}
		if left.Base != TypeI32 && left.Base != TypeBool {
			c.diag.Add(binary.OpPos, "comparison is only supported for i32 and bool in v0")
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
	case TypeVoid, TypeBool, TypeI32, TypeStr:
		return Type(ref.Name)
	default:
		c.diag.Add(ref.Pos, "unknown type %q", ref.Name)
		return TypeInvalid
	}
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

func blockDefinitelyReturns(block *ast.BlockStmt) bool {
	for _, stmt := range block.Stmts {
		if stmtDefinitelyReturns(stmt) {
			return true
		}
	}
	return false
}

func stmtDefinitelyReturns(stmt ast.Statement) bool {
	switch s := stmt.(type) {
	case *ast.ReturnStmt:
		return true
	case *ast.BlockStmt:
		return blockDefinitelyReturns(s)
	default:
		return false
	}
}

func formatErrorName(name string) string {
	return fmt.Sprintf("error.%s", name)
}
