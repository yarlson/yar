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
	if got, want := litExpr.Type.Name, "[]i32"; got != want {
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
	if got, want := program.Functions[0].Params[0].Type.Name, "token.Kind"; got != want {
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
