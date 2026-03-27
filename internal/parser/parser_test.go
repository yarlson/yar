package parser

import (
	"strings"
	"testing"

	"yar/internal/ast"
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
