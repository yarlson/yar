package parser

import (
	"strings"
	"testing"

	"yar/internal/ast"
	"yar/internal/token"
)

func TestParsePropagateExpr(t *testing.T) {
	t.Parallel()

	program, diags := Parse(`
package main

fn divide(a i32, b i32) !i32 {
	return error.DivideByZero
}

fn main() !i32 {
	x := divide(10, 2)?
	return x
}
`)
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	stmt, ok := program.Functions[1].Body.Stmts[0].(*ast.LetStmt)
	if !ok {
		t.Fatalf("expected let statement, got %T", program.Functions[1].Body.Stmts[0])
	}
	if stmt.Name != "x" {
		t.Fatalf("unexpected local name: %q", stmt.Name)
	}
	if _, ok := stmt.Value.(*ast.PropagateExpr); !ok {
		t.Fatalf("expected propagate expression, got %T", stmt.Value)
	}
}

func TestParseGenerics(t *testing.T) {
	t.Parallel()

	program, diags := Parse(`
package main

struct Box[T] {
	value T
}

fn first[T](value T) T {
	return value
}

fn main() i32 {
	box := Box[i32]{value: first[i32](1)}
	return box.value
}
`)
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	if got, want := len(program.Structs[0].TypeParams), 1; got != want {
		t.Fatalf("unexpected struct type parameter count: got %d want %d", got, want)
	}
	if got, want := program.Structs[0].TypeParams[0].Name, "T"; got != want {
		t.Fatalf("unexpected struct type parameter name: got %q want %q", got, want)
	}
	if got, want := len(program.Functions[0].TypeParams), 1; got != want {
		t.Fatalf("unexpected function type parameter count: got %d want %d", got, want)
	}
	if got, want := program.Functions[0].TypeParams[0].Name, "T"; got != want {
		t.Fatalf("unexpected function type parameter name: got %q want %q", got, want)
	}

	stmt, ok := program.Functions[1].Body.Stmts[0].(*ast.LetStmt)
	if !ok {
		t.Fatalf("expected let statement, got %T", program.Functions[1].Body.Stmts[0])
	}
	lit, ok := stmt.Value.(*ast.StructLiteralExpr)
	if !ok {
		t.Fatalf("expected struct literal, got %T", stmt.Value)
	}
	if got, want := lit.Type.Name, "Box"; got != want {
		t.Fatalf("unexpected struct literal type name: got %q want %q", got, want)
	}
	if got, want := len(lit.Type.TypeArgs), 1; got != want {
		t.Fatalf("unexpected struct literal type argument count: got %d want %d", got, want)
	}
	if got, want := lit.Type.TypeArgs[0].String(), "i32"; got != want {
		t.Fatalf("unexpected struct literal type argument: got %q want %q", got, want)
	}

	call, ok := lit.Fields[0].Value.(*ast.CallExpr)
	if !ok {
		t.Fatalf("expected call expression, got %T", lit.Fields[0].Value)
	}
	typeApp, ok := call.Callee.(*ast.TypeApplicationExpr)
	if !ok {
		t.Fatalf("expected type application callee, got %T", call.Callee)
	}
	if got, want := len(typeApp.TypeArgs), 1; got != want {
		t.Fatalf("unexpected call type argument count: got %d want %d", got, want)
	}
	if got, want := typeApp.TypeArgs[0].String(), "i32"; got != want {
		t.Fatalf("unexpected call type argument: got %q want %q", got, want)
	}
}

func TestParseClosures(t *testing.T) {
	t.Parallel()

	program, diags := Parse(`
package main

fn make_adder(x i32) fn(i32) i32 {
	return fn(y i32) i32 {
		return x + y
	}
}

fn main() i32 {
	f := fn(value i32) !i32 {
		return value
	}
	return 0
}
`)
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	if got, want := program.Functions[0].Return.String(), "fn(i32) i32"; got != want {
		t.Fatalf("unexpected function return type: got %q want %q", got, want)
	}

	ret, ok := program.Functions[0].Body.Stmts[0].(*ast.ReturnStmt)
	if !ok {
		t.Fatalf("expected return statement, got %T", program.Functions[0].Body.Stmts[0])
	}
	lit, ok := ret.Value.(*ast.FunctionLiteralExpr)
	if !ok {
		t.Fatalf("expected function literal, got %T", ret.Value)
	}
	if got, want := len(lit.Params), 1; got != want {
		t.Fatalf("unexpected literal parameter count: got %d want %d", got, want)
	}
	if got, want := lit.Params[0].Name, "y"; got != want {
		t.Fatalf("unexpected literal parameter name: got %q want %q", got, want)
	}
	if lit.ReturnIsBang {
		t.Fatal("expected outer returned literal to be non-errorable")
	}

	stmt, ok := program.Functions[1].Body.Stmts[0].(*ast.LetStmt)
	if !ok {
		t.Fatalf("expected let statement, got %T", program.Functions[1].Body.Stmts[0])
	}
	mainLit, ok := stmt.Value.(*ast.FunctionLiteralExpr)
	if !ok {
		t.Fatalf("expected function literal, got %T", stmt.Value)
	}
	if !mainLit.ReturnIsBang {
		t.Fatal("expected bang return on function literal")
	}
	if got, want := mainLit.Return.String(), "i32"; got != want {
		t.Fatalf("unexpected literal return type: got %q want %q", got, want)
	}
}

func TestParseEnumAndMatch(t *testing.T) {
	t.Parallel()

	program, diags := Parse(`
package main

enum TokenKind {
	Ident
	Int
}

enum Expr {
	Int { value i64 }
	Name { text str }
}

fn main() i32 {
	expr := Expr.Name{text: "main"}
	match expr {
	case Expr.Int(_) {
		return 1
	}
	case Expr.Name(v) {
		print(v.text)
		return 0
	}
	}
}
`)
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	if got, want := len(program.Enums), 2; got != want {
		t.Fatalf("unexpected enum count: got %d want %d", got, want)
	}
	if got, want := program.Enums[0].Cases[1].Name, "Int"; got != want {
		t.Fatalf("unexpected enum case name: got %q want %q", got, want)
	}
	if got, want := program.Enums[1].Cases[0].Fields[0].Type.String(), "i64"; got != want {
		t.Fatalf("unexpected payload field type: got %q want %q", got, want)
	}

	stmt, ok := program.Functions[0].Body.Stmts[1].(*ast.MatchStmt)
	if !ok {
		t.Fatalf("expected match statement, got %T", program.Functions[0].Body.Stmts[1])
	}
	if got, want := stmt.Arms[0].EnumType.String(), "Expr"; got != want {
		t.Fatalf("unexpected arm enum type: got %q want %q", got, want)
	}
	if got, want := stmt.Arms[0].CaseName, "Int"; got != want {
		t.Fatalf("unexpected arm case name: got %q want %q", got, want)
	}
	if !stmt.Arms[0].BindIgnore {
		t.Fatal("expected first arm to ignore payload binding")
	}
	if got, want := stmt.Arms[1].BindName, "v"; got != want {
		t.Fatalf("unexpected payload binding: got %q want %q", got, want)
	}
}

func TestParseHandleExpr(t *testing.T) {
	t.Parallel()

	program, diags := Parse(`
package main

fn divide(a i32, b i32) !i32 {
	return error.DivideByZero
}

fn log_error(err error) void {
	return
}

fn main() i32 {
	x := divide(10, 2) or |err| {
		log_error(err)
		return 0
	}
	return x
}
`)
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	stmt, ok := program.Functions[2].Body.Stmts[0].(*ast.LetStmt)
	if !ok {
		t.Fatalf("expected let statement, got %T", program.Functions[2].Body.Stmts[0])
	}
	handle, ok := stmt.Value.(*ast.HandleExpr)
	if !ok {
		t.Fatalf("expected handle expression, got %T", stmt.Value)
	}
	if handle.ErrName != "err" {
		t.Fatalf("unexpected handler binding: %q", handle.ErrName)
	}
	if len(handle.Handler.Stmts) != 2 {
		t.Fatalf("unexpected handler statement count: %d", len(handle.Handler.Stmts))
	}
}

func TestParseSliceForms(t *testing.T) {
	t.Parallel()

	program, diags := Parse(`
package main

fn main() i32 {
	values := []i32{1, 2, 3}
	prefix := values[0:2]
	return prefix[1]
}
`)
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	litStmt, ok := program.Functions[0].Body.Stmts[0].(*ast.LetStmt)
	if !ok {
		t.Fatalf("expected let statement, got %T", program.Functions[0].Body.Stmts[0])
	}
	litExpr, ok := litStmt.Value.(*ast.SliceLiteralExpr)
	if !ok {
		t.Fatalf("expected slice literal expression, got %T", litStmt.Value)
	}
	if got, want := litExpr.Type.String(), "[]i32"; got != want {
		t.Fatalf("unexpected slice literal type: got %q want %q", got, want)
	}

	sliceStmt, ok := program.Functions[0].Body.Stmts[1].(*ast.LetStmt)
	if !ok {
		t.Fatalf("expected let statement, got %T", program.Functions[0].Body.Stmts[1])
	}
	sliceExpr, ok := sliceStmt.Value.(*ast.SliceExpr)
	if !ok {
		t.Fatalf("expected slice expression, got %T", sliceStmt.Value)
	}
	if _, ok := sliceExpr.Start.(*ast.IntLiteral); !ok {
		t.Fatalf("expected integer slice start, got %T", sliceExpr.Start)
	}
	if _, ok := sliceExpr.End.(*ast.IntLiteral); !ok {
		t.Fatalf("expected integer slice end, got %T", sliceExpr.End)
	}
}

func TestParsePointerForms(t *testing.T) {
	t.Parallel()

	program, diags := Parse(`
package main

struct Node {
	value i32
	next *Node
}

fn set_value(node *Node, value i32) void {
	(*node).value = value
}

fn main() i32 {
	tail := &Node{value: 2, next: nil}
	return 0
}
`)
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	if got, want := program.Structs[0].Fields[1].Type.String(), "*Node"; got != want {
		t.Fatalf("unexpected pointer field type: got %q want %q", got, want)
	}
	if got, want := program.Functions[0].Params[0].Type.String(), "*Node"; got != want {
		t.Fatalf("unexpected pointer param type: got %q want %q", got, want)
	}

	assign, ok := program.Functions[0].Body.Stmts[0].(*ast.AssignStmt)
	if !ok {
		t.Fatalf("expected assignment statement, got %T", program.Functions[0].Body.Stmts[0])
	}
	target, ok := assign.Target.(*ast.SelectorExpr)
	if !ok {
		t.Fatalf("expected selector assignment target, got %T", assign.Target)
	}
	group, ok := target.Inner.(*ast.GroupExpr)
	if !ok {
		t.Fatalf("expected grouped dereference base, got %T", target.Inner)
	}
	deref, ok := group.Inner.(*ast.UnaryExpr)
	if !ok {
		t.Fatalf("expected unary dereference, got %T", group.Inner)
	}
	if deref.Operator != token.Star {
		t.Fatalf("expected dereference operator, got %s", deref.Operator)
	}

	stmt, ok := program.Functions[1].Body.Stmts[0].(*ast.LetStmt)
	if !ok {
		t.Fatalf("expected let statement, got %T", program.Functions[1].Body.Stmts[0])
	}
	addr, ok := stmt.Value.(*ast.UnaryExpr)
	if !ok {
		t.Fatalf("expected unary address-of expression, got %T", stmt.Value)
	}
	if addr.Operator != token.Amp {
		t.Fatalf("expected address-of operator, got %s", addr.Operator)
	}
	lit, ok := addr.Inner.(*ast.StructLiteralExpr)
	if !ok {
		t.Fatalf("expected struct literal under address-of, got %T", addr.Inner)
	}
	nilValue, ok := lit.Fields[1].Value.(*ast.NilLiteral)
	if !ok {
		t.Fatalf("expected nil literal field value, got %T", lit.Fields[1].Value)
	}
	if nilValue == nil {
		t.Fatal("expected nil literal")
	}
}

func TestParseMethodDeclarations(t *testing.T) {
	t.Parallel()

	program, diags := Parse(`
package main

struct User {
	name str
}

fn (u User) label(prefix str) str {
	return prefix + u.name
}

fn (u *User) rename(name str) void {
	(*u).name = name
}

fn main() i32 {
	user := User{name: "ada"}
	print(user.label("hi "))
	print("\n")
	return 0
}
`)
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	valueMethod := program.Functions[0]
	if valueMethod.Receiver == nil {
		t.Fatal("expected value receiver")
	}
	if got, want := valueMethod.Receiver.Name, "u"; got != want {
		t.Fatalf("unexpected receiver name: got %q want %q", got, want)
	}
	if got, want := valueMethod.Receiver.Type.String(), "User"; got != want {
		t.Fatalf("unexpected receiver type: got %q want %q", got, want)
	}
	if got, want := valueMethod.Params[0].Name, "prefix"; got != want {
		t.Fatalf("unexpected param name: got %q want %q", got, want)
	}

	pointerMethod := program.Functions[1]
	if pointerMethod.Receiver == nil {
		t.Fatal("expected pointer receiver")
	}
	if got, want := pointerMethod.Receiver.Type.String(), "*User"; got != want {
		t.Fatalf("unexpected receiver type: got %q want %q", got, want)
	}

	stmt, ok := program.Functions[2].Body.Stmts[1].(*ast.ExprStmt)
	if !ok {
		t.Fatalf("expected expression statement, got %T", program.Functions[2].Body.Stmts[1])
	}
	call, ok := stmt.Expr.(*ast.CallExpr)
	if !ok {
		t.Fatalf("expected call expression, got %T", stmt.Expr)
	}
	selector, ok := call.Args[0].(*ast.CallExpr)
	if !ok {
		t.Fatalf("expected nested method call argument, got %T", call.Args[0])
	}
	callee, ok := selector.Callee.(*ast.SelectorExpr)
	if !ok {
		t.Fatalf("expected selector callee, got %T", selector.Callee)
	}
	if got, want := callee.Name, "label"; got != want {
		t.Fatalf("unexpected method name: got %q want %q", got, want)
	}
}

func TestParseInterfaceDeclarations(t *testing.T) {
	t.Parallel()

	program, diags := Parse(`
package main

interface Writer {
	write(msg str) !void
}

interface Labeler {
	label(prefix str) str
}

fn main() i32 {
	return 0
}
`)
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	if got, want := len(program.Interfaces), 2; got != want {
		t.Fatalf("unexpected interface count: got %d want %d", got, want)
	}

	writer := program.Interfaces[0]
	if got, want := writer.Name, "Writer"; got != want {
		t.Fatalf("unexpected interface name: got %q want %q", got, want)
	}
	if got, want := len(writer.Methods), 1; got != want {
		t.Fatalf("unexpected method count: got %d want %d", got, want)
	}
	if got, want := writer.Methods[0].Name, "write"; got != want {
		t.Fatalf("unexpected method name: got %q want %q", got, want)
	}
	if got, want := writer.Methods[0].Params[0].Type.String(), "str"; got != want {
		t.Fatalf("unexpected param type: got %q want %q", got, want)
	}
	if !writer.Methods[0].ReturnIsBang {
		t.Fatal("expected errorable interface method")
	}

	labeler := program.Interfaces[1]
	if got, want := labeler.Methods[0].Return.String(), "str"; got != want {
		t.Fatalf("unexpected return type: got %q want %q", got, want)
	}
}

func TestParseForClausePointerAssignment(t *testing.T) {
	t.Parallel()

	program, diags := Parse(`
package main

struct Node {
	value i32
}

fn main() i32 {
	node := &Node{value: 0}
	for (*node).value = 0; (*node).value < 1; (*node).value = (*node).value + 1 {
	}
	return 0
}
`)
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	loop, ok := program.Functions[0].Body.Stmts[1].(*ast.ForStmt)
	if !ok {
		t.Fatalf("expected for statement, got %T", program.Functions[0].Body.Stmts[1])
	}
	if _, ok := loop.Init.(*ast.AssignStmt); !ok {
		t.Fatalf("expected pointer assignment init clause, got %T", loop.Init)
	}
	if _, ok := loop.Post.(*ast.AssignStmt); !ok {
		t.Fatalf("expected pointer assignment post clause, got %T", loop.Post)
	}
}

func TestParseBoolOperatorPrecedence(t *testing.T) {
	t.Parallel()

	program, diags := Parse(`
package main

fn main() i32 {
	x := true || false && false
	return 0
}
`)
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	stmt, ok := program.Functions[0].Body.Stmts[0].(*ast.LetStmt)
	if !ok {
		t.Fatalf("expected let statement, got %T", program.Functions[0].Body.Stmts[0])
	}

	orExpr, ok := stmt.Value.(*ast.BinaryExpr)
	if !ok {
		t.Fatalf("expected binary expression, got %T", stmt.Value)
	}
	if orExpr.Operator != token.PipePipe {
		t.Fatalf("expected || at root, got %s", orExpr.Operator)
	}

	andExpr, ok := orExpr.Right.(*ast.BinaryExpr)
	if !ok {
		t.Fatalf("expected && on right side, got %T", orExpr.Right)
	}
	if andExpr.Operator != token.AmpAmp {
		t.Fatalf("expected && on right side, got %s", andExpr.Operator)
	}
}

func TestParseRejectsLetSyntax(t *testing.T) {
	t.Parallel()

	_, diags := Parse(`
package main

fn main() i32 {
	let x = 1
	return x
}
`)
	if len(diags) == 0 {
		t.Fatal("expected diagnostics")
	}

	messages := make([]string, 0, len(diags))
	for _, diag := range diags {
		messages = append(messages, diag.Message)
	}
	if !strings.Contains(strings.Join(messages, "\n"), "use ':=' for local declarations; 'let' is no longer supported") {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}
}

func TestParseV02ProgramShape(t *testing.T) {
	t.Parallel()

	program, diags := Parse(`
package main

struct User {
	id i32
	name str
}

fn main() i32 {
	var count i32 = 0
	users := [2]User{
		User{id: 1, name: "alice"},
		User{id: 2, name: "bob"},
	}

	for count < len(users) {
		user := users[count]
		if user.id == 2 {
			users[count].name = "eve"
		} else if user.id == 1 {
			count = count + 1
			continue
		} else {
			break
		}
		count = count + 1
	}

	return 0
}
`)
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	if len(program.Structs) != 1 {
		t.Fatalf("expected 1 struct, got %d", len(program.Structs))
	}
	if program.Structs[0].Name != "User" {
		t.Fatalf("unexpected struct name: %q", program.Structs[0].Name)
	}

	mainFn := program.Functions[0]
	loop, ok := mainFn.Body.Stmts[2].(*ast.ForStmt)
	if !ok {
		t.Fatalf("expected for statement, got %T", mainFn.Body.Stmts[2])
	}
	if loop.Init != nil || loop.Post != nil {
		t.Fatalf("expected condition-only loop")
	}

	ifStmt, ok := loop.Body.Stmts[1].(*ast.IfStmt)
	if !ok {
		t.Fatalf("expected if statement, got %T", loop.Body.Stmts[1])
	}
	if ifStmt.Else == nil {
		t.Fatal("expected else branch")
	}
	elseIf, ok := ifStmt.Else.(*ast.IfStmt)
	if !ok {
		t.Fatalf("expected else-if branch, got %T", ifStmt.Else)
	}
	if elseIf.Else == nil {
		t.Fatal("expected trailing else branch")
	}

	assign, ok := ifStmt.Then.Stmts[0].(*ast.AssignStmt)
	if !ok {
		t.Fatalf("expected assignment in then branch, got %T", ifStmt.Then.Stmts[0])
	}
	if _, ok := assign.Target.(*ast.SelectorExpr); !ok {
		t.Fatalf("expected selector assignment target, got %T", assign.Target)
	}
}

func TestParseImportsPubAndQualifiedCall(t *testing.T) {
	t.Parallel()

	program, diags := Parse(`
package main

import "lexer"
import "token"

pub struct User {
	id i32
}

pub fn use_kind(kind token.Kind) i32 {
	return 0
}

fn main() i32 {
	return lexer.exit_code()
}
`)
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %+v", diags)
	}

	if len(program.Imports) != 2 {
		t.Fatalf("expected 2 imports, got %d", len(program.Imports))
	}
	if got, want := program.Imports[0].Path, "lexer"; got != want {
		t.Fatalf("unexpected first import path: got %q want %q", got, want)
	}
	if !program.Structs[0].Exported {
		t.Fatal("expected struct to be exported")
	}
	if !program.Functions[0].Exported {
		t.Fatal("expected use_kind to be exported")
	}
	if got, want := program.Functions[0].Params[0].Type.String(), "token.Kind"; got != want {
		t.Fatalf("unexpected qualified param type: got %q want %q", got, want)
	}

	ret, ok := program.Functions[1].Body.Stmts[0].(*ast.ReturnStmt)
	if !ok {
		t.Fatalf("expected return statement, got %T", program.Functions[1].Body.Stmts[0])
	}
	call, ok := ret.Value.(*ast.CallExpr)
	if !ok {
		t.Fatalf("expected call expression, got %T", ret.Value)
	}
	selector, ok := call.Callee.(*ast.SelectorExpr)
	if !ok {
		t.Fatalf("expected qualified callee, got %T", call.Callee)
	}
	inner, ok := selector.Inner.(*ast.IdentExpr)
	if !ok {
		t.Fatalf("expected package name identifier, got %T", selector.Inner)
	}
	if got, want := inner.Name, "lexer"; got != want {
		t.Fatalf("unexpected callee package: got %q want %q", got, want)
	}
	if got, want := selector.Name, "exit_code"; got != want {
		t.Fatalf("unexpected callee name: got %q want %q", got, want)
	}
}

func TestParseMapLiteral(t *testing.T) {
	t.Parallel()

	src := `
package main

fn main() i32 {
	m := map[str]i32{"a": 1, "b": 2}
	return 0
}
`
	program, diags := Parse(src)
	if len(diags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", diags)
	}

	if len(program.Functions) != 1 {
		t.Fatalf("expected 1 function, got %d", len(program.Functions))
	}
	fn := program.Functions[0]
	if len(fn.Body.Stmts) < 1 {
		t.Fatal("expected at least 1 statement")
	}
	letStmt, ok := fn.Body.Stmts[0].(*ast.LetStmt)
	if !ok {
		t.Fatalf("expected let statement, got %T", fn.Body.Stmts[0])
	}
	mapLit, ok := letStmt.Value.(*ast.MapLiteralExpr)
	if !ok {
		t.Fatalf("expected map literal, got %T", letStmt.Value)
	}
	if got, want := mapLit.Type.String(), "map[str]i32"; got != want {
		t.Fatalf("unexpected map type: got %q want %q", got, want)
	}
	if got, want := len(mapLit.Pairs), 2; got != want {
		t.Fatalf("unexpected pair count: got %d want %d", got, want)
	}
}

func TestParseMapTypeRef(t *testing.T) {
	t.Parallel()

	src := `
package main

fn lookup(m map[str]i32) i32 {
	return 0
}

fn main() i32 {
	return 0
}
`
	program, diags := Parse(src)
	if len(diags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", diags)
	}

	if len(program.Functions) != 2 {
		t.Fatalf("expected 2 functions, got %d", len(program.Functions))
	}
	fn := program.Functions[0]
	if len(fn.Params) != 1 {
		t.Fatalf("expected 1 param, got %d", len(fn.Params))
	}
	if got, want := fn.Params[0].Type.String(), "map[str]i32"; got != want {
		t.Fatalf("unexpected param type: got %q want %q", got, want)
	}
}
