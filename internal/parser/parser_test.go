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
