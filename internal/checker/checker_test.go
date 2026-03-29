package checker

import (
	"strings"
	"testing"

	"yar/internal/parser"
)

func TestCheckErrorSugarValid(t *testing.T) {
	t.Parallel()

	src := `
package main

fn divide(a i32, b i32) !i32 {
	if b == 0 {
		return error.DivideByZero
	}
	return a / b
}

fn write_file(path str, data str) !void {
	return
}

fn log_error(err error) void {
	return
}

fn main() !i32 {
	x := divide(10, 2)?
	write_file("out.txt", "ok")?
	y := divide(10, 2) or |err| {
		log_error(err)
		return 0
	}
	return x + y
}
`

	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
	}
	_, diags := Check(program)
	if len(diags) > 0 {
		t.Fatalf("unexpected checker diagnostics: %+v", diags)
	}
}

func TestCheckErrorSugarInvalid(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		src    string
		substr string
	}{
		{
			name: "propagate non error",
			src: `
package main

fn main() i32 {
	x := 1?
	return x
}
`,
			substr: "? requires an errorable expression or error value",
		},
		{
			name: "handle non error",
			src: `
package main

fn main() i32 {
	x := 1 or |err| {
		return 0
	}
	return x
}
`,
			substr: "or requires an errorable expression or error value",
		},
		{
			name: "propagate in non error function",
			src: `
package main

fn divide(a i32, b i32) !i32 {
	return a / b
}

fn main() i32 {
	x := divide(10, 2)?
	return x
}
`,
			substr: "cannot use ? in a function that cannot return an error",
		},
		{
			name: "handler name escapes scope",
			src: `
package main

fn divide(a i32, b i32) !i32 {
	return a / b
}

fn main() i32 {
	x := divide(10, 2) or |err| {
		return 0
	}
	print_int(err)
	return x
}
`,
			substr: "unknown local \"err\"",
		},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			program, parseDiags := parser.Parse(tc.src)
			if len(parseDiags) > 0 {
				t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
			}

			_, diags := Check(program)
			if len(diags) == 0 {
				t.Fatal("expected checker diagnostics")
			}

			messages := make([]string, 0, len(diags))
			for _, diag := range diags {
				messages = append(messages, diag.Message)
			}
			if !strings.Contains(strings.Join(messages, "\n"), tc.substr) {
				t.Fatalf("expected diagnostic containing %q, got %q", tc.substr, strings.Join(messages, "\n"))
			}
		})
	}
}

func TestCheckV02FeaturesValid(t *testing.T) {
	t.Parallel()

	src := `
package main

struct User {
	id i32
	name str
}

fn lookup(id i32) !User {
	if id <= 0 {
		return error.InvalidUserID
	} else {
		return User{id: id, name: "user"}
	}
}

fn main() !i32 {
	var found User
	users := [2]User{
		User{id: 1, name: "alice"},
		User{id: 2, name: "bob"},
	}

	for i := 0; i < len(users); i = i + 1 {
		user := users[i]
		if !(user.id % 2 == 0) {
			continue
		}
		found = user
		break
	}

	if found.id == 0 {
		found = lookup(2)?
	}

	return 0
}
`

	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
	}

	_, diags := Check(program)
	if len(diags) > 0 {
		t.Fatalf("unexpected checker diagnostics: %+v", diags)
	}
}

func TestCheckBoolOperatorsValid(t *testing.T) {
	t.Parallel()

	src := `
package main

fn main() i32 {
	ok := true
	ready := false
	debug := true
	if ok && ready || debug {
		return 1
	}
	return 0
}
`

	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
	}

	_, diags := Check(program)
	if len(diags) > 0 {
		t.Fatalf("unexpected checker diagnostics: %+v", diags)
	}
}

func TestCheckBoolOperatorsWithPropagateValid(t *testing.T) {
	t.Parallel()

	src := `
package main

fn maybe(ok bool) !bool {
	return ok
}

fn main() !i32 {
	ready := true
	if maybe(true)? && ready {
		return 1
	}
	return 0
}
`

	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
	}

	_, diags := Check(program)
	if len(diags) > 0 {
		t.Fatalf("unexpected checker diagnostics: %+v", diags)
	}
}

func TestCheckMethodsValid(t *testing.T) {
	t.Parallel()

	src := `
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
`

	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
	}

	_, diags := Check(program)
	if len(diags) > 0 {
		t.Fatalf("unexpected checker diagnostics: %+v", diags)
	}
}

func TestCheckMethodsInvalid(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		src    string
		substr string
	}{
		{
			name: "non struct receiver",
			src: `
package main

fn (n i32) double() i32 {
	return n * 2
}

fn main() i32 {
	return 0
}
`,
			substr: "method receiver must be a named struct type or pointer to one",
		},
		{
			name: "pointer mismatch",
			src: `
package main

struct Counter {
	value i32
}

fn (c Counter) current() i32 {
	return c.value
}

fn main() i32 {
	counter := &Counter{value: 2}
	return counter.current()
}
`,
			substr: "type \"*Counter\" has no method \"current\"",
		},
		{
			name: "method values unsupported",
			src: `
package main

struct Counter {
	value i32
}

fn (c Counter) current() i32 {
	return c.value
}

fn main() i32 {
	counter := Counter{value: 2}
	value := counter.current
	return 0
}
`,
			substr: "method values are not supported",
		},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			program, parseDiags := parser.Parse(tc.src)
			if len(parseDiags) > 0 {
				t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
			}

			_, diags := Check(program)
			if len(diags) == 0 {
				t.Fatal("expected checker diagnostics")
			}

			messages := make([]string, 0, len(diags))
			for _, diag := range diags {
				messages = append(messages, diag.Message)
			}
			if !strings.Contains(strings.Join(messages, "\n"), tc.substr) {
				t.Fatalf("expected diagnostic containing %q, got %q", tc.substr, strings.Join(messages, "\n"))
			}
		})
	}
}

func TestCheckClosuresValid(t *testing.T) {
	t.Parallel()

	src := `
package main

fn apply_twice(f fn(i32) i32, value i32) i32 {
	return f(f(value))
}

fn make_adder(base i32) fn(i32) i32 {
	return fn(delta i32) i32 {
		return base + delta
	}
}

fn main() i32 {
	base := 10
	add := make_adder(base)
	inc := fn(value i32) i32 {
		return value + 1
	}
	return apply_twice(add, inc(1))
}
`

	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
	}

	info, diags := Check(program)
	if len(diags) > 0 {
		t.Fatalf("unexpected checker diagnostics: %+v", diags)
	}
	if got, want := len(info.FunctionLiterals), 2; got != want {
		t.Fatalf("unexpected function literal count: got %d want %d", got, want)
	}
}

func TestCheckClosuresInvalid(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		src    string
		substr string
	}{
		{
			name: "captured assignment rejected",
			src: `
package main

fn main() i32 {
	value := 1
	f := fn() i32 {
		value = 2
		return value
	}
	return f()
}
`,
			substr: "cannot assign to captured outer local",
		},
		{
			name: "call arity mismatch through function value",
			src: `
package main

fn main() i32 {
	f := fn(value i32) i32 {
		return value
	}
	return f()
}
`,
			substr: "function \"function value\" expects 1 arguments, got 0",
		},
		{
			name: "captured address rejected",
			src: `
package main

fn main() i32 {
	value := 1
	f := fn() i32 {
		ptr := &value
		if ptr == nil {
			return 1
		}
		return value
	}
	return f()
}
`,
			substr: "captured outer local \"value\" is not addressable",
		},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			program, parseDiags := parser.Parse(tc.src)
			if len(parseDiags) > 0 {
				t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
			}

			_, diags := Check(program)
			if len(diags) == 0 {
				t.Fatal("expected checker diagnostics")
			}

			messages := make([]string, 0, len(diags))
			for _, diag := range diags {
				messages = append(messages, diag.Message)
			}
			if !strings.Contains(strings.Join(messages, "\n"), tc.substr) {
				t.Fatalf("expected diagnostic containing %q, got %q", tc.substr, strings.Join(messages, "\n"))
			}
		})
	}
}

func TestCheckMethodNameDoesNotShadowFieldAccess(t *testing.T) {
	t.Parallel()

	src := `
package main

struct User {
	name str
}

fn (u User) name() str {
	return "method"
}

fn main() i32 {
	user := User{name: "field"}
	print(user.name)
	print("\n")
	print(user.name())
	print("\n")
	return 0
}
`

	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
	}

	_, diags := Check(program)
	if len(diags) > 0 {
		t.Fatalf("unexpected checker diagnostics: %+v", diags)
	}
}

func TestCheckEnumsAndMatchValid(t *testing.T) {
	t.Parallel()

	src := `
package main

enum TokenKind {
	Ident
	Int
}

enum Expr {
	Int { value i64 }
	Name { text str }
}

fn kind_name(kind TokenKind) str {
	match kind {
	case TokenKind.Ident {
		return "ident"
	}
	case TokenKind.Int {
		return "int"
	}
	}
}

fn main() i32 {
	expr := Expr.Name{text: "main"}
	match expr {
	case Expr.Int(_) {
		return 1
	}
	case Expr.Name(v) {
		print(v.text)
		print(kind_name(TokenKind.Ident))
		return 0
	}
	}
}
`

	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
	}

	info, diags := Check(program)
	if len(diags) > 0 {
		t.Fatalf("unexpected checker diagnostics: %+v", diags)
	}
	if _, ok := info.Enums["Expr"]; !ok {
		t.Fatal("expected enum metadata for Expr")
	}
	if _, ok := info.Structs["Expr.Name"]; !ok {
		t.Fatal("expected payload struct metadata for Expr.Name")
	}
}

func TestCheckEnumsAndMatchInvalid(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		src    string
		substr string
	}{
		{
			name: "non exhaustive match",
			src: `
package main

enum TokenKind {
	Ident
	Int
}

fn main() i32 {
	kind := TokenKind.Ident
	match kind {
	case TokenKind.Ident {
		return 0
	}
	}
}
`,
			substr: "not exhaustive",
		},
		{
			name: "duplicate enum case",
			src: `
package main

enum TokenKind {
	Ident
	Ident
}

fn main() i32 {
	return 0
}
`,
			substr: "already declared in enum",
		},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			program, parseDiags := parser.Parse(tc.src)
			if len(parseDiags) > 0 {
				t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
			}

			_, diags := Check(program)
			if len(diags) == 0 {
				t.Fatal("expected checker diagnostics")
			}

			messages := make([]string, 0, len(diags))
			for _, diag := range diags {
				messages = append(messages, diag.Message)
			}
			if !strings.Contains(strings.Join(messages, "\n"), tc.substr) {
				t.Fatalf("expected diagnostic containing %q, got %q", tc.substr, strings.Join(messages, "\n"))
			}
		})
	}
}

func TestCheckSlicesValid(t *testing.T) {
	t.Parallel()

	src := `
package main

fn prefix(values []i32, n i32) []i32 {
	return values[0:n]
}

fn main() i32 {
	values := []i32{}
	values = append(values, 1)
	values = append(values, 2)
	part := prefix(values, len(values))
	part[1] = 9
	return values[1]
}
`

	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
	}

	_, diags := Check(program)
	if len(diags) > 0 {
		t.Fatalf("unexpected checker diagnostics: %+v", diags)
	}
}

func TestCheckRecursiveSliceStructValid(t *testing.T) {
	t.Parallel()

	src := `
package main

struct Node {
	children []Node
}

fn main() i32 {
	root := Node{}
	root.children = append(root.children, Node{})
	return len(root.children)
}
`

	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
	}

	_, diags := Check(program)
	if len(diags) > 0 {
		t.Fatalf("unexpected checker diagnostics: %+v", diags)
	}
}

func TestCheckPointersValid(t *testing.T) {
	t.Parallel()

	src := `
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
	head := &Node{value: 1, next: tail}
	set_value(head, 3)
	if (*head).next != nil {
		next := (*head).next
		return (*next).value + (*head).value
	}
	return 0
}
`

	program, parseDiags := parser.Parse(src)
	if len(parseDiags) > 0 {
		t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
	}

	_, diags := Check(program)
	if len(diags) > 0 {
		t.Fatalf("unexpected checker diagnostics: %+v", diags)
	}
}

func TestCheckV02FeaturesInvalid(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		src    string
		substr string
	}{
		{
			name: "break outside loop",
			src: `
package main

fn main() i32 {
	break
	return 0
}
`,
			substr: "break can only be used inside a loop",
		},
		{
			name: "continue outside loop",
			src: `
package main

fn main() i32 {
	continue
	return 0
}
`,
			substr: "continue can only be used inside a loop",
		},
		{
			name: "len requires array",
			src: `
package main

fn main() i32 {
	x := len(1)
	return x
}
`,
			substr: "len requires an array, slice, map, or str argument",
		},
		{
			name: "builtin len cannot be redeclared",
			src: `
package main

fn len(value i32) i32 {
	return value
}

fn main() i32 {
	return 0
}
`,
			substr: "function \"len\" is already declared",
		},
		{
			name: "slice element type cannot be void",
			src: `
package main

fn main() i32 {
	values := []void{}
	return 0
}
`,
			substr: "slice element type \"void\" is not allowed",
		},
		{
			name: "append value type mismatch",
			src: `
package main

fn main() i32 {
	values := []i32{}
	values = append(values, true)
	return 0
}
`,
			substr: "argument 2 to \"append\" must be i32, got bool",
		},
		{
			name: "slicing requires slice",
			src: `
package main

fn main() i32 {
	values := [2]i32{1, 2}
	part := values[0:1]
	return len(part)
}
`,
			substr: "slicing requires a slice or str value",
		},
		{
			name: "unknown struct field",
			src: `
package main

struct User {
	id i32
}

fn main() i32 {
	user := User{id: 1}
	return user.name
}
`,
			substr: "struct \"User\" has no field \"name\"",
		},
		{
			name: "logical operators require bool operands",
			src: `
package main

fn main() i32 {
	x := 1 && 2
	return 0
}
`,
			substr: "logical operators require bool operands",
		},
		{
			name: "logical operators reject errorable operands",
			src: `
package main

fn maybe() !bool {
	return true
}

fn main() !i32 {
	ok := true
	if maybe() && ok {
		return 1
	}
	return 0
}
`,
			substr: "binary operators cannot use errorable operands",
		},
		{
			name: "direct recursive pointerless struct rejected",
			src: `
package main

struct Bad {
	next Bad
}

fn main() i32 {
	return 0
}
`,
			substr: "struct \"Bad\" cannot contain itself recursively",
		},
		{
			name: "void pointer rejected",
			src: `
package main

fn main() i32 {
	var p *void
	return 0
}
`,
			substr: "pointer target type \"void\" is not allowed",
		},
		{
			name: "dereference requires pointer",
			src: `
package main

fn main() i32 {
	x := *1
	return x
}
`,
			substr: "dereference requires a pointer operand",
		},
		{
			name: "address of temporary rejected",
			src: `
package main

fn main() i32 {
	x := &(1 + 2)
	return 0
}
`,
			substr: "address-of requires an addressable operand or composite literal",
		},
		{
			name: "nil requires pointer context",
			src: `
package main

fn main() i32 {
	p := nil
	return 0
}
`,
			substr: "cannot infer type from nil without a pointer context",
		},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			program, parseDiags := parser.Parse(tc.src)
			if len(parseDiags) > 0 {
				t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
			}

			_, diags := Check(program)
			if len(diags) == 0 {
				t.Fatal("expected checker diagnostics")
			}

			messages := make([]string, 0, len(diags))
			for _, diag := range diags {
				messages = append(messages, diag.Message)
			}
			if !strings.Contains(strings.Join(messages, "\n"), tc.substr) {
				t.Fatalf("expected diagnostic containing %q, got %q", tc.substr, strings.Join(messages, "\n"))
			}
		})
	}
}

func TestCheckMapFeaturesValid(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		src  string
	}{
		{
			name: "map literal and len",
			src: `
package main

fn main() i32 {
	m := map[str]i32{"a": 1}
	return len(m)
}
`,
		},
		{
			name: "map has builtin",
			src: `
package main

fn main() i32 {
	m := map[i32]bool{1: true}
	if has(m, 1) {
		return 0
	}
	return 1
}
`,
		},
		{
			name: "map index with propagation",
			src: `
package main

fn main() !i32 {
	m := map[str]i32{"x": 42}
	v := m["x"]?
	return v
}
`,
		},
		{
			name: "map assignment",
			src: `
package main

fn main() !i32 {
	m := map[str]i32{}
	m["k"] = 10
	return m["k"]?
}
`,
		},
		{
			name: "map delete",
			src: `
package main

fn main() i32 {
	m := map[str]i32{"a": 1}
	delete(m, "a")
	return len(m)
}
`,
		},
		{
			name: "map keys builtin",
			src: `
package main

fn main() i32 {
	m := map[str]i32{"a": 1, "b": 2}
	names := keys(m)
	return len(names)
}
`,
		},
		{
			name: "map index with or handler",
			src: `
package main

fn main() i32 {
	m := map[str]i32{}
	v := m["x"] or |err| {
		return 0
	}
	return v
}
`,
		},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			program, parseDiags := parser.Parse(tc.src)
			if len(parseDiags) > 0 {
				t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
			}

			_, diags := Check(program)
			if len(diags) > 0 {
				messages := make([]string, 0, len(diags))
				for _, diag := range diags {
					messages = append(messages, diag.Message)
				}
				t.Fatalf("unexpected diagnostics: %s", strings.Join(messages, "\n"))
			}
		})
	}
}

func TestCheckMapFeaturesInvalid(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		src    string
		substr string
	}{
		{
			name: "invalid key type array",
			src: `
package main

fn main() i32 {
	m := map[[2]i32]i32{}
	return 0
}
`,
			substr: "map key type",
		},
		{
			name: "has requires map",
			src: `
package main

fn main() i32 {
	x := has(1, 2)
	return 0
}
`,
			substr: "has requires a map",
		},
		{
			name: "delete requires map",
			src: `
package main

fn main() i32 {
	delete(1, 2)
	return 0
}
`,
			substr: "delete requires a map",
		},
		{
			name: "keys requires map",
			src: `
package main

fn main() i32 {
	values := []i32{1, 2}
	names := keys(values)
	return len(names)
}
`,
			substr: "keys requires a map",
		},
		{
			name: "map index returns errorable",
			src: `
package main

fn main() i32 {
	m := map[str]i32{}
	x := m["k"]
	return 0
}
`,
			substr: "errorable value cannot be bound to a local",
		},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			program, parseDiags := parser.Parse(tc.src)
			if len(parseDiags) > 0 {
				t.Fatalf("unexpected parse diagnostics: %+v", parseDiags)
			}

			_, diags := Check(program)
			if len(diags) == 0 {
				t.Fatal("expected checker diagnostics")
			}

			messages := make([]string, 0, len(diags))
			for _, diag := range diags {
				messages = append(messages, diag.Message)
			}
			if !strings.Contains(strings.Join(messages, "\n"), tc.substr) {
				t.Fatalf("expected diagnostic containing %q, got %q", tc.substr, strings.Join(messages, "\n"))
			}
		})
	}
}

func TestCheckStringOpsValid(t *testing.T) {
	t.Parallel()

	src := `
package main

fn main() i32 {
	s := "hello"
	n := len(s)
	if s == "hello" {
		print("eq\n")
	}
	if s != "other" {
		print("ne\n")
	}
	cat := s + " world"
	b := s[0]
	sub := s[1:3]
	print(cat)
	print(sub)
	print_int(b)
	print_int(n)
	return 0
}
`
	program, diags := parser.Parse(src)
	if program == nil {
		t.Fatalf("parse failed: %+v", diags)
	}
	_, checkDiags := Check(program)
	if len(checkDiags) > 0 {
		messages := make([]string, 0, len(checkDiags))
		for _, d := range checkDiags {
			messages = append(messages, d.Message)
		}
		t.Fatalf("unexpected diagnostics: %s", strings.Join(messages, "\n"))
	}
}

func TestCheckStringOpsInvalid(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		src    string
		substr string
	}{
		{
			name: "string equality with non-string",
			src: `
package main

fn main() i32 {
	ok := "a" == 1
	return 0
}
`,
			substr: "comparison operands must have the same type",
		},
		{
			name: "string index with bool",
			src: `
package main

fn main() i32 {
	x := "abc"[true]
	return 0
}
`,
			substr: "index expression must be an integer",
		},
		{
			name: "string concat with int",
			src: `
package main

fn main() i32 {
	x := "abc" + 1
	return 0
}
`,
			substr: "requires matching integer or str operands",
		},
	}

	for _, tc := range tests {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			program, _ := parser.Parse(tc.src)
			if program == nil {
				t.Fatal("parse failed")
			}

			_, diags := Check(program)
			if len(diags) == 0 {
				t.Fatal("expected checker diagnostics")
			}

			messages := make([]string, 0, len(diags))
			for _, diag := range diags {
				messages = append(messages, diag.Message)
			}
			if !strings.Contains(strings.Join(messages, "\n"), tc.substr) {
				t.Fatalf("expected diagnostic containing %q, got %q", tc.substr, strings.Join(messages, "\n"))
			}
		})
	}
}
