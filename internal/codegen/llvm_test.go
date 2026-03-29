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

func TestGenerateLowersShortCircuitBoolOperators(t *testing.T) {
	t.Parallel()

	ir := compileIR(t, `
package main

fn left() bool {
	return true
}

fn right() bool {
	return false
}

fn main() i32 {
	if left() && right() || left() {
		return 1
	}
	return 0
}
`)

	if !strings.Contains(ir, "logic.rhs") {
		t.Fatalf("expected short-circuit rhs block in IR:\n%s", ir)
	}
	if !strings.Contains(ir, "logic.end") {
		t.Fatalf("expected short-circuit end block in IR:\n%s", ir)
	}
	if !strings.Contains(ir, "phi i1") {
		t.Fatalf("expected phi for short-circuit boolean result in IR:\n%s", ir)
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
		"declare void @yar_gc_init_stack_top(ptr)",
		"declare void @yar_trap_oom()",
	} {
		if !strings.Contains(ir, want) {
			t.Fatalf("expected runtime helper declaration %q in IR:\n%s", want, ir)
		}
	}
}

func TestGenerateInitializesGCStackTopInMainWrapper(t *testing.T) {
	t.Parallel()

	ir := compileIR(t, `
package main

fn main() i32 {
	return 0
}
`)

	for _, want := range []string{
		"%gc.stack.slot = alloca i8",
		"call void @yar_gc_init_stack_top(ptr %gc.stack.slot)",
		"call void @yar_set_args(i32 %argc, ptr %argv)",
	} {
		if !strings.Contains(ir, want) {
			t.Fatalf("expected %q in IR:\n%s", want, ir)
		}
	}
}

func TestGenerateLowersPointers(t *testing.T) {
	t.Parallel()

	ir := compileIR(t, `
package main

struct Node {
	value i32
	next *Node
}

fn main() i32 {
	tail := &Node{value: 2, next: nil}
	head := &Node{value: 1, next: tail}
	if (*head).next == nil {
		return 1
	}
	next := (*head).next
	return (*next).value
}
`)

	for _, want := range []string{
		"%yar.struct.Node = type { i32, ptr }",
		"call ptr @yar_alloc(i64",
		"icmp eq ptr",
		"load ptr, ptr",
	} {
		if !strings.Contains(ir, want) {
			t.Fatalf("expected %q in IR:\n%s", want, ir)
		}
	}
}

func TestGenerateLowersEnumsAndMatch(t *testing.T) {
	t.Parallel()

	ir := compileIR(t, `
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
		match TokenKind.Ident {
		case TokenKind.Ident {
			return 0
		}
		case TokenKind.Int {
			return 2
		}
		}
	}
	}
}
`)

	for _, want := range []string{
		"%yar.enum.Expr = type { i32, [2 x i64] }",
		"%yar.enum.TokenKind = type { i32 }",
		"switch i32",
		"store %yar.struct.Expr_2EName",
	} {
		if !strings.Contains(ir, want) {
			t.Fatalf("expected %q in IR:\n%s", want, ir)
		}
	}
}

func TestGenerateLowersMapKeysBuiltin(t *testing.T) {
	t.Parallel()

	ir := compileIR(t, `
package main

fn main() i32 {
	m := map[str]i32{"a": 1, "b": 2}
	names := keys(m)
	return len(names)
}
`)

	for _, want := range []string{
		"declare %yar.slice @yar_map_keys(ptr)",
		"call %yar.slice @yar_map_keys(ptr",
		"extractvalue %yar.slice",
	} {
		if !strings.Contains(ir, want) {
			t.Fatalf("expected %q in IR:\n%s", want, ir)
		}
	}
}

func TestGenerateLowersMethods(t *testing.T) {
	t.Parallel()

	ir := compileIR(t, `
package main

struct Counter {
	value i32
}

fn (c Counter) current() i32 {
	return c.value
}

fn (c *Counter) inc(delta i32) void {
	(*c).value = (*c).value + delta
}

fn main() i32 {
	counter := &Counter{value: 2}
	counter.inc(3)
	return (*counter).current()
}
`)

	for _, want := range []string{
		"define i32 @yar.Counter.current(",
		"define void @yar.Counter.inc(",
		"call void @yar.Counter.inc(",
		"call i32 @yar.Counter.current(",
	} {
		if !strings.Contains(ir, want) {
			t.Fatalf("expected %q in IR:\n%s", want, ir)
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

	ir, err := Generate(program, info, "")
	if err != nil {
		t.Fatalf("generate IR: %v", err)
	}
	return ir
}
