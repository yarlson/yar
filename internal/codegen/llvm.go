package codegen

import (
	"fmt"
	"sort"
	"strings"
	"yar/internal/ast"
	"yar/internal/checker"
	"yar/internal/token"
)

type Generator struct {
	program        *ast.Program
	info           checker.Info
	functions      map[string]checker.Signature
	stringNames    map[string]string
	stringOrder    []string
	nextStringID   int
	usedResultType map[checker.Type]bool
}

func Generate(program *ast.Program, info checker.Info) (string, error) {
	g := &Generator{
		program:        program,
		info:           info,
		functions:      builtins(),
		stringNames:    make(map[string]string),
		usedResultType: make(map[checker.Type]bool),
	}

	for fn, sig := range info.Functions {
		g.functions[fn.Name] = sig
		if sig.Errorable {
			g.usedResultType[sig.Return] = true
		}
	}

	var b strings.Builder
	b.WriteString("; yar v0\n")
	b.WriteString("source_filename = \"yar\"\n\n")
	b.WriteString("%yar.str = type { ptr, i64 }\n")

	resultTypes := make([]string, 0, len(g.usedResultType))
	for typ := range g.usedResultType {
		resultTypes = append(resultTypes, string(typ))
	}
	sort.Strings(resultTypes)
	for _, name := range resultTypes {
		typ := checker.Type(name)
		fmt.Fprintf(&b, "%s = type %s\n", g.resultTypeName(typ), g.resultStructLiteral(typ))
	}
	b.WriteString("\n")
	b.WriteString("declare void @yar_print(ptr, i64)\n")
	b.WriteString("declare void @yar_print_int(i32)\n\n")

	var functionIR []string
	for _, fn := range program.Functions {
		sig, ok := info.Functions[fn]
		if !ok {
			continue
		}
		emitter := newFunctionEmitter(g, fn, sig)
		functionIR = append(functionIR, emitter.emit())
	}

	wrapper, err := g.emitMainWrapper()
	if err != nil {
		return "", err
	}
	functionIR = append(functionIR, wrapper)

	for _, value := range g.stringOrder {
		name := g.stringNames[value]
		bytes := escapeLLVMString(value + "\x00")
		length := len([]byte(value)) + 1
		fmt.Fprintf(&b, "@%s = private unnamed_addr constant [%d x i8] c\"%s\"\n", name, length, bytes)
	}
	if len(g.stringOrder) > 0 {
		b.WriteByte('\n')
	}

	for i, def := range functionIR {
		if i > 0 {
			b.WriteByte('\n')
		}
		b.WriteString(def)
	}

	return b.String(), nil
}

func builtins() map[string]checker.Signature {
	return map[string]checker.Signature{
		"print": {
			Name:    "print",
			Params:  []checker.Type{checker.TypeStr},
			Return:  checker.TypeVoid,
			Builtin: true,
		},
		"print_int": {
			Name:    "print_int",
			Params:  []checker.Type{checker.TypeI32},
			Return:  checker.TypeVoid,
			Builtin: true,
		},
	}
}

func (g *Generator) emitMainWrapper() (string, error) {
	mainSig, ok := g.functions["main"]
	if !ok {
		return "", fmt.Errorf("missing main signature")
	}

	var b strings.Builder
	b.WriteString("define i32 @main() {\n")
	b.WriteString("entry:\n")
	if !mainSig.Errorable {
		fmt.Fprintf(&b, "  %%main_value = call i32 @yar.main()\n")
		b.WriteString("  ret i32 %main_value\n")
		b.WriteString("}\n")
		return b.String(), nil
	}

	fmt.Fprintf(&b, "  %%main_result = call %s @yar.main()\n", g.resultTypeName(mainSig.Return))
	fmt.Fprintf(&b, "  %%main_is_err = extractvalue %s %%main_result, 0\n", g.resultTypeName(mainSig.Return))
	b.WriteString("  br i1 %main_is_err, label %main.err, label %main.ok\n\n")
	b.WriteString("main.ok:\n")
	fmt.Fprintf(&b, "  %%main_ok_value = extractvalue %s %%main_result, 2\n", g.resultTypeName(mainSig.Return))
	b.WriteString("  ret i32 %main_ok_value\n\n")
	b.WriteString("main.err:\n")
	fmt.Fprintf(&b, "  %%main_err_code = extractvalue %s %%main_result, 1\n", g.resultTypeName(mainSig.Return))
	if len(g.info.OrderedErrors) == 0 {
		msg := g.stringConstant("unhandled error\n")
		ptr, length := g.stringPointer(msg, "main.error.message")
		fmt.Fprintf(&b, "  %s\n", ptr)
		fmt.Fprintf(&b, "  call void @yar_print(ptr %%main.error.message, i64 %d)\n", length)
		b.WriteString("  ret i32 1\n")
		b.WriteString("}\n")
		return b.String(), nil
	}

	b.WriteString("  switch i32 %main_err_code, label %main.err.unknown [\n")
	for _, name := range g.info.OrderedErrors {
		fmt.Fprintf(&b, "    i32 %d, label %%main.err.%s\n", g.info.ErrorCodes[name], sanitizeLabel(name))
	}
	b.WriteString("  ]\n\n")
	for _, name := range g.info.OrderedErrors {
		label := sanitizeLabel(name)
		b.WriteString("main.err." + label + ":\n")
		ptr1, length1 := g.stringPointer(g.stringConstant("unhandled error: "), "main.err.prefix."+label)
		ptr2, length2 := g.stringPointer(g.stringConstant(name), "main.err.name."+label)
		ptr3, length3 := g.stringPointer(g.stringConstant("\n"), "main.err.newline."+label)
		fmt.Fprintf(&b, "  %s\n", ptr1)
		fmt.Fprintf(&b, "  call void @yar_print(ptr %%main.err.prefix.%s, i64 %d)\n", label, length1)
		fmt.Fprintf(&b, "  %s\n", ptr2)
		fmt.Fprintf(&b, "  call void @yar_print(ptr %%main.err.name.%s, i64 %d)\n", label, length2)
		fmt.Fprintf(&b, "  %s\n", ptr3)
		fmt.Fprintf(&b, "  call void @yar_print(ptr %%main.err.newline.%s, i64 %d)\n", label, length3)
		b.WriteString("  ret i32 1\n\n")
	}
	b.WriteString("main.err.unknown:\n")
	ptr, length := g.stringPointer(g.stringConstant("unhandled error\n"), "main.err.unknown.message")
	fmt.Fprintf(&b, "  %s\n", ptr)
	fmt.Fprintf(&b, "  call void @yar_print(ptr %%main.err.unknown.message, i64 %d)\n", length)
	b.WriteString("  ret i32 1\n")
	b.WriteString("}\n")
	return b.String(), nil
}

func (g *Generator) llvmType(typ checker.Type) string {
	switch typ {
	case checker.TypeVoid:
		return "void"
	case checker.TypeBool:
		return "i1"
	case checker.TypeI32:
		return "i32"
	case checker.TypeStr:
		return "%yar.str"
	default:
		panic("unsupported type: " + typ)
	}
}

func (g *Generator) resultTypeName(typ checker.Type) string {
	return "%yar.result." + string(typ)
}

func (g *Generator) resultStructLiteral(typ checker.Type) string {
	if typ == checker.TypeVoid {
		return "{ i1, i32 }"
	}
	return fmt.Sprintf("{ i1, i32, %s }", g.llvmType(typ))
}

func (g *Generator) functionName(name string) string {
	return "@yar." + name
}

func (g *Generator) stringConstant(value string) string {
	if name, ok := g.stringNames[value]; ok {
		return name
	}
	name := fmt.Sprintf(".str.%d", g.nextStringID)
	g.nextStringID++
	g.stringNames[value] = name
	g.stringOrder = append(g.stringOrder, value)
	return name
}

func (g *Generator) stringPointer(global, temp string) (instruction string, length int) {
	value := findValueByGlobal(g.stringNames, global)
	length = len([]byte(value))
	instruction = fmt.Sprintf("%%%s = getelementptr inbounds [%d x i8], ptr @%s, i64 0, i64 0", temp, length+1, global)
	return instruction, length
}

func findValueByGlobal(index map[string]string, global string) string {
	for value, name := range index {
		if name == global {
			return value
		}
	}
	return ""
}

func escapeLLVMString(value string) string {
	var b strings.Builder
	for _, c := range []byte(value) {
		switch {
		case c >= 32 && c <= 126 && c != '"' && c != '\\':
			b.WriteByte(c)
		default:
			fmt.Fprintf(&b, "\\%02X", c)
		}
	}
	return b.String()
}

func sanitizeLabel(name string) string {
	var b strings.Builder
	for _, r := range name {
		if (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9') {
			b.WriteRune(r)
			continue
		}
		b.WriteByte('_')
	}
	return b.String()
}

type functionEmitter struct {
	g          *Generator
	fn         *ast.FunctionDecl
	sig        checker.Signature
	builder    strings.Builder
	tempID     int
	labelID    int
	terminated bool
	scopes     []map[string]localSlot
}

type localSlot struct {
	typ checker.Type
	ptr string
}

func newFunctionEmitter(g *Generator, fn *ast.FunctionDecl, sig checker.Signature) *functionEmitter {
	return &functionEmitter{
		g:      g,
		fn:     fn,
		sig:    sig,
		scopes: []map[string]localSlot{{}},
	}
}

func (f *functionEmitter) emit() string {
	retType := f.g.llvmType(f.sig.Return)
	if f.sig.Errorable {
		retType = f.g.resultTypeName(f.sig.Return)
	}

	params := make([]string, 0, len(f.fn.Params))
	for i, param := range f.fn.Params {
		params = append(params, fmt.Sprintf("%s %%arg%d", f.g.llvmType(f.sig.Params[i]), i))
		_ = param
	}

	fmt.Fprintf(&f.builder, "define %s %s(%s) {\n", retType, f.g.functionName(f.fn.Name), strings.Join(params, ", "))
	f.emitLabel("entry")
	for i, param := range f.fn.Params {
		slot := f.newTemp("slot")
		fmt.Fprintf(&f.builder, "  %%%s = alloca %s\n", slot, f.g.llvmType(f.sig.Params[i]))
		fmt.Fprintf(&f.builder, "  store %s %%arg%d, ptr %%%s\n", f.g.llvmType(f.sig.Params[i]), i, slot)
		f.bindLocal(param.Name, localSlot{typ: f.sig.Params[i], ptr: "%" + slot})
	}

	f.genBlock(f.fn.Body)
	if !f.terminated {
		if f.sig.Return == checker.TypeVoid {
			if f.sig.Errorable {
				result := f.emitSuccessVoid()
				fmt.Fprintf(&f.builder, "  ret %s %s\n", f.g.resultTypeName(checker.TypeVoid), result)
			} else {
				f.builder.WriteString("  ret void\n")
			}
		}
	}
	f.builder.WriteString("}\n")
	return f.builder.String()
}

func (f *functionEmitter) genBlock(block *ast.BlockStmt) {
	f.pushScope()
	defer f.popScope()
	for _, stmt := range block.Stmts {
		if f.terminated {
			return
		}
		f.genStatement(stmt)
	}
}

func (f *functionEmitter) genStatement(stmt ast.Statement) {
	switch s := stmt.(type) {
	case *ast.BlockStmt:
		f.genBlock(s)
	case *ast.LetStmt:
		value := f.genExpression(s.Value)
		slot := f.newTemp("slot")
		fmt.Fprintf(&f.builder, "  %%%s = alloca %s\n", slot, f.g.llvmType(value.typ))
		fmt.Fprintf(&f.builder, "  store %s %s, ptr %%%s\n", f.g.llvmType(value.typ), value.ref, slot)
		f.bindLocal(s.Name, localSlot{typ: value.typ, ptr: "%" + slot})
	case *ast.AssignStmt:
		slot, ok := f.lookupLocal(s.Name)
		if !ok {
			return
		}
		value := f.genExpression(s.Value)
		fmt.Fprintf(&f.builder, "  store %s %s, ptr %s\n", f.g.llvmType(slot.typ), value.ref, slot.ptr)
	case *ast.IfStmt:
		cond := f.genExpression(s.Cond)
		thenLabel := f.newLabel("if.then")
		endLabel := f.newLabel("if.end")
		fmt.Fprintf(&f.builder, "  br i1 %s, label %%%s, label %%%s\n", cond.ref, thenLabel, endLabel)
		f.terminated = true
		f.emitLabel(thenLabel)
		f.genBlock(s.Then)
		if !f.terminated {
			fmt.Fprintf(&f.builder, "  br label %%%s\n", endLabel)
			f.terminated = true
		}
		f.emitLabel(endLabel)
	case *ast.ReturnStmt:
		f.genReturn(s)
	case *ast.ExprStmt:
		_ = f.genExpression(s.Expr)
	default:
		panic("unsupported statement")
	}
}

func (f *functionEmitter) genReturn(stmt *ast.ReturnStmt) {
	if f.sig.Errorable {
		if stmt.Value == nil {
			result := f.emitSuccessVoid()
			fmt.Fprintf(&f.builder, "  ret %s %s\n", f.g.resultTypeName(checker.TypeVoid), result)
			f.terminated = true
			return
		}

		if errLit, ok := stmt.Value.(*ast.ErrorLiteral); ok {
			result := f.emitErrorResult(f.sig.Return, errLit.Name)
			fmt.Fprintf(&f.builder, "  ret %s %s\n", f.g.resultTypeName(f.sig.Return), result)
			f.terminated = true
			return
		}

		value := f.genExpression(stmt.Value)
		result := f.emitSuccessResult(f.sig.Return, value.ref)
		fmt.Fprintf(&f.builder, "  ret %s %s\n", f.g.resultTypeName(f.sig.Return), result)
		f.terminated = true
		return
	}

	if stmt.Value == nil {
		f.builder.WriteString("  ret void\n")
		f.terminated = true
		return
	}

	value := f.genExpression(stmt.Value)
	fmt.Fprintf(&f.builder, "  ret %s %s\n", f.g.llvmType(value.typ), value.ref)
	f.terminated = true
}

type exprValue struct {
	ref       string
	typ       checker.Type
	errorable bool
}

func (f *functionEmitter) genExpression(expr ast.Expression) exprValue {
	switch e := expr.(type) {
	case *ast.IdentExpr:
		slot, _ := f.lookupLocal(e.Name)
		tmp := f.newTemp("load")
		fmt.Fprintf(&f.builder, "  %%%s = load %s, ptr %s\n", tmp, f.g.llvmType(slot.typ), slot.ptr)
		return exprValue{ref: "%" + tmp, typ: slot.typ}
	case *ast.IntLiteral:
		return exprValue{ref: fmt.Sprintf("%d", e.Value), typ: checker.TypeI32}
	case *ast.BoolLiteral:
		if e.Value {
			return exprValue{ref: "1", typ: checker.TypeBool}
		}
		return exprValue{ref: "0", typ: checker.TypeBool}
	case *ast.StringLiteral:
		return f.emitStringValue(e.Value)
	case *ast.GroupExpr:
		return f.genExpression(e.Inner)
	case *ast.BinaryExpr:
		return f.genBinary(e)
	case *ast.CallExpr:
		return f.genCall(e)
	case *ast.CatchExpr:
		return f.genCatch(e)
	default:
		panic(fmt.Sprintf("unsupported expression %T", expr))
	}
}

func (f *functionEmitter) genBinary(expr *ast.BinaryExpr) exprValue {
	left := f.genExpression(expr.Left)
	right := f.genExpression(expr.Right)
	out := f.newTemp("tmp")
	switch expr.Operator {
	case token.Plus:
		fmt.Fprintf(&f.builder, "  %%%s = add i32 %s, %s\n", out, left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: checker.TypeI32}
	case token.Minus:
		fmt.Fprintf(&f.builder, "  %%%s = sub i32 %s, %s\n", out, left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: checker.TypeI32}
	case token.Star:
		fmt.Fprintf(&f.builder, "  %%%s = mul i32 %s, %s\n", out, left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: checker.TypeI32}
	case token.Slash:
		fmt.Fprintf(&f.builder, "  %%%s = sdiv i32 %s, %s\n", out, left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: checker.TypeI32}
	case token.EqualEqual:
		fmt.Fprintf(&f.builder, "  %%%s = icmp eq %s %s, %s\n", out, f.g.llvmType(left.typ), left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: checker.TypeBool}
	case token.BangEqual:
		fmt.Fprintf(&f.builder, "  %%%s = icmp ne %s %s, %s\n", out, f.g.llvmType(left.typ), left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: checker.TypeBool}
	default:
		panic("unsupported binary operator")
	}
}

func (f *functionEmitter) genCall(expr *ast.CallExpr) exprValue {
	sig := f.g.functions[expr.Name]
	args := make([]exprValue, 0, len(expr.Args))
	for _, arg := range expr.Args {
		args = append(args, f.genExpression(arg))
	}

	if sig.Builtin {
		switch expr.Name {
		case "print":
			ptr := f.newTemp("str.ptr")
			length := f.newTemp("str.len")
			fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 0\n", ptr, args[0].ref)
			fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 1\n", length, args[0].ref)
			fmt.Fprintf(&f.builder, "  call void @yar_print(ptr %%%s, i64 %%%s)\n", ptr, length)
			return exprValue{typ: checker.TypeVoid}
		case "print_int":
			fmt.Fprintf(&f.builder, "  call void @yar_print_int(i32 %s)\n", args[0].ref)
			return exprValue{typ: checker.TypeVoid}
		}
	}

	var llvmArgs []string
	for i, arg := range args {
		llvmArgs = append(llvmArgs, fmt.Sprintf("%s %s", f.g.llvmType(sig.Params[i]), arg.ref))
	}

	callType := f.g.llvmType(sig.Return)
	if sig.Errorable {
		callType = f.g.resultTypeName(sig.Return)
	}
	tmp := f.newTemp("call")
	fmt.Fprintf(&f.builder, "  %%%s = call %s %s(%s)\n", tmp, callType, f.g.functionName(expr.Name), strings.Join(llvmArgs, ", "))
	return exprValue{
		ref:       "%" + tmp,
		typ:       sig.Return,
		errorable: sig.Errorable,
	}
}

func (f *functionEmitter) genCatch(expr *ast.CatchExpr) exprValue {
	target := f.genExpression(expr.Target)
	catchLabel := f.newLabel("catch")
	okLabel := f.newLabel("catch.ok")
	isErr := f.newTemp("iserr")
	fmt.Fprintf(&f.builder, "  %%%s = extractvalue %s %s, 0\n", isErr, f.g.resultTypeName(target.typ), target.ref)
	fmt.Fprintf(&f.builder, "  br i1 %%%s, label %%%s, label %%%s\n", isErr, catchLabel, okLabel)
	f.terminated = true

	f.emitLabel(catchLabel)
	f.genBlock(expr.Block)
	if !f.terminated {
		f.builder.WriteString("  unreachable\n")
		f.terminated = true
	}

	f.emitLabel(okLabel)
	value := f.newTemp("ok")
	fmt.Fprintf(&f.builder, "  %%%s = extractvalue %s %s, 2\n", value, f.g.resultTypeName(target.typ), target.ref)
	return exprValue{ref: "%" + value, typ: target.typ}
}

func (f *functionEmitter) emitStringValue(value string) exprValue {
	global := f.g.stringConstant(value)
	ptrName := f.newTemp("str.ptr")
	fmt.Fprintf(&f.builder, "  %%%s = getelementptr inbounds [%d x i8], ptr @%s, i64 0, i64 0\n", ptrName, len([]byte(value))+1, global)
	tmp1 := f.newTemp("str")
	tmp2 := f.newTemp("str")
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %%yar.str zeroinitializer, ptr %%%s, 0\n", tmp1, ptrName)
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %%yar.str %%%s, i64 %d, 1\n", tmp2, tmp1, len([]byte(value)))
	return exprValue{ref: "%" + tmp2, typ: checker.TypeStr}
}

func (f *functionEmitter) emitSuccessVoid() string {
	tmp1 := f.newTemp("result")
	tmp2 := f.newTemp("result")
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s zeroinitializer, i1 0, 0\n", tmp1, f.g.resultTypeName(checker.TypeVoid))
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s %%%s, i32 0, 1\n", tmp2, f.g.resultTypeName(checker.TypeVoid), tmp1)
	return "%" + tmp2
}

func (f *functionEmitter) emitSuccessResult(typ checker.Type, value string) string {
	tmp1 := f.newTemp("result")
	tmp2 := f.newTemp("result")
	tmp3 := f.newTemp("result")
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s zeroinitializer, i1 0, 0\n", tmp1, f.g.resultTypeName(typ))
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s %%%s, i32 0, 1\n", tmp2, f.g.resultTypeName(typ), tmp1)
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s %%%s, %s %s, 2\n", tmp3, f.g.resultTypeName(typ), tmp2, f.g.llvmType(typ), value)
	return "%" + tmp3
}

func (f *functionEmitter) emitErrorResult(typ checker.Type, name string) string {
	code := f.g.info.ErrorCodes[name]
	tmp1 := f.newTemp("result")
	tmp2 := f.newTemp("result")
	if typ == checker.TypeVoid {
		fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s zeroinitializer, i1 1, 0\n", tmp1, f.g.resultTypeName(checker.TypeVoid))
		fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s %%%s, i32 %d, 1\n", tmp2, f.g.resultTypeName(checker.TypeVoid), tmp1, code)
		return "%" + tmp2
	}
	tmp3 := f.newTemp("result")
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s zeroinitializer, i1 1, 0\n", tmp1, f.g.resultTypeName(typ))
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s %%%s, i32 %d, 1\n", tmp2, f.g.resultTypeName(typ), tmp1, code)
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s %%%s, %s %s, 2\n", tmp3, f.g.resultTypeName(typ), tmp2, f.g.llvmType(typ), zeroValue(typ))
	return "%" + tmp3
}

func zeroValue(typ checker.Type) string {
	switch typ {
	case checker.TypeBool:
		return "0"
	case checker.TypeI32:
		return "0"
	case checker.TypeStr:
		return "zeroinitializer"
	default:
		return "0"
	}
}

func (f *functionEmitter) newTemp(prefix string) string {
	name := fmt.Sprintf("%s.%d", prefix, f.tempID)
	f.tempID++
	return name
}

func (f *functionEmitter) newLabel(prefix string) string {
	name := fmt.Sprintf("%s.%d", prefix, f.labelID)
	f.labelID++
	return name
}

func (f *functionEmitter) emitLabel(label string) {
	fmt.Fprintf(&f.builder, "%s:\n", label)
	f.terminated = false
}

func (f *functionEmitter) pushScope() {
	f.scopes = append(f.scopes, map[string]localSlot{})
}

func (f *functionEmitter) popScope() {
	f.scopes = f.scopes[:len(f.scopes)-1]
}

func (f *functionEmitter) bindLocal(name string, slot localSlot) {
	f.scopes[len(f.scopes)-1][name] = slot
}

func (f *functionEmitter) lookupLocal(name string) (localSlot, bool) {
	for i := len(f.scopes) - 1; i >= 0; i-- {
		if slot, ok := f.scopes[i][name]; ok {
			return slot, true
		}
	}
	return localSlot{}, false
}
