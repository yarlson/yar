package codegen

import (
	"strings"
	"testing"

	"yar/internal/checker"
	"yar/internal/parser"
)

func TestGenerateLowersPropagateSugar(t *testing.T) {
	t.Parallel()

	ir := compileIR(t, `
package main

fn divide(a i32, b i32) !i32 {
	if b == 0 {
		return error.DivideByZero
	}
	return a / b
}

fn main() !i32 {
	x := divide(10, 2)?
	return x
}
`)

	if !strings.Contains(ir, "propagate.err") {
		t.Fatalf("expected propagate error block in IR:\n%s", ir)
	}
	if !strings.Contains(ir, "extractvalue %yar.result.i32") {
		t.Fatalf("expected result extraction in IR:\n%s", ir)
	}
}

func TestGenerateLowersHandleSugar(t *testing.T) {
	t.Parallel()

	ir := compileIR(t, `
package main

fn divide(a i32, b i32) !i32 {
	if b == 0 {
		return error.DivideByZero
	}
	return a / b
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

	if !strings.Contains(ir, "handle.err") {
		t.Fatalf("expected handle error block in IR:\n%s", ir)
	}
	if !strings.Contains(ir, "call void @yar.log_error(i32") {
		t.Fatalf("expected handler to receive bound error value:\n%s", ir)
	}
}

func compileIR(t *testing.T, src string) string {
	t.Helper()

	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
	}

	info, checkDiags := checker.Check(program)
	if len(checkDiags) > 0 {
		t.Fatalf("unexpected checker diagnostics: %+v", checkDiags)
	}

	ir, err := Generate(program, info)
	if err != nil {
		t.Fatalf("generate IR: %v", err)
	}
	return ir
}
