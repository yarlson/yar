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
	TypeInvalid    Type = ""
	TypeVoid       Type = "void"
	TypeNoReturn   Type = "noreturn"
	TypeBool       Type = "bool"
	TypeI32        Type = "i32"
	TypeI64        Type = "i64"
	TypeStr        Type = "str"
	TypeError      Type = "error"
	TypeUntypedInt Type = "untyped-int"
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

type coercedIntegers struct {
	Left   ExprType
	Right  ExprType
	Result Type
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
			"panic": {
				Name:    "panic",
				Params:  []Type{TypeStr},
				Return:  TypeNoReturn,
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
		if sig.Return == TypeNoReturn && sig.Errorable {
			c.diag.Add(fn.Return.Pos, "noreturn functions cannot also be errorable")
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
		if paramType == TypeVoid || paramType == TypeNoReturn || paramType == TypeInvalid {
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
	if sig.Return == TypeVoid {
		return
	}
	if !c.blockDefinitelyTerminates(fn.Body) {
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
		if value.Base == TypeInvalid || value.Base == TypeVoid || value.Base == TypeNoReturn {
			c.diag.Add(s.Value.Pos(), "let binding requires a value")
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
		if value.Base == TypeNoReturn {
			c.diag.Add(s.Value.Pos(), "assignment requires a value")
			return
		}
		value = c.coerceUntypedInteger(s.Value, value, targetType)
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
	if value.Base == TypeNoReturn {
		c.diag.Add(stmt.Value.Pos(), "return requires a value")
		return
	}
	value = c.coerceUntypedInteger(stmt.Value, value, sig.Return)
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
			if argType.Base == TypeNoReturn {
				c.diag.Add(arg.Pos(), "argument %d to %q requires a value", i+1, e.Name)
				continue
			}
			argType = c.coerceUntypedInteger(arg, argType, sig.Params[i])
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
		if !c.blockDefinitelyTerminates(e.Block) {
			c.diag.Add(e.CatchPos, "catch block must return or terminate on all paths")
		}
		et := ExprType{Base: targetType.Base}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.TryExpr:
		if !c.current.signature.Errorable {
			c.diag.Add(e.TryPos, "try can only be used inside an errorable function")
			return ExprType{Base: TypeInvalid}
		}
		targetType := c.checkExpression(e.Target)
		if !targetType.Errorable {
			c.diag.Add(e.Target.Pos(), "try can only be used with an errorable expression")
			return ExprType{Base: TypeInvalid}
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
		if left.Base != right.Base {
			c.diag.Add(binary.OpPos, "comparison operands must have the same type")
			return ExprType{Base: TypeInvalid}
		}
		if left.Base != TypeBool {
			c.diag.Add(binary.OpPos, "comparison is only supported for bool and integers in v0")
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
	case TypeVoid, TypeNoReturn, TypeBool, TypeI32, TypeI64, TypeStr:
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

func (c *Checker) blockDefinitelyTerminates(block *ast.BlockStmt) bool {
	for _, stmt := range block.Stmts {
		if c.stmtDefinitelyTerminates(stmt) {
			return true
		}
	}
	return false
}

func (c *Checker) stmtDefinitelyTerminates(stmt ast.Statement) bool {
	switch s := stmt.(type) {
	case *ast.ReturnStmt:
		return true
	case *ast.BlockStmt:
		return c.blockDefinitelyTerminates(s)
	case *ast.ExprStmt:
		exprType, ok := c.info.ExprTypes[s.Expr]
		return ok && exprType.Base == TypeNoReturn
	default:
		return false
	}
}

func formatErrorName(name string) string {
	return fmt.Sprintf("error.%s", name)
}

func isIntegerType(typ Type) bool {
	return typ == TypeI32 || typ == TypeI64 || typ == TypeUntypedInt
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
