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

func TestGenerateDeclaresMemoryRuntimeHelpers(t *testing.T) {
	t.Parallel()

	ir := compileIR(t, `
package main

fn main() i32 {
	return 0
}
`)

	for _, want := range []string{
		"declare ptr @yar_alloc(i64)",
		"declare ptr @yar_alloc_zeroed(i64)",
		"declare void @yar_trap_oom()",
	} {
		if !strings.Contains(ir, want) {
			t.Fatalf("expected runtime helper declaration %q in IR:\n%s", want, ir)
		}
	}
}

func TestEmitAllocHelpers(t *testing.T) {
	t.Parallel()

	f := &functionEmitter{g: &Generator{}}

	ptr := f.emitAllocType(checker.TypeI32, true)
	if !strings.HasPrefix(ptr, "%alloc.") {
		t.Fatalf("expected allocation temp, got %q", ptr)
	}

	ir := f.builder.String()
	if !strings.Contains(ir, "getelementptr i32, ptr null, i32 1") {
		t.Fatalf("expected type-size calculation in IR:\n%s", ir)
	}
	if !strings.Contains(ir, "call ptr @yar_alloc_zeroed(i64 %size.") {
		t.Fatalf("expected zeroed allocation helper call in IR:\n%s", ir)
	}

	f = &functionEmitter{g: &Generator{}}
	ptr = f.emitAllocBytes("8", false)
	if !strings.HasPrefix(ptr, "%alloc.") {
		t.Fatalf("expected allocation temp, got %q", ptr)
	}
	if got := f.builder.String(); !strings.Contains(got, "call ptr @yar_alloc(i64 8)") {
		t.Fatalf("expected plain allocation helper call in IR:\n%s", got)
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
