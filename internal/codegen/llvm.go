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
	literalNames   map[*ast.FunctionLiteralExpr]string
	literalQueue   []*ast.FunctionLiteralExpr
	emittedLits    map[*ast.FunctionLiteralExpr]bool
	nextLiteralID  int
	usedResultType map[checker.Type]bool
}

func Generate(program *ast.Program, info checker.Info) (string, error) {
	g := &Generator{
		program:        program,
		info:           info,
		functions:      builtins(),
		stringNames:    make(map[string]string),
		literalNames:   make(map[*ast.FunctionLiteralExpr]string),
		emittedLits:    make(map[*ast.FunctionLiteralExpr]bool),
		usedResultType: make(map[checker.Type]bool),
	}

	for _, sig := range info.Functions {
		g.functions[sig.FullName] = sig
		if sig.Errorable {
			g.usedResultType[sig.Return] = true
		}
	}

	for _, et := range info.ExprTypes {
		if et.Errorable {
			g.usedResultType[et.Base] = true
		}
	}
	for _, lit := range info.FunctionLiterals {
		if lit.Signature.Errorable {
			g.usedResultType[lit.Signature.Return] = true
		}
	}

	var b strings.Builder
	b.WriteString("; yar v0.2\n")
	b.WriteString("source_filename = \"yar\"\n\n")
	b.WriteString("%yar.str = type { ptr, i64 }\n")
	b.WriteString("%yar.slice = type { ptr, i32, i32 }\n")
	b.WriteString("%yar.closure = type { ptr, ptr }\n")

	structNames := make([]string, 0, len(info.Structs))
	for name := range info.Structs {
		structNames = append(structNames, name)
	}
	sort.Strings(structNames)
	for _, name := range structNames {
		info := info.Structs[name]
		fieldTypes := make([]string, 0, len(info.Fields))
		for _, field := range info.Fields {
			fieldTypes = append(fieldTypes, g.llvmType(field.Type))
		}
		fmt.Fprintf(&b, "%s = type { %s }\n", g.structTypeName(name), strings.Join(fieldTypes, ", "))
	}

	enumNames := make([]string, 0, len(info.Enums))
	for name := range info.Enums {
		enumNames = append(enumNames, name)
	}
	sort.Strings(enumNames)
	for _, name := range enumNames {
		fmt.Fprintf(&b, "%s = type %s\n", g.enumTypeName(name), g.enumTypeLiteral(info.Enums[name]))
	}

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
	g.writeRuntimeDecls(&b)
	b.WriteByte('\n')

	var functionIR []string
	for _, fn := range program.Functions {
		sig, ok := info.Functions[fn]
		if !ok {
			continue
		}
		if sig.HostIntrinsic {
			continue
		}
		emitter := newFunctionEmitter(g, fn, sig)
		functionIR = append(functionIR, emitter.emit())
	}

	for len(g.literalQueue) > 0 {
		lit := g.literalQueue[0]
		g.literalQueue = g.literalQueue[1:]
		if g.emittedLits[lit] {
			continue
		}
		g.emittedLits[lit] = true
		emitter := newFunctionLiteralEmitter(g, lit, g.info.FunctionLiterals[lit], g.literalNames[lit])
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

func (g *Generator) writeRuntimeDecls(b *strings.Builder) {
	b.WriteString("declare void @yar_print(ptr, i64)\n")
	b.WriteString("declare void @yar_print_int(i32)\n")
	b.WriteString("declare void @yar_panic(ptr, i64)\n")
	b.WriteString("declare void @yar_eprint(ptr, i64)\n")
	b.WriteString("declare ptr @yar_alloc(i64)\n")
	b.WriteString("declare ptr @yar_alloc_zeroed(i64)\n")
	b.WriteString("declare void @yar_trap_oom()\n")
	b.WriteString("declare void @yar_set_args(i32, ptr)\n")
	b.WriteString("declare void @yar_slice_index_check(i64, i64)\n")
	b.WriteString("declare void @yar_slice_range_check(i64, i64, i64)\n")
	b.WriteString("declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)\n")
	b.WriteString("declare i32 @yar_str_equal(ptr, i64, ptr, i64)\n")
	b.WriteString("declare %yar.str @yar_str_concat(ptr, i64, ptr, i64)\n")
	b.WriteString("declare void @yar_str_index_check(i64, i64)\n")
	b.WriteString("declare void @yar_process_args(ptr)\n")
	b.WriteString("declare i32 @yar_process_run(ptr, ptr)\n")
	b.WriteString("declare i32 @yar_process_run_inherit(ptr, ptr)\n")
	b.WriteString("declare i32 @yar_env_lookup(%yar.str, ptr)\n")
	b.WriteString("declare ptr @yar_map_new(i32, i32, i32)\n")
	b.WriteString("declare void @yar_map_set(ptr, ptr, ptr)\n")
	b.WriteString("declare i32 @yar_map_get(ptr, ptr, ptr)\n")
	b.WriteString("declare i32 @yar_map_has(ptr, ptr)\n")
	b.WriteString("declare void @yar_map_delete(ptr, ptr)\n")
	b.WriteString("declare i32 @yar_map_len(ptr)\n")
	b.WriteString("declare %yar.slice @yar_map_keys(ptr)\n")
	b.WriteString("declare %yar.str @yar_str_from_byte(i32)\n")
	b.WriteString("declare i32 @yar_fs_read_file(%yar.str, ptr)\n")
	b.WriteString("declare i32 @yar_fs_write_file(%yar.str, %yar.str)\n")
	b.WriteString("declare i32 @yar_fs_read_dir(%yar.str, ptr)\n")
	b.WriteString("declare i32 @yar_fs_stat(%yar.str, ptr)\n")
	b.WriteString("declare i32 @yar_fs_mkdir_all(%yar.str)\n")
	b.WriteString("declare i32 @yar_fs_remove_all(%yar.str)\n")
	b.WriteString("declare i32 @yar_fs_temp_dir(%yar.str, ptr)\n")
}

func builtins() map[string]checker.Signature {
	return map[string]checker.Signature{
		"print": {
			Name:     "print",
			FullName: "print",
			Params:   []checker.Type{checker.TypeStr},
			Return:   checker.TypeVoid,
			Builtin:  true,
		},
		"print_int": {
			Name:     "print_int",
			FullName: "print_int",
			Params:   []checker.Type{checker.TypeI32},
			Return:   checker.TypeVoid,
			Builtin:  true,
		},
		"panic": {
			Name:     "panic",
			FullName: "panic",
			Params:   []checker.Type{checker.TypeStr},
			Return:   checker.TypeNoReturn,
			Builtin:  true,
		},
		"chr": {
			Name:     "chr",
			FullName: "chr",
			Params:   []checker.Type{checker.TypeI32},
			Return:   checker.TypeStr,
			Builtin:  true,
		},
		"i32_to_i64": {
			Name:     "i32_to_i64",
			FullName: "i32_to_i64",
			Params:   []checker.Type{checker.TypeI32},
			Return:   checker.TypeI64,
			Builtin:  true,
		},
		"i64_to_i32": {
			Name:     "i64_to_i32",
			FullName: "i64_to_i32",
			Params:   []checker.Type{checker.TypeI64},
			Return:   checker.TypeI32,
			Builtin:  true,
		},
	}
}

func (g *Generator) emitMainWrapper() (string, error) {
	mainSig, ok := g.functions["main"]
	if !ok {
		return "", fmt.Errorf("missing main signature")
	}

	var b strings.Builder
	b.WriteString("define i32 @main(i32 %argc, ptr %argv) {\n")
	b.WriteString("entry:\n")
	b.WriteString("  call void @yar_set_args(i32 %argc, ptr %argv)\n")
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
	case checker.TypeNoReturn:
		return "void"
	case checker.TypeBool:
		return "i1"
	case checker.TypeI32:
		return "i32"
	case checker.TypeI64:
		return "i64"
	case checker.TypeStr:
		return "%yar.str"
	case checker.TypeError:
		return "i32"
	}

	if _, ok := checker.ParsePointerType(typ); ok {
		return "ptr"
	}
	if _, ok := checker.ParseMapType(typ); ok {
		return "ptr"
	}
	if _, ok := checker.ParseFunctionType(typ); ok {
		return "%yar.closure"
	}
	if array, ok := checker.ParseArrayType(typ); ok {
		return fmt.Sprintf("[%d x %s]", array.Len, g.llvmType(array.Elem))
	}
	if _, ok := checker.ParseSliceType(typ); ok {
		return "%yar.slice"
	}
	if _, ok := g.info.Enums[string(typ)]; ok {
		return g.enumTypeName(string(typ))
	}
	return g.structTypeName(string(typ))
}

func (g *Generator) structTypeName(name string) string {
	return "%yar.struct." + sanitizeLabel(name)
}

func (g *Generator) enumTypeName(name string) string {
	return "%yar.enum." + sanitizeLabel(name)
}

func (g *Generator) enumTypeLiteral(info checker.EnumInfo) string {
	words := g.enumPayloadWords(info)
	if words == 0 {
		return "{ i32 }"
	}
	return fmt.Sprintf("{ i32, [%d x i64] }", words)
}

func (g *Generator) resultTypeName(typ checker.Type) string {
	return "%yar.result." + sanitizeLabel(string(typ))
}

func (g *Generator) resultStructLiteral(typ checker.Type) string {
	if typ == checker.TypeVoid {
		return "{ i1, i32 }"
	}
	return fmt.Sprintf("{ i1, i32, %s }", g.llvmType(typ))
}

func (g *Generator) functionName(name string) string {
	full := "yar." + name
	if isPlainLLVMName(full) {
		return "@" + full
	}
	return "@\"" + full + "\""
}

func isPlainLLVMName(name string) bool {
	for i := 0; i < len(name); i++ {
		c := name[i]
		if (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') {
			continue
		}
		switch c {
		case '.', '_':
			continue
		default:
			return false
		}
	}
	return true
}

func (g *Generator) lookupEnumCaseType(name string) (checker.EnumInfo, checker.EnumCaseInfo, bool) {
	idx := strings.LastIndex(name, ".")
	if idx <= 0 {
		return checker.EnumInfo{}, checker.EnumCaseInfo{}, false
	}
	enumName := name[:idx]
	caseName := name[idx+1:]
	enumInfo, ok := g.info.Enums[enumName]
	if !ok {
		return checker.EnumInfo{}, checker.EnumCaseInfo{}, false
	}
	enumCase, _, ok := enumInfo.Case(caseName)
	if !ok {
		return checker.EnumInfo{}, checker.EnumCaseInfo{}, false
	}
	return enumInfo, enumCase, true
}

func (g *Generator) enumPayloadWords(info checker.EnumInfo) int {
	maxSize := 0
	for _, enumCase := range info.Cases {
		if enumCase.PayloadType == checker.TypeInvalid || enumCase.PayloadType == "" {
			continue
		}
		size := g.typeSize(enumCase.PayloadType)
		if size > maxSize {
			maxSize = size
		}
	}
	if maxSize == 0 {
		return 0
	}
	return (maxSize + 7) / 8
}

func (g *Generator) typeSize(typ checker.Type) int {
	switch typ {
	case checker.TypeBool:
		return 1
	case checker.TypeI32, checker.TypeError:
		return 4
	case checker.TypeI64:
		return 8
	case checker.TypeStr:
		return 16
	}

	if _, ok := checker.ParsePointerType(typ); ok {
		return 8
	}
	if _, ok := checker.ParseFunctionType(typ); ok {
		return 16
	}
	if array, ok := checker.ParseArrayType(typ); ok {
		return array.Len * g.typeSize(array.Elem)
	}
	if _, ok := checker.ParseSliceType(typ); ok {
		return 16
	}
	if _, ok := checker.ParseMapType(typ); ok {
		return 8
	}
	if info, ok := g.info.Structs[string(typ)]; ok {
		size := 0
		align := 1
		for _, field := range info.Fields {
			fieldAlign := g.typeAlign(field.Type)
			size = alignTo(size, fieldAlign)
			size += g.typeSize(field.Type)
			if fieldAlign > align {
				align = fieldAlign
			}
		}
		return alignTo(size, align)
	}
	if info, ok := g.info.Enums[string(typ)]; ok {
		words := g.enumPayloadWords(info)
		if words == 0 {
			return 4
		}
		return 8 + words*8
	}
	return 0
}

func (g *Generator) typeAlign(typ checker.Type) int {
	switch typ {
	case checker.TypeBool:
		return 1
	case checker.TypeI32, checker.TypeError:
		return 4
	case checker.TypeI64, checker.TypeStr:
		return 8
	}

	if _, ok := checker.ParsePointerType(typ); ok {
		return 8
	}
	if _, ok := checker.ParseFunctionType(typ); ok {
		return 8
	}
	if array, ok := checker.ParseArrayType(typ); ok {
		return g.typeAlign(array.Elem)
	}
	if _, ok := checker.ParseSliceType(typ); ok {
		return 8
	}
	if _, ok := checker.ParseMapType(typ); ok {
		return 8
	}
	if info, ok := g.info.Structs[string(typ)]; ok {
		align := 1
		for _, field := range info.Fields {
			fieldAlign := g.typeAlign(field.Type)
			if fieldAlign > align {
				align = fieldAlign
			}
		}
		return align
	}
	if info, ok := g.info.Enums[string(typ)]; ok {
		if g.enumPayloadWords(info) == 0 {
			return 4
		}
		return 8
	}
	return 1
}

func alignTo(size, align int) int {
	if align <= 1 {
		return size
	}
	rem := size % align
	if rem == 0 {
		return size
	}
	return size + align - rem
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

func mapKeyKind(keyType checker.Type) int {
	switch keyType {
	case checker.TypeBool:
		return 0
	case checker.TypeI32:
		return 1
	case checker.TypeI64:
		return 2
	case checker.TypeStr:
		return 3
	default:
		return 1
	}
}

func sanitizeLabel(name string) string {
	var b strings.Builder
	for i := 0; i < len(name); i++ {
		c := name[i]
		if (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') {
			b.WriteByte(c)
			continue
		}
		if c == '_' {
			b.WriteString("__")
			continue
		}
		fmt.Fprintf(&b, "_%02X", c)
	}
	return b.String()
}

type functionEmitter struct {
	g            *Generator
	fn           *ast.FunctionDecl
	literal      *ast.FunctionLiteralExpr
	name         string
	captures     []checker.CaptureInfo
	sig          checker.Signature
	builder      strings.Builder
	tempID       int
	labelID      int
	currentLabel string
	terminated   bool
	scopes       []map[string]localSlot
	loops        []loopContext
}

type localSlot struct {
	typ checker.Type
	ptr string
}

type loopContext struct {
	breakLabel    string
	continueLabel string
}

func newFunctionEmitter(g *Generator, fn *ast.FunctionDecl, sig checker.Signature) *functionEmitter {
	return &functionEmitter{
		g:      g,
		fn:     fn,
		name:   sig.FullName,
		sig:    sig,
		scopes: []map[string]localSlot{{}},
	}
}

func newFunctionLiteralEmitter(g *Generator, lit *ast.FunctionLiteralExpr, info checker.FunctionLiteralInfo, name string) *functionEmitter {
	return &functionEmitter{
		g:        g,
		literal:  lit,
		name:     name,
		captures: info.Captures,
		sig:      info.Signature,
		scopes:   []map[string]localSlot{{}},
	}
}

func functionParams(fn *ast.FunctionDecl) []ast.Param {
	params := make([]ast.Param, 0, len(fn.Params)+1)
	if fn.Receiver != nil {
		params = append(params, ast.Param{
			Name:    fn.Receiver.Name,
			NamePos: fn.Receiver.NamePos,
			Type:    fn.Receiver.Type,
		})
	}
	params = append(params, fn.Params...)
	return params
}

func (f *functionEmitter) body() *ast.BlockStmt {
	if f.fn != nil {
		return f.fn.Body
	}
	return f.literal.Body
}

func (f *functionEmitter) params() []ast.Param {
	if f.fn != nil {
		return functionParams(f.fn)
	}
	return append([]ast.Param(nil), f.literal.Params...)
}

func (f *functionEmitter) emit() string {
	retType := f.g.llvmType(f.sig.Return)
	if f.sig.Errorable {
		retType = f.g.resultTypeName(f.sig.Return)
	}

	fnParams := f.params()
	params := make([]string, 0, len(fnParams)+1)
	if f.literal != nil {
		params = append(params, "ptr %env")
	}
	for i := range fnParams {
		params = append(params, fmt.Sprintf("%s %%arg%d", f.g.llvmType(f.sig.Params[i]), i))
	}

	fmt.Fprintf(&f.builder, "define %s %s(%s) {\n", retType, f.g.functionName(f.name), strings.Join(params, ", "))
	f.emitLabel("entry")
	if f.literal != nil && len(f.captures) > 0 {
		envType := f.g.closureEnvTypeLiteral(f.captures)
		for i, capture := range f.captures {
			fieldPtr := f.newTemp("capture.ptr")
			fmt.Fprintf(&f.builder, "  %%%s = getelementptr inbounds %s, ptr %%env, i32 0, i32 %d\n", fieldPtr, envType, i)
			f.bindLocal(capture.Name, localSlot{typ: capture.Type, ptr: "%" + fieldPtr})
		}
	}
	for i, param := range fnParams {
		slot := f.newLocalSlot(f.sig.Params[i])
		fmt.Fprintf(&f.builder, "  store %s %%arg%d, ptr %s\n", f.g.llvmType(f.sig.Params[i]), i, slot)
		f.bindLocal(param.Name, localSlot{typ: f.sig.Params[i], ptr: slot})
	}

	f.genBlock(f.body())
	if !f.terminated {
		switch f.sig.Return {
		case checker.TypeVoid:
			if f.sig.Errorable {
				result := f.emitSuccessVoid()
				fmt.Fprintf(&f.builder, "  ret %s %s\n", f.g.resultTypeName(checker.TypeVoid), result)
			} else {
				f.builder.WriteString("  ret void\n")
			}
		case checker.TypeNoReturn:
			f.builder.WriteString("  unreachable\n")
		}
	}
	f.builder.WriteString("}\n")
	return f.builder.String()
}

func (f *functionEmitter) genBlock(block *ast.BlockStmt) {
	f.genBlockWithErrorBinding(block, "", "")
}

func (f *functionEmitter) genBlockWithErrorBinding(block *ast.BlockStmt, name, code string) {
	f.pushScope()
	defer f.popScope()
	if name != "" {
		slot := f.newTemp("slot")
		fmt.Fprintf(&f.builder, "  %%%s = alloca i32\n", slot)
		fmt.Fprintf(&f.builder, "  store i32 %s, ptr %%%s\n", code, slot)
		f.bindLocal(name, localSlot{typ: checker.TypeError, ptr: "%" + slot})
	}
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
		slot := f.newLocalSlot(value.typ)
		fmt.Fprintf(&f.builder, "  store %s %s, ptr %s\n", f.g.llvmType(value.typ), value.ref, slot)
		f.bindLocal(s.Name, localSlot{typ: value.typ, ptr: slot})
	case *ast.VarStmt:
		typ := f.localType(stmt)
		slot := f.newLocalSlot(typ)
		value := f.g.zeroValue(typ)
		if s.Value != nil {
			value = f.genExpression(s.Value).ref
		}
		fmt.Fprintf(&f.builder, "  store %s %s, ptr %s\n", f.g.llvmType(typ), value, slot)
		f.bindLocal(s.Name, localSlot{typ: typ, ptr: slot})
	case *ast.AssignStmt:
		if indexExpr, ok := s.Target.(*ast.IndexExpr); ok {
			innerType := f.exprType(indexExpr.Inner)
			if mapType, ok := checker.ParseMapType(innerType); ok {
				mapValue := f.genExpression(indexExpr.Inner)
				keyValue := f.genExpression(indexExpr.Index)
				value := f.genExpression(s.Value)
				f.emitMapSet(mapValue.ref, mapType.Key, keyValue, mapType.Value, value)
				return
			}
		}
		ptr, typ := f.addressOfTarget(s.Target)
		value := f.genExpression(s.Value)
		fmt.Fprintf(&f.builder, "  store %s %s, ptr %s\n", f.g.llvmType(typ), value.ref, ptr)
	case *ast.IfStmt:
		f.genIf(s)
	case *ast.ForStmt:
		f.genFor(s)
	case *ast.MatchStmt:
		f.genMatch(s)
	case *ast.BreakStmt:
		if len(f.loops) == 0 {
			return
		}
		fmt.Fprintf(&f.builder, "  br label %%%s\n", f.loops[len(f.loops)-1].breakLabel)
		f.terminated = true
	case *ast.ContinueStmt:
		if len(f.loops) == 0 {
			return
		}
		fmt.Fprintf(&f.builder, "  br label %%%s\n", f.loops[len(f.loops)-1].continueLabel)
		f.terminated = true
	case *ast.ReturnStmt:
		f.genReturn(s)
	case *ast.ExprStmt:
		value := f.genExpression(s.Expr)
		if value.typ == checker.TypeNoReturn {
			f.terminated = true
		}
	default:
		panic("unsupported statement")
	}
}

func (f *functionEmitter) genIf(stmt *ast.IfStmt) {
	cond := f.genExpression(stmt.Cond)
	thenLabel := f.newLabel("if.then")
	endLabel := f.newLabel("if.end")

	if stmt.Else == nil {
		fmt.Fprintf(&f.builder, "  br i1 %s, label %%%s, label %%%s\n", cond.ref, thenLabel, endLabel)
		f.terminated = true
		f.emitLabel(thenLabel)
		f.genBlock(stmt.Then)
		if !f.terminated {
			fmt.Fprintf(&f.builder, "  br label %%%s\n", endLabel)
			f.terminated = true
		}
		f.emitLabel(endLabel)
		return
	}

	elseLabel := f.newLabel("if.else")
	fmt.Fprintf(&f.builder, "  br i1 %s, label %%%s, label %%%s\n", cond.ref, thenLabel, elseLabel)
	f.terminated = true

	f.emitLabel(thenLabel)
	f.genBlock(stmt.Then)
	thenTerminated := f.terminated
	if !thenTerminated {
		fmt.Fprintf(&f.builder, "  br label %%%s\n", endLabel)
		f.terminated = true
	}

	f.emitLabel(elseLabel)
	f.genStatement(stmt.Else)
	elseTerminated := f.terminated
	if !elseTerminated {
		fmt.Fprintf(&f.builder, "  br label %%%s\n", endLabel)
		f.terminated = true
	}

	if thenTerminated && elseTerminated {
		f.terminated = true
		return
	}
	f.emitLabel(endLabel)
}

func (f *functionEmitter) genFor(stmt *ast.ForStmt) {
	f.pushScope()
	defer f.popScope()

	if stmt.Init != nil {
		f.genStatement(stmt.Init)
		if f.terminated {
			return
		}
	}

	condLabel := f.newLabel("for.cond")
	bodyLabel := f.newLabel("for.body")
	endLabel := f.newLabel("for.end")
	continueLabel := condLabel
	postLabel := ""
	if stmt.Post != nil {
		postLabel = f.newLabel("for.post")
		continueLabel = postLabel
	}

	fmt.Fprintf(&f.builder, "  br label %%%s\n", condLabel)
	f.terminated = true

	f.emitLabel(condLabel)
	if stmt.Cond != nil {
		cond := f.genExpression(stmt.Cond)
		fmt.Fprintf(&f.builder, "  br i1 %s, label %%%s, label %%%s\n", cond.ref, bodyLabel, endLabel)
	} else {
		fmt.Fprintf(&f.builder, "  br label %%%s\n", bodyLabel)
	}
	f.terminated = true

	f.loops = append(f.loops, loopContext{
		breakLabel:    endLabel,
		continueLabel: continueLabel,
	})

	f.emitLabel(bodyLabel)
	f.genBlock(stmt.Body)
	if !f.terminated {
		fmt.Fprintf(&f.builder, "  br label %%%s\n", continueLabel)
		f.terminated = true
	}

	if stmt.Post != nil {
		f.emitLabel(postLabel)
		f.genStatement(stmt.Post)
		if !f.terminated {
			fmt.Fprintf(&f.builder, "  br label %%%s\n", condLabel)
			f.terminated = true
		}
	}

	f.loops = f.loops[:len(f.loops)-1]
	f.emitLabel(endLabel)
}

func (f *functionEmitter) genMatch(stmt *ast.MatchStmt) {
	enumType := f.exprType(stmt.Value)
	enumInfo, ok := f.g.info.Enums[string(enumType)]
	if !ok {
		panic("match requires enum metadata")
	}

	value := f.genExpression(stmt.Value)
	slot := f.newTemp("match.slot")
	fmt.Fprintf(&f.builder, "  %%%s = alloca %s\n", slot, f.g.llvmType(enumType))
	fmt.Fprintf(&f.builder, "  store %s %s, ptr %%%s\n", f.g.llvmType(enumType), value.ref, slot)

	tagPtr := f.newTemp("match.tag.ptr")
	fmt.Fprintf(&f.builder, "  %%%s = getelementptr inbounds %s, ptr %%%s, i32 0, i32 0\n", tagPtr, f.g.llvmType(enumType), slot)
	tag := f.newTemp("match.tag")
	fmt.Fprintf(&f.builder, "  %%%s = load i32, ptr %%%s\n", tag, tagPtr)

	defaultLabel := f.newLabel("match.unreachable")
	endLabel := f.newLabel("match.end")
	armLabels := make([]string, len(stmt.Arms))
	fmt.Fprintf(&f.builder, "  switch i32 %%%s, label %%%s [\n", tag, defaultLabel)
	for i, arm := range stmt.Arms {
		enumCase, _, ok := enumInfo.Case(arm.CaseName)
		if !ok {
			panic("missing enum case metadata")
		}
		armLabels[i] = f.newLabel("match.case")
		fmt.Fprintf(&f.builder, "    i32 %d, label %%%s\n", enumCase.Tag, armLabels[i])
	}
	f.builder.WriteString("  ]\n")
	f.terminated = true

	needsEnd := false
	for i, arm := range stmt.Arms {
		enumCase, _, _ := enumInfo.Case(arm.CaseName)
		f.emitLabel(armLabels[i])
		if arm.BindName != "" && !arm.BindIgnore && enumCase.PayloadType != "" {
			f.pushScope()
			payloadValue := f.loadValue(f.enumPayloadPtr("%"+slot, enumType), enumCase.PayloadType)
			bindSlot := f.newLocalSlot(enumCase.PayloadType)
			fmt.Fprintf(&f.builder, "  store %s %s, ptr %s\n", f.g.llvmType(enumCase.PayloadType), payloadValue.ref, bindSlot)
			f.bindLocal(arm.BindName, localSlot{typ: enumCase.PayloadType, ptr: bindSlot})
			f.genBlock(arm.Body)
			f.popScope()
		} else {
			f.genBlock(arm.Body)
		}
		if !f.terminated {
			fmt.Fprintf(&f.builder, "  br label %%%s\n", endLabel)
			f.terminated = true
			needsEnd = true
		}
	}

	f.emitLabel(defaultLabel)
	f.builder.WriteString("  unreachable\n")
	f.terminated = true

	if needsEnd {
		f.emitLabel(endLabel)
		return
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
		if exprType, ok := f.g.info.ExprTypes[stmt.Value]; ok && exprType.Errorable {
			fmt.Fprintf(&f.builder, "  ret %s %s\n", f.g.resultTypeName(f.sig.Return), value.ref)
			f.terminated = true
			return
		}
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

	if errLit, ok := stmt.Value.(*ast.ErrorLiteral); ok {
		code := f.g.info.ErrorCodes[errLit.Name]
		fmt.Fprintf(&f.builder, "  ret i32 %d\n", code)
		f.terminated = true
		return
	}

	value := f.genExpression(stmt.Value)
	fmt.Fprintf(&f.builder, "  ret %s %s\n", f.g.llvmType(value.typ), value.ref)
	f.terminated = true
}

type exprValue struct {
	ref string
	typ checker.Type
}

func (f *functionEmitter) genExpression(expr ast.Expression) exprValue {
	switch e := expr.(type) {
	case *ast.IdentExpr:
		slot, _ := f.lookupLocal(e.Name)
		tmp := f.newTemp("load")
		fmt.Fprintf(&f.builder, "  %%%s = load %s, ptr %s\n", tmp, f.g.llvmType(slot.typ), slot.ptr)
		return exprValue{ref: "%" + tmp, typ: slot.typ}
	case *ast.IntLiteral:
		literalType := f.exprType(expr)
		if literalType == checker.TypeUntypedInt {
			literalType = checker.TypeI32
		}
		return exprValue{ref: fmt.Sprintf("%d", e.Value), typ: literalType}
	case *ast.BoolLiteral:
		if e.Value {
			return exprValue{ref: "1", typ: checker.TypeBool}
		}
		return exprValue{ref: "0", typ: checker.TypeBool}
	case *ast.NilLiteral:
		return exprValue{ref: "null", typ: f.exprType(expr)}
	case *ast.StringLiteral:
		return f.emitStringValue(e.Value)
	case *ast.GroupExpr:
		return f.genExpression(e.Inner)
	case *ast.FunctionLiteralExpr:
		return f.genFunctionLiteral(e)
	case *ast.UnaryExpr:
		return f.genUnary(expr, e)
	case *ast.PropagateExpr:
		return f.genPropagate(e)
	case *ast.HandleExpr:
		return f.genHandle(e)
	case *ast.BinaryExpr:
		return f.genBinary(e)
	case *ast.SelectorExpr:
		if value, ok := f.genEnumCaseSelector(e); ok {
			return value
		}
		return f.genSelector(e)
	case *ast.IndexExpr:
		return f.genIndex(e)
	case *ast.SliceExpr:
		return f.genSlice(expr, e)
	case *ast.StructLiteralExpr:
		if value, ok := f.genEnumCaseLiteral(expr, e); ok {
			return value
		}
		return f.genStructLiteral(expr, e)
	case *ast.ArrayLiteralExpr:
		return f.genArrayLiteral(expr, e)
	case *ast.SliceLiteralExpr:
		return f.genSliceLiteral(expr, e)
	case *ast.MapLiteralExpr:
		return f.genMapLiteral(expr, e)
	case *ast.CallExpr:
		return f.genCall(e)
	default:
		panic(fmt.Sprintf("unsupported expression %T", expr))
	}
}

func (f *functionEmitter) genFunctionLiteral(lit *ast.FunctionLiteralExpr) exprValue {
	name := f.g.queueFunctionLiteral(lit)
	info := f.g.info.FunctionLiterals[lit]
	envRef := "null"
	if len(info.Captures) > 0 {
		envRef = f.emitAllocBytes(fmt.Sprintf("%d", f.g.closureEnvSize(info.Captures)), false)
		envType := f.g.closureEnvTypeLiteral(info.Captures)
		for i, capture := range info.Captures {
			slot, ok := f.lookupLocal(capture.Name)
			if !ok {
				panic("missing captured local " + capture.Name)
			}
			value := f.loadValue(slot.ptr, slot.typ)
			fieldPtr := f.newTemp("closure.env.ptr")
			fmt.Fprintf(&f.builder, "  %%%s = getelementptr inbounds %s, ptr %s, i32 0, i32 %d\n", fieldPtr, envType, envRef, i)
			fmt.Fprintf(&f.builder, "  store %s %s, ptr %%%s\n", f.g.llvmType(capture.Type), value.ref, fieldPtr)
		}
	}
	return f.emitClosureValue(f.g.functionName(name), envRef, f.exprType(lit))
}

func (f *functionEmitter) genUnary(expr ast.Expression, unary *ast.UnaryExpr) exprValue {
	switch unary.Operator {
	case token.Amp:
		ptr, typ := f.addressOfExpr(unary.Inner)
		return exprValue{ref: ptr, typ: checker.MakePointerType(typ)}
	case token.Star:
		value := f.genExpression(unary.Inner)
		pointer, _ := checker.ParsePointerType(value.typ)
		load := f.newTemp("deref")
		fmt.Fprintf(&f.builder, "  %%%s = load %s, ptr %s\n", load, f.g.llvmType(pointer.Elem), value.ref)
		return exprValue{ref: "%" + load, typ: pointer.Elem}
	case token.Minus:
		value := f.genExpression(unary.Inner)
		out := f.newTemp("neg")
		typ := f.exprType(expr)
		if typ == checker.TypeUntypedInt {
			typ = checker.TypeI32
		}
		fmt.Fprintf(&f.builder, "  %%%s = sub %s 0, %s\n", out, f.g.llvmType(typ), value.ref)
		return exprValue{ref: "%" + out, typ: typ}
	case token.Bang:
		value := f.genExpression(unary.Inner)
		out := f.newTemp("not")
		fmt.Fprintf(&f.builder, "  %%%s = xor i1 %s, true\n", out, value.ref)
		return exprValue{ref: "%" + out, typ: checker.TypeBool}
	default:
		panic("unsupported unary operator")
	}
}

func (f *functionEmitter) genEnumCaseSelector(expr *ast.SelectorExpr) (exprValue, bool) {
	inner, ok := expr.Inner.(*ast.IdentExpr)
	if !ok {
		return exprValue{}, false
	}
	enumInfo, ok := f.g.info.Enums[inner.Name]
	if !ok {
		return exprValue{}, false
	}
	enumCase, _, ok := enumInfo.Case(expr.Name)
	if !ok {
		return exprValue{}, false
	}
	return f.emitEnumValue(checker.Type(enumInfo.Name), enumCase, nil), true
}

func (f *functionEmitter) genEnumCaseLiteral(expr ast.Expression, lit *ast.StructLiteralExpr) (exprValue, bool) {
	enumInfo, enumCase, ok := f.g.lookupEnumCaseType(lit.Type.String())
	if !ok {
		return exprValue{}, false
	}
	payload := f.emitStructValue(enumCase.PayloadType, lit.Fields)
	value := f.emitEnumValue(checker.Type(enumInfo.Name), enumCase, &exprValue{ref: payload, typ: enumCase.PayloadType})
	value.typ = f.exprType(expr)
	return value, true
}

func (f *functionEmitter) emitStructValue(typ checker.Type, fields []ast.StructLiteralField) string {
	if len(fields) == 0 {
		return "zeroinitializer"
	}

	value := "zeroinitializer"
	info := f.g.info.Structs[string(typ)]
	for _, fieldInit := range fields {
		field, index, _ := info.Field(fieldInit.Name)
		fieldValue := f.genExpression(fieldInit.Value)
		next := f.newTemp("struct")
		fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s %s, %s %s, %d\n", next, f.g.llvmType(typ), value, f.g.llvmType(field.Type), fieldValue.ref, index)
		value = "%" + next
	}
	return value
}

func (f *functionEmitter) genSelector(expr *ast.SelectorExpr) exprValue {
	basePtr, baseType := f.addressOfAggregateExpr(expr.Inner)
	info := f.g.info.Structs[string(baseType)]
	field, index, _ := info.Field(expr.Name)
	ptr := f.newTemp("field.ptr")
	fmt.Fprintf(&f.builder, "  %%%s = getelementptr inbounds %s, ptr %s, i32 0, i32 %d\n", ptr, f.g.llvmType(baseType), basePtr, index)
	load := f.newTemp("field")
	fmt.Fprintf(&f.builder, "  %%%s = load %s, ptr %%%s\n", load, f.g.llvmType(field.Type), ptr)
	return exprValue{ref: "%" + load, typ: field.Type}
}

func (f *functionEmitter) genIndex(expr *ast.IndexExpr) exprValue {
	innerType := f.exprType(expr.Inner)
	if mapType, ok := checker.ParseMapType(innerType); ok {
		return f.genMapLookup(expr, mapType)
	}

	if innerType == checker.TypeStr {
		return f.genStringIndex(expr)
	}

	basePtr, baseType := f.addressOfAggregateExpr(expr.Inner)
	if array, ok := checker.ParseArrayType(baseType); ok {
		index := f.genExpression(expr.Index)
		ptr := f.newTemp("elem.ptr")
		fmt.Fprintf(&f.builder, "  %%%s = getelementptr inbounds %s, ptr %s, i32 0, %s %s\n", ptr, f.g.llvmType(baseType), basePtr, f.g.llvmType(index.typ), index.ref)
		load := f.newTemp("elem")
		fmt.Fprintf(&f.builder, "  %%%s = load %s, ptr %%%s\n", load, f.g.llvmType(array.Elem), ptr)
		return exprValue{ref: "%" + load, typ: array.Elem}
	}

	slice, _ := checker.ParseSliceType(baseType)
	ptr := f.addressOfSliceElement(basePtr, slice.Elem, f.genExpression(expr.Index))
	load := f.newTemp("elem")
	fmt.Fprintf(&f.builder, "  %%%s = load %s, ptr %s\n", load, f.g.llvmType(slice.Elem), ptr)
	return exprValue{ref: "%" + load, typ: slice.Elem}
}

func (f *functionEmitter) genStringIndex(expr *ast.IndexExpr) exprValue {
	strValue := f.genExpression(expr.Inner)
	index := f.genExpression(expr.Index)

	strPtr := f.newTemp("str.ptr")
	strLen := f.newTemp("str.len")
	fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 0\n", strPtr, strValue.ref)
	fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 1\n", strLen, strValue.ref)

	index64 := f.intToI64(index.ref, index.typ)
	fmt.Fprintf(&f.builder, "  call void @yar_str_index_check(i64 %s, i64 %%%s)\n", index64, strLen)

	bytePtr := f.newTemp("str.byte.ptr")
	fmt.Fprintf(&f.builder, "  %%%s = getelementptr i8, ptr %%%s, i64 %s\n", bytePtr, strPtr, index64)
	byteVal := f.newTemp("str.byte")
	fmt.Fprintf(&f.builder, "  %%%s = load i8, ptr %%%s\n", byteVal, bytePtr)
	result := f.newTemp("str.byte.i32")
	fmt.Fprintf(&f.builder, "  %%%s = zext i8 %%%s to i32\n", result, byteVal)
	return exprValue{ref: "%" + result, typ: checker.TypeI32}
}

func (f *functionEmitter) genStringSlice(_ ast.Expression, sliceExpr *ast.SliceExpr) exprValue {
	strValue := f.genExpression(sliceExpr.Inner)

	strPtr := f.newTemp("str.ptr")
	strLen := f.newTemp("str.len")
	fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 0\n", strPtr, strValue.ref)
	fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 1\n", strLen, strValue.ref)

	start := f.genExpression(sliceExpr.Start)
	end := f.genExpression(sliceExpr.End)

	start64 := f.intToI64(start.ref, start.typ)
	end64 := f.intToI64(end.ref, end.typ)

	fmt.Fprintf(&f.builder, "  call void @yar_slice_range_check(i64 %s, i64 %s, i64 %%%s)\n", start64, end64, strLen)

	newPtr := f.newTemp("str.slice.ptr")
	fmt.Fprintf(&f.builder, "  %%%s = getelementptr i8, ptr %%%s, i64 %s\n", newPtr, strPtr, start64)
	newLen := f.newTemp("str.slice.len")
	fmt.Fprintf(&f.builder, "  %%%s = sub i64 %s, %s\n", newLen, end64, start64)

	result := f.newTemp("str.slice")
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %%yar.str undef, ptr %%%s, 0\n", result, newPtr)
	result2 := f.newTemp("str.slice")
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %%yar.str %%%s, i64 %%%s, 1\n", result2, result, newLen)
	return exprValue{ref: "%" + result2, typ: checker.TypeStr}
}

func (f *functionEmitter) genMapLookup(expr *ast.IndexExpr, mapType checker.MapType) exprValue {
	mapValue := f.genExpression(expr.Inner)
	keyValue := f.genExpression(expr.Index)

	keySlot := f.newTemp("map.key.slot")
	fmt.Fprintf(&f.builder, "  %%%s = alloca %s\n", keySlot, f.g.llvmType(mapType.Key))
	fmt.Fprintf(&f.builder, "  store %s %s, ptr %%%s\n", f.g.llvmType(mapType.Key), keyValue.ref, keySlot)

	valSlot := f.newTemp("map.val.slot")
	fmt.Fprintf(&f.builder, "  %%%s = alloca %s\n", valSlot, f.g.llvmType(mapType.Value))

	found := f.newTemp("map.found")
	fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_map_get(ptr %s, ptr %%%s, ptr %%%s)\n", found, mapValue.ref, keySlot, valSlot)

	isFound := f.newTemp("map.is_found")
	fmt.Fprintf(&f.builder, "  %%%s = icmp ne i32 %%%s, 0\n", isFound, found)

	okLabel := f.newLabel("map.ok")
	missLabel := f.newLabel("map.miss")
	endLabel := f.newLabel("map.end")
	fmt.Fprintf(&f.builder, "  br i1 %%%s, label %%%s, label %%%s\n", isFound, okLabel, missLabel)
	f.terminated = true

	f.emitLabel(okLabel)
	okVal := f.newTemp("map.val")
	fmt.Fprintf(&f.builder, "  %%%s = load %s, ptr %%%s\n", okVal, f.g.llvmType(mapType.Value), valSlot)
	okResult := f.emitSuccessResult(mapType.Value, "%"+okVal)
	fmt.Fprintf(&f.builder, "  br label %%%s\n", endLabel)
	f.terminated = true
	okResultLabel := f.currentLabel

	f.emitLabel(missLabel)
	missingKeyCode := f.g.info.ErrorCodes["MissingKey"]
	missResult := f.emitErrorCodeResult(mapType.Value, fmt.Sprintf("%d", missingKeyCode))
	fmt.Fprintf(&f.builder, "  br label %%%s\n", endLabel)
	f.terminated = true
	missResultLabel := f.currentLabel

	f.emitLabel(endLabel)
	result := f.newTemp("map.result")
	resultType := f.g.resultTypeName(mapType.Value)
	fmt.Fprintf(&f.builder, "  %%%s = phi %s [%s, %%%s], [%s, %%%s]\n", result, resultType, okResult, okResultLabel, missResult, missResultLabel)

	return exprValue{ref: "%" + result, typ: mapType.Value}
}

func (f *functionEmitter) genSlice(expr ast.Expression, sliceExpr *ast.SliceExpr) exprValue {
	if f.exprType(sliceExpr.Inner) == checker.TypeStr {
		return f.genStringSlice(expr, sliceExpr)
	}

	basePtr, baseType := f.addressOfAggregateExpr(sliceExpr.Inner)
	sliceType, _ := checker.ParseSliceType(baseType)
	sliceValue := f.loadValue(basePtr, baseType)
	dataPtr := f.extractSliceField(sliceValue.ref, 0, "slice.data")
	length := f.extractSliceField(sliceValue.ref, 1, "slice.len")
	capacity := f.extractSliceField(sliceValue.ref, 2, "slice.cap")
	length64 := f.intToI64(length.ref, length.typ)
	capacity64 := f.intToI64(capacity.ref, capacity.typ)
	start := f.genExpression(sliceExpr.Start)
	end := f.genExpression(sliceExpr.End)
	start64 := f.intToI64(start.ref, start.typ)
	end64 := f.intToI64(end.ref, end.typ)
	fmt.Fprintf(&f.builder, "  call void @yar_slice_range_check(i64 %s, i64 %s, i64 %s)\n", start64, end64, length64)
	newPtr := f.newTemp("slice.ptr")
	fmt.Fprintf(&f.builder, "  %%%s = getelementptr %s, ptr %s, i64 %s\n", newPtr, f.g.llvmType(sliceType.Elem), dataPtr.ref, start64)
	newLen64 := f.newTemp("slice.len64")
	fmt.Fprintf(&f.builder, "  %%%s = sub i64 %s, %s\n", newLen64, end64, start64)
	newLen := f.newTemp("slice.len")
	fmt.Fprintf(&f.builder, "  %%%s = trunc i64 %%%s to i32\n", newLen, newLen64)
	newCap64 := f.newTemp("slice.cap64")
	fmt.Fprintf(&f.builder, "  %%%s = sub i64 %s, %s\n", newCap64, capacity64, start64)
	newCap := f.newTemp("slice.cap")
	fmt.Fprintf(&f.builder, "  %%%s = trunc i64 %%%s to i32\n", newCap, newCap64)
	ref := f.emitSliceValue("%"+newPtr, "%"+newLen, "%"+newCap)
	return exprValue{ref: ref, typ: f.exprType(expr)}
}

func (f *functionEmitter) genStructLiteral(expr ast.Expression, lit *ast.StructLiteralExpr) exprValue {
	typ := f.exprType(expr)
	return exprValue{ref: f.emitStructValue(typ, lit.Fields), typ: typ}
}

func (f *functionEmitter) genArrayLiteral(expr ast.Expression, lit *ast.ArrayLiteralExpr) exprValue {
	typ := f.exprType(expr)
	array, _ := checker.ParseArrayType(typ)
	if len(lit.Elements) == 0 {
		return exprValue{ref: "zeroinitializer", typ: typ}
	}

	value := "zeroinitializer"
	for i, element := range lit.Elements {
		elementValue := f.genExpression(element)
		next := f.newTemp("array")
		fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s %s, %s %s, %d\n", next, f.g.llvmType(typ), value, f.g.llvmType(array.Elem), elementValue.ref, i)
		value = "%" + next
	}
	return exprValue{ref: value, typ: typ}
}

func (f *functionEmitter) genSliceLiteral(expr ast.Expression, lit *ast.SliceLiteralExpr) exprValue {
	typ := f.exprType(expr)
	sliceType, _ := checker.ParseSliceType(typ)
	if len(lit.Elements) == 0 {
		return exprValue{ref: "zeroinitializer", typ: typ}
	}
	dataPtr := f.emitAllocBytes(f.emitArrayAllocSize(sliceType.Elem, len(lit.Elements)), false)
	for i, element := range lit.Elements {
		elementValue := f.genExpression(element)
		ptr := f.newTemp("slice.elem.ptr")
		fmt.Fprintf(&f.builder, "  %%%s = getelementptr %s, ptr %s, i64 %d\n", ptr, f.g.llvmType(sliceType.Elem), dataPtr, i)
		fmt.Fprintf(&f.builder, "  store %s %s, ptr %%%s\n", f.g.llvmType(sliceType.Elem), elementValue.ref, ptr)
	}
	length := fmt.Sprintf("%d", len(lit.Elements))
	ref := f.emitSliceValue(dataPtr, length, length)
	return exprValue{ref: ref, typ: typ}
}

func (f *functionEmitter) genMapLiteral(expr ast.Expression, lit *ast.MapLiteralExpr) exprValue {
	typ := f.exprType(expr)
	mapType, _ := checker.ParseMapType(typ)

	keyKind := mapKeyKind(mapType.Key)
	keySize := f.g.typeSize(mapType.Key)
	valueSize := f.g.typeSize(mapType.Value)

	mapPtr := f.newTemp("map.new")
	fmt.Fprintf(&f.builder, "  %%%s = call ptr @yar_map_new(i32 %d, i32 %d, i32 %d)\n", mapPtr, keyKind, keySize, valueSize)

	for _, pair := range lit.Pairs {
		keyValue := f.genExpression(pair.Key)
		valValue := f.genExpression(pair.Value)
		f.emitMapSet("%"+mapPtr, mapType.Key, keyValue, mapType.Value, valValue)
	}

	return exprValue{ref: "%" + mapPtr, typ: typ}
}

func (f *functionEmitter) emitMapSet(mapRef string, keyType checker.Type, key exprValue, valueType checker.Type, value exprValue) {
	keySlot := f.newTemp("map.key.slot")
	fmt.Fprintf(&f.builder, "  %%%s = alloca %s\n", keySlot, f.g.llvmType(keyType))
	fmt.Fprintf(&f.builder, "  store %s %s, ptr %%%s\n", f.g.llvmType(keyType), key.ref, keySlot)

	valSlot := f.newTemp("map.val.slot")
	fmt.Fprintf(&f.builder, "  %%%s = alloca %s\n", valSlot, f.g.llvmType(valueType))
	fmt.Fprintf(&f.builder, "  store %s %s, ptr %%%s\n", f.g.llvmType(valueType), value.ref, valSlot)

	fmt.Fprintf(&f.builder, "  call void @yar_map_set(ptr %s, ptr %%%s, ptr %%%s)\n", mapRef, keySlot, valSlot)
}

func (f *functionEmitter) genPropagate(expr *ast.PropagateExpr) exprValue {
	innerType := f.exprInfo(expr.Inner)
	if innerType.Errorable {
		result := f.genExpression(expr.Inner)
		errFlag := f.newTemp("propagate.is_err")
		errLabel := f.newLabel("propagate.err")
		okLabel := f.newLabel("propagate.ok")
		fmt.Fprintf(&f.builder, "  %%%s = extractvalue %s %s, 0\n", errFlag, f.g.resultTypeName(innerType.Base), result.ref)
		fmt.Fprintf(&f.builder, "  br i1 %%%s, label %%%s, label %%%s\n", errFlag, errLabel, okLabel)
		f.terminated = true

		errCode := f.newTemp("propagate.err_code")
		f.emitLabel(errLabel)
		fmt.Fprintf(&f.builder, "  %%%s = extractvalue %s %s, 1\n", errCode, f.g.resultTypeName(innerType.Base), result.ref)
		f.emitPropagatedError("%" + errCode)

		f.emitLabel(okLabel)
		if innerType.Base == checker.TypeVoid {
			return exprValue{typ: checker.TypeVoid}
		}
		success := f.newTemp("propagate.value")
		fmt.Fprintf(&f.builder, "  %%%s = extractvalue %s %s, 2\n", success, f.g.resultTypeName(innerType.Base), result.ref)
		return exprValue{ref: "%" + success, typ: innerType.Base}
	}

	errValue := f.genExpression(expr.Inner)
	isErr := f.newTemp("propagate.is_err")
	errLabel := f.newLabel("propagate.err")
	okLabel := f.newLabel("propagate.ok")
	fmt.Fprintf(&f.builder, "  %%%s = icmp ne i32 %s, 0\n", isErr, errValue.ref)
	fmt.Fprintf(&f.builder, "  br i1 %%%s, label %%%s, label %%%s\n", isErr, errLabel, okLabel)
	f.terminated = true

	f.emitLabel(errLabel)
	f.emitPropagatedError(errValue.ref)

	f.emitLabel(okLabel)
	return exprValue{typ: checker.TypeVoid}
}

func (f *functionEmitter) genHandle(expr *ast.HandleExpr) exprValue {
	innerType := f.exprInfo(expr.Inner)
	if innerType.Errorable {
		result := f.genExpression(expr.Inner)
		errFlag := f.newTemp("handle.is_err")
		errLabel := f.newLabel("handle.err")
		fmt.Fprintf(&f.builder, "  %%%s = extractvalue %s %s, 0\n", errFlag, f.g.resultTypeName(innerType.Base), result.ref)
		if innerType.Base != checker.TypeVoid {
			okLabel := f.newLabel("handle.ok")
			fmt.Fprintf(&f.builder, "  br i1 %%%s, label %%%s, label %%%s\n", errFlag, errLabel, okLabel)
			f.terminated = true

			errCode := f.newTemp("handle.err_code")
			f.emitLabel(errLabel)
			fmt.Fprintf(&f.builder, "  %%%s = extractvalue %s %s, 1\n", errCode, f.g.resultTypeName(innerType.Base), result.ref)
			f.genBlockWithErrorBinding(expr.Handler, expr.ErrName, "%"+errCode)
			if !f.terminated {
				panic("value handler must terminate")
			}

			f.emitLabel(okLabel)
			success := f.newTemp("handle.value")
			fmt.Fprintf(&f.builder, "  %%%s = extractvalue %s %s, 2\n", success, f.g.resultTypeName(innerType.Base), result.ref)
			return exprValue{ref: "%" + success, typ: innerType.Base}
		}

		continueLabel := f.newLabel("handle.cont")
		fmt.Fprintf(&f.builder, "  br i1 %%%s, label %%%s, label %%%s\n", errFlag, errLabel, continueLabel)
		f.terminated = true

		errCode := f.newTemp("handle.err_code")
		f.emitLabel(errLabel)
		fmt.Fprintf(&f.builder, "  %%%s = extractvalue %s %s, 1\n", errCode, f.g.resultTypeName(innerType.Base), result.ref)
		f.genBlockWithErrorBinding(expr.Handler, expr.ErrName, "%"+errCode)
		if !f.terminated {
			fmt.Fprintf(&f.builder, "  br label %%%s\n", continueLabel)
			f.terminated = true
		}

		f.emitLabel(continueLabel)
		return exprValue{typ: checker.TypeVoid}
	}

	errValue := f.genExpression(expr.Inner)
	isErr := f.newTemp("handle.is_err")
	errLabel := f.newLabel("handle.err")
	continueLabel := f.newLabel("handle.cont")
	fmt.Fprintf(&f.builder, "  %%%s = icmp ne i32 %s, 0\n", isErr, errValue.ref)
	fmt.Fprintf(&f.builder, "  br i1 %%%s, label %%%s, label %%%s\n", isErr, errLabel, continueLabel)
	f.terminated = true

	f.emitLabel(errLabel)
	f.genBlockWithErrorBinding(expr.Handler, expr.ErrName, errValue.ref)
	if !f.terminated {
		fmt.Fprintf(&f.builder, "  br label %%%s\n", continueLabel)
		f.terminated = true
	}

	f.emitLabel(continueLabel)
	return exprValue{typ: checker.TypeVoid}
}

func (f *functionEmitter) genBinary(expr *ast.BinaryExpr) exprValue {
	if expr.Operator == token.AmpAmp || expr.Operator == token.PipePipe {
		return f.genShortCircuitBinary(expr)
	}

	left := f.genExpression(expr.Left)
	right := f.genExpression(expr.Right)
	if left.typ == checker.TypeStr {
		return f.genStringBinary(expr.Operator, left, right)
	}

	out := f.newTemp("tmp")
	switch expr.Operator {
	case token.Plus:
		fmt.Fprintf(&f.builder, "  %%%s = add %s %s, %s\n", out, f.g.llvmType(left.typ), left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: left.typ}
	case token.Minus:
		fmt.Fprintf(&f.builder, "  %%%s = sub %s %s, %s\n", out, f.g.llvmType(left.typ), left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: left.typ}
	case token.Star:
		fmt.Fprintf(&f.builder, "  %%%s = mul %s %s, %s\n", out, f.g.llvmType(left.typ), left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: left.typ}
	case token.Slash:
		fmt.Fprintf(&f.builder, "  %%%s = sdiv %s %s, %s\n", out, f.g.llvmType(left.typ), left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: left.typ}
	case token.Percent:
		fmt.Fprintf(&f.builder, "  %%%s = srem %s %s, %s\n", out, f.g.llvmType(left.typ), left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: left.typ}
	case token.Less:
		fmt.Fprintf(&f.builder, "  %%%s = icmp slt %s %s, %s\n", out, f.g.llvmType(left.typ), left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: checker.TypeBool}
	case token.LessEqual:
		fmt.Fprintf(&f.builder, "  %%%s = icmp sle %s %s, %s\n", out, f.g.llvmType(left.typ), left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: checker.TypeBool}
	case token.Greater:
		fmt.Fprintf(&f.builder, "  %%%s = icmp sgt %s %s, %s\n", out, f.g.llvmType(left.typ), left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: checker.TypeBool}
	case token.GreaterEqual:
		fmt.Fprintf(&f.builder, "  %%%s = icmp sge %s %s, %s\n", out, f.g.llvmType(left.typ), left.ref, right.ref)
		return exprValue{ref: "%" + out, typ: checker.TypeBool}
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

func (f *functionEmitter) genStringBinary(op token.Kind, left, right exprValue) exprValue {
	lPtr := f.newTemp("str.l.ptr")
	lLen := f.newTemp("str.l.len")
	rPtr := f.newTemp("str.r.ptr")
	rLen := f.newTemp("str.r.len")
	fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 0\n", lPtr, left.ref)
	fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 1\n", lLen, left.ref)
	fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 0\n", rPtr, right.ref)
	fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 1\n", rLen, right.ref)

	switch op {
	case token.Plus:
		result := f.newTemp("str.concat")
		fmt.Fprintf(&f.builder, "  %%%s = call %%yar.str @yar_str_concat(ptr %%%s, i64 %%%s, ptr %%%s, i64 %%%s)\n", result, lPtr, lLen, rPtr, rLen)
		return exprValue{ref: "%" + result, typ: checker.TypeStr}
	case token.EqualEqual:
		raw := f.newTemp("str.eq.raw")
		fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_str_equal(ptr %%%s, i64 %%%s, ptr %%%s, i64 %%%s)\n", raw, lPtr, lLen, rPtr, rLen)
		result := f.newTemp("str.eq")
		fmt.Fprintf(&f.builder, "  %%%s = icmp ne i32 %%%s, 0\n", result, raw)
		return exprValue{ref: "%" + result, typ: checker.TypeBool}
	case token.BangEqual:
		raw := f.newTemp("str.eq.raw")
		fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_str_equal(ptr %%%s, i64 %%%s, ptr %%%s, i64 %%%s)\n", raw, lPtr, lLen, rPtr, rLen)
		result := f.newTemp("str.ne")
		fmt.Fprintf(&f.builder, "  %%%s = icmp eq i32 %%%s, 0\n", result, raw)
		return exprValue{ref: "%" + result, typ: checker.TypeBool}
	default:
		panic("unsupported string binary operator")
	}
}

func (f *functionEmitter) genShortCircuitBinary(expr *ast.BinaryExpr) exprValue {
	left := f.genExpression(expr.Left)
	rhsLabel := f.newLabel("logic.rhs")
	shortLabel := f.newLabel("logic.short")
	endLabel := f.newLabel("logic.end")
	shortValue := "0"
	if expr.Operator == token.PipePipe {
		shortValue = "1"
	}

	if expr.Operator == token.AmpAmp {
		fmt.Fprintf(&f.builder, "  br i1 %s, label %%%s, label %%%s\n", left.ref, rhsLabel, shortLabel)
	} else {
		fmt.Fprintf(&f.builder, "  br i1 %s, label %%%s, label %%%s\n", left.ref, shortLabel, rhsLabel)
	}
	f.terminated = true

	f.emitLabel(shortLabel)
	fmt.Fprintf(&f.builder, "  br label %%%s\n", endLabel)
	f.terminated = true

	f.emitLabel(rhsLabel)
	right := f.genExpression(expr.Right)
	rhsResultLabel := f.currentLabel
	fmt.Fprintf(&f.builder, "  br label %%%s\n", endLabel)
	f.terminated = true

	f.emitLabel(endLabel)
	out := f.newTemp("logic")
	fmt.Fprintf(&f.builder, "  %%%s = phi i1 [%s, %%%s], [%s, %%%s]\n", out, shortValue, shortLabel, right.ref, rhsResultLabel)
	return exprValue{ref: "%" + out, typ: checker.TypeBool}
}

func (f *functionEmitter) genCall(expr *ast.CallExpr) exprValue {
	sig, ok := f.g.info.Calls[expr]
	if !ok {
		panic("missing call signature")
	}

	if sig.Name == "len" && sig.Builtin {
		arg := f.genExpression(expr.Args[0])
		if array, ok := checker.ParseArrayType(f.exprType(expr.Args[0])); ok {
			return exprValue{ref: fmt.Sprintf("%d", array.Len), typ: checker.TypeI32}
		}
		if _, ok := checker.ParseMapType(f.exprType(expr.Args[0])); ok {
			lenResult := f.newTemp("map.len")
			fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_map_len(ptr %s)\n", lenResult, arg.ref)
			return exprValue{ref: "%" + lenResult, typ: checker.TypeI32}
		}
		if f.exprType(expr.Args[0]) == checker.TypeStr {
			length64 := f.newTemp("str.len64")
			fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 1\n", length64, arg.ref)
			length32 := f.newTemp("str.len")
			fmt.Fprintf(&f.builder, "  %%%s = trunc i64 %%%s to i32\n", length32, length64)
			return exprValue{ref: "%" + length32, typ: checker.TypeI32}
		}
		length := f.extractSliceField(arg.ref, 1, "slice.len")
		return exprValue{ref: length.ref, typ: checker.TypeI32}
	}
	if sig.Name == "has" && sig.Builtin {
		mapArg := f.genExpression(expr.Args[0])
		keyArg := f.genExpression(expr.Args[1])
		mapType, _ := checker.ParseMapType(f.exprType(expr.Args[0]))
		keySlot := f.newTemp("map.key.slot")
		fmt.Fprintf(&f.builder, "  %%%s = alloca %s\n", keySlot, f.g.llvmType(mapType.Key))
		fmt.Fprintf(&f.builder, "  store %s %s, ptr %%%s\n", f.g.llvmType(mapType.Key), keyArg.ref, keySlot)
		hasResult := f.newTemp("map.has")
		fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_map_has(ptr %s, ptr %%%s)\n", hasResult, mapArg.ref, keySlot)
		boolResult := f.newTemp("map.has.bool")
		fmt.Fprintf(&f.builder, "  %%%s = icmp ne i32 %%%s, 0\n", boolResult, hasResult)
		return exprValue{ref: "%" + boolResult, typ: checker.TypeBool}
	}
	if sig.Name == "delete" && sig.Builtin {
		mapArg := f.genExpression(expr.Args[0])
		keyArg := f.genExpression(expr.Args[1])
		mapType, _ := checker.ParseMapType(f.exprType(expr.Args[0]))
		keySlot := f.newTemp("map.key.slot")
		fmt.Fprintf(&f.builder, "  %%%s = alloca %s\n", keySlot, f.g.llvmType(mapType.Key))
		fmt.Fprintf(&f.builder, "  store %s %s, ptr %%%s\n", f.g.llvmType(mapType.Key), keyArg.ref, keySlot)
		fmt.Fprintf(&f.builder, "  call void @yar_map_delete(ptr %s, ptr %%%s)\n", mapArg.ref, keySlot)
		return exprValue{typ: checker.TypeVoid}
	}
	if sig.Name == "append" && sig.Builtin {
		sliceValue := f.genExpression(expr.Args[0])
		element := f.genExpression(expr.Args[1])
		sliceType, _ := checker.ParseSliceType(sig.Return)
		oldData := f.extractSliceField(sliceValue.ref, 0, "append.data")
		oldLen := f.extractSliceField(sliceValue.ref, 1, "append.len")
		oldCap := f.extractSliceField(sliceValue.ref, 2, "append.cap")
		newLen := f.newTemp("append.len")
		fmt.Fprintf(&f.builder, "  %%%s = add i32 %s, 1\n", newLen, oldLen.ref)
		oldLen64 := f.intToI64(oldLen.ref, oldLen.typ)
		needsGrow := f.newTemp("append.needs_grow")
		growLabel := f.newLabel("append.grow")
		reuseLabel := f.newLabel("append.reuse")
		writeLabel := f.newLabel("append.write")
		fmt.Fprintf(&f.builder, "  %%%s = icmp eq i32 %s, %s\n", needsGrow, oldLen.ref, oldCap.ref)
		fmt.Fprintf(&f.builder, "  br i1 %%%s, label %%%s, label %%%s\n", needsGrow, growLabel, reuseLabel)
		f.terminated = true

		f.emitLabel(reuseLabel)
		fmt.Fprintf(&f.builder, "  br label %%%s\n", writeLabel)
		f.terminated = true

		f.emitLabel(growLabel)
		capWasZero := f.newTemp("append.cap_zero")
		doubledCap := f.newTemp("append.cap_double")
		newCap := f.newTemp("append.cap")
		fmt.Fprintf(&f.builder, "  %%%s = icmp eq i32 %s, 0\n", capWasZero, oldCap.ref)
		fmt.Fprintf(&f.builder, "  %%%s = mul i32 %s, 2\n", doubledCap, oldCap.ref)
		fmt.Fprintf(&f.builder, "  %%%s = select i1 %%%s, i32 1, i32 %%%s\n", newCap, capWasZero, doubledCap)
		allocSize := f.emitScaledSize(sliceType.Elem, f.intToI64("%"+newCap, checker.TypeI32))
		newData := f.emitAllocBytes(allocSize, false)
		hasExisting := f.newTemp("append.has_existing")
		copyLabel := f.newLabel("append.copy")
		growReadyLabel := f.newLabel("append.ready")
		fmt.Fprintf(&f.builder, "  %%%s = icmp ne i32 %s, 0\n", hasExisting, oldLen.ref)
		fmt.Fprintf(&f.builder, "  br i1 %%%s, label %%%s, label %%%s\n", hasExisting, copyLabel, growReadyLabel)
		f.terminated = true

		f.emitLabel(copyLabel)
		copySize := f.emitScaledSize(sliceType.Elem, oldLen64)
		f.emitMemcpy(newData, oldData.ref, copySize)
		fmt.Fprintf(&f.builder, "  br label %%%s\n", growReadyLabel)
		f.terminated = true

		f.emitLabel(growReadyLabel)
		dataPhi := f.newTemp("append.data")
		capPhi := f.newTemp("append.cap")
		fmt.Fprintf(&f.builder, "  br label %%%s\n", writeLabel)
		f.terminated = true

		f.emitLabel(writeLabel)
		fmt.Fprintf(&f.builder, "  %%%s = phi ptr [%s, %%%s], [%s, %%%s]\n", dataPhi, oldData.ref, reuseLabel, newData, growReadyLabel)
		fmt.Fprintf(&f.builder, "  %%%s = phi i32 [%s, %%%s], [%%%s, %%%s]\n", capPhi, oldCap.ref, reuseLabel, newCap, growReadyLabel)
		elemPtr := f.newTemp("append.elem.ptr")
		fmt.Fprintf(&f.builder, "  %%%s = getelementptr %s, ptr %%%s, i64 %s\n", elemPtr, f.g.llvmType(sliceType.Elem), dataPhi, oldLen64)
		fmt.Fprintf(&f.builder, "  store %s %s, ptr %%%s\n", f.g.llvmType(sliceType.Elem), element.ref, elemPtr)
		ref := f.emitSliceValue("%"+dataPhi, "%"+newLen, "%"+capPhi)
		return exprValue{ref: ref, typ: sig.Return}
	}
	if sig.Name == "keys" && sig.Builtin {
		mapArg := f.genExpression(expr.Args[0])
		result := f.newTemp("map.keys")
		fmt.Fprintf(&f.builder, "  %%%s = call %%yar.slice @yar_map_keys(ptr %s)\n", result, mapArg.ref)
		return exprValue{ref: "%" + result, typ: sig.Return}
	}
	if sig.ValueCall {
		callee := f.genExpression(expr.Callee)
		codePtr := f.newTemp("closure.code")
		envPtr := f.newTemp("closure.env")
		fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.closure %s, 0\n", codePtr, callee.ref)
		fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.closure %s, 1\n", envPtr, callee.ref)

		llvmArgs := make([]string, 0, len(expr.Args)+1)
		llvmArgs = append(llvmArgs, fmt.Sprintf("ptr %%%s", envPtr))
		for i, arg := range expr.Args {
			value := f.genExpression(arg)
			llvmArgs = append(llvmArgs, fmt.Sprintf("%s %s", f.g.llvmType(sig.Params[i]), value.ref))
		}

		callType := f.g.llvmType(sig.Return)
		if sig.Errorable {
			callType = f.g.resultTypeName(sig.Return)
		}
		result := f.newTemp("call")
		fmt.Fprintf(&f.builder, "  %%%s = call %s %%%s(%s)\n", result, callType, codePtr, strings.Join(llvmArgs, ", "))
		return exprValue{ref: "%" + result, typ: sig.Return}
	}

	args := make([]exprValue, 0, len(expr.Args))
	if sig.Method {
		selector, ok := expr.Callee.(*ast.SelectorExpr)
		if !ok {
			panic("method call requires selector callee")
		}
		args = append(args, f.genExpression(selector.Inner))
	}
	for _, arg := range expr.Args {
		args = append(args, f.genExpression(arg))
	}

	if sig.HostIntrinsic {
		return f.genHostIntrinsicCall(sig, args)
	}

	if sig.Builtin {
		switch sig.Name {
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
		case "panic":
			ptr := f.newTemp("panic.ptr")
			length := f.newTemp("panic.len")
			fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 0\n", ptr, args[0].ref)
			fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 1\n", length, args[0].ref)
			fmt.Fprintf(&f.builder, "  call void @yar_panic(ptr %%%s, i64 %%%s)\n", ptr, length)
			f.builder.WriteString("  unreachable\n")
			f.terminated = true
			return exprValue{typ: checker.TypeNoReturn}
		case "chr":
			tmp := f.newTemp("chr")
			fmt.Fprintf(&f.builder, "  %%%s = call %%yar.str @yar_str_from_byte(i32 %s)\n", tmp, args[0].ref)
			return exprValue{ref: "%" + tmp, typ: checker.TypeStr}
		case "i32_to_i64":
			tmp := f.newTemp("widen")
			fmt.Fprintf(&f.builder, "  %%%s = sext i32 %s to i64\n", tmp, args[0].ref)
			return exprValue{ref: "%" + tmp, typ: checker.TypeI64}
		case "i64_to_i32":
			tmp := f.newTemp("narrow")
			fmt.Fprintf(&f.builder, "  %%%s = trunc i64 %s to i32\n", tmp, args[0].ref)
			return exprValue{ref: "%" + tmp, typ: checker.TypeI32}
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
	if sig.Return == checker.TypeVoid && !sig.Errorable {
		fmt.Fprintf(&f.builder, "  call void %s(%s)\n", f.g.functionName(sig.FullName), strings.Join(llvmArgs, ", "))
		return exprValue{typ: checker.TypeVoid}
	}
	if sig.Return == checker.TypeNoReturn {
		fmt.Fprintf(&f.builder, "  call void %s(%s)\n", f.g.functionName(sig.FullName), strings.Join(llvmArgs, ", "))
		f.builder.WriteString("  unreachable\n")
		f.terminated = true
		return exprValue{typ: checker.TypeNoReturn}
	}
	tmp := f.newTemp("call")
	fmt.Fprintf(&f.builder, "  %%%s = call %s %s(%s)\n", tmp, callType, f.g.functionName(sig.FullName), strings.Join(llvmArgs, ", "))
	return exprValue{
		ref: "%" + tmp,
		typ: sig.Return,
	}
}

func (f *functionEmitter) genHostIntrinsicCall(sig checker.Signature, args []exprValue) exprValue {
	switch sig.FullName {
	case "process.args":
		out := f.newTemp("process.args.out")
		fmt.Fprintf(&f.builder, "  %%%s = alloca %%yar.slice\n", out)
		fmt.Fprintf(&f.builder, "  store %%yar.slice zeroinitializer, ptr %%%s\n", out)
		fmt.Fprintf(&f.builder, "  call void @yar_process_args(ptr %%%s)\n", out)
		return f.loadValue("%"+out, sig.Return)
	case "process.run":
		argv := f.newTemp("process.run.argv")
		fmt.Fprintf(&f.builder, "  %%%s = alloca %%yar.slice\n", argv)
		fmt.Fprintf(&f.builder, "  store %%yar.slice %s, ptr %%%s\n", args[0].ref, argv)
		out := f.newTemp("process.run.out")
		fmt.Fprintf(&f.builder, "  %%%s = alloca %s\n", out, f.g.llvmType(sig.Return))
		fmt.Fprintf(&f.builder, "  store %s zeroinitializer, ptr %%%s\n", f.g.llvmType(sig.Return), out)
		status := f.newTemp("process.run.status")
		fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_process_run(ptr %%%s, ptr %%%s)\n", status, argv, out)
		value := f.loadValue("%"+out, sig.Return)
		return exprValue{ref: f.emitHostStatusResult(sig.FullName, sig.Return, "%"+status, value.ref), typ: sig.Return}
	case "process.run_inherit":
		argv := f.newTemp("process.run_inherit.argv")
		fmt.Fprintf(&f.builder, "  %%%s = alloca %%yar.slice\n", argv)
		fmt.Fprintf(&f.builder, "  store %%yar.slice %s, ptr %%%s\n", args[0].ref, argv)
		out := f.newTemp("process.run_inherit.out")
		fmt.Fprintf(&f.builder, "  %%%s = alloca i32\n", out)
		fmt.Fprintf(&f.builder, "  store i32 0, ptr %%%s\n", out)
		status := f.newTemp("process.run_inherit.status")
		fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_process_run_inherit(ptr %%%s, ptr %%%s)\n", status, argv, out)
		value := f.loadValue("%"+out, sig.Return)
		return exprValue{ref: f.emitHostStatusResult(sig.FullName, sig.Return, "%"+status, value.ref), typ: sig.Return}
	case "env.lookup":
		out := f.newTemp("env.lookup.out")
		fmt.Fprintf(&f.builder, "  %%%s = alloca %%yar.str\n", out)
		fmt.Fprintf(&f.builder, "  store %%yar.str zeroinitializer, ptr %%%s\n", out)
		status := f.newTemp("env.lookup.status")
		fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_env_lookup(%%yar.str %s, ptr %%%s)\n", status, args[0].ref, out)
		value := f.loadValue("%"+out, checker.TypeStr)
		return exprValue{ref: f.emitHostStatusResult(sig.FullName, sig.Return, "%"+status, value.ref), typ: sig.Return}
	case "stdio.eprint":
		ptr := f.newTemp("eprint.ptr")
		length := f.newTemp("eprint.len")
		fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 0\n", ptr, args[0].ref)
		fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.str %s, 1\n", length, args[0].ref)
		fmt.Fprintf(&f.builder, "  call void @yar_eprint(ptr %%%s, i64 %%%s)\n", ptr, length)
		return exprValue{typ: checker.TypeVoid}
	case "fs.read_file":
		out := f.newTemp("fs.read_file.out")
		fmt.Fprintf(&f.builder, "  %%%s = alloca %%yar.str\n", out)
		fmt.Fprintf(&f.builder, "  store %%yar.str zeroinitializer, ptr %%%s\n", out)
		status := f.newTemp("fs.read_file.status")
		fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_fs_read_file(%%yar.str %s, ptr %%%s)\n", status, args[0].ref, out)
		value := f.loadValue("%"+out, checker.TypeStr)
		return exprValue{ref: f.emitHostStatusResult(sig.FullName, sig.Return, "%"+status, value.ref), typ: sig.Return}
	case "fs.write_file":
		status := f.newTemp("fs.write_file.status")
		fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_fs_write_file(%%yar.str %s, %%yar.str %s)\n", status, args[0].ref, args[1].ref)
		return exprValue{ref: f.emitHostStatusResult(sig.FullName, sig.Return, "%"+status, ""), typ: sig.Return}
	case "fs.read_dir":
		out := f.newTemp("fs.read_dir.out")
		fmt.Fprintf(&f.builder, "  %%%s = alloca %%yar.slice\n", out)
		fmt.Fprintf(&f.builder, "  store %%yar.slice zeroinitializer, ptr %%%s\n", out)
		status := f.newTemp("fs.read_dir.status")
		fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_fs_read_dir(%%yar.str %s, ptr %%%s)\n", status, args[0].ref, out)
		value := f.loadValue("%"+out, sig.Return)
		return exprValue{ref: f.emitHostStatusResult(sig.FullName, sig.Return, "%"+status, value.ref), typ: sig.Return}
	case "fs.stat":
		out := f.newTemp("fs.stat.out")
		fmt.Fprintf(&f.builder, "  %%%s = alloca i32\n", out)
		fmt.Fprintf(&f.builder, "  store i32 0, ptr %%%s\n", out)
		status := f.newTemp("fs.stat.status")
		fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_fs_stat(%%yar.str %s, ptr %%%s)\n", status, args[0].ref, out)
		tag := f.newTemp("fs.stat.tag")
		fmt.Fprintf(&f.builder, "  %%%s = load i32, ptr %%%s\n", tag, out)
		return exprValue{ref: f.emitHostStatusResult(sig.FullName, sig.Return, "%"+status, f.emitEnumTagValue(sig.Return, "%"+tag)), typ: sig.Return}
	case "fs.mkdir_all":
		status := f.newTemp("fs.mkdir_all.status")
		fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_fs_mkdir_all(%%yar.str %s)\n", status, args[0].ref)
		return exprValue{ref: f.emitHostStatusResult(sig.FullName, sig.Return, "%"+status, ""), typ: sig.Return}
	case "fs.remove_all":
		status := f.newTemp("fs.remove_all.status")
		fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_fs_remove_all(%%yar.str %s)\n", status, args[0].ref)
		return exprValue{ref: f.emitHostStatusResult(sig.FullName, sig.Return, "%"+status, ""), typ: sig.Return}
	case "fs.temp_dir":
		out := f.newTemp("fs.temp_dir.out")
		fmt.Fprintf(&f.builder, "  %%%s = alloca %%yar.str\n", out)
		fmt.Fprintf(&f.builder, "  store %%yar.str zeroinitializer, ptr %%%s\n", out)
		status := f.newTemp("fs.temp_dir.status")
		fmt.Fprintf(&f.builder, "  %%%s = call i32 @yar_fs_temp_dir(%%yar.str %s, ptr %%%s)\n", status, args[0].ref, out)
		value := f.loadValue("%"+out, checker.TypeStr)
		return exprValue{ref: f.emitHostStatusResult(sig.FullName, sig.Return, "%"+status, value.ref), typ: sig.Return}
	default:
		panic("unsupported host intrinsic")
	}
}

func (f *functionEmitter) addressOfTarget(target ast.Expression) (string, checker.Type) {
	ptr, typ, ok := f.addressOfLValueExpr(target)
	if !ok {
		panic(fmt.Sprintf("unsupported assignment target %T", target))
	}
	return ptr, typ
}

func (f *functionEmitter) addressOfExpr(expr ast.Expression) (string, checker.Type) {
	if ptr, typ, ok := f.addressOfLValueExpr(expr); ok {
		return ptr, typ
	}

	value := f.genExpression(expr)
	ptr := f.emitAllocType(value.typ, false)
	fmt.Fprintf(&f.builder, "  store %s %s, ptr %s\n", f.g.llvmType(value.typ), value.ref, ptr)
	return ptr, value.typ
}

func (f *functionEmitter) addressOfLValueExpr(expr ast.Expression) (string, checker.Type, bool) {
	switch e := expr.(type) {
	case *ast.IdentExpr:
		slot, ok := f.lookupLocal(e.Name)
		if !ok {
			return "", checker.TypeInvalid, false
		}
		return slot.ptr, slot.typ, true
	case *ast.GroupExpr:
		return f.addressOfLValueExpr(e.Inner)
	case *ast.UnaryExpr:
		if e.Operator != token.Star {
			return "", checker.TypeInvalid, false
		}
		value := f.genExpression(e.Inner)
		pointer, _ := checker.ParsePointerType(value.typ)
		return value.ref, pointer.Elem, true
	case *ast.SelectorExpr:
		basePtr, baseType, ok := f.addressOfLValueExpr(e.Inner)
		if !ok {
			return "", checker.TypeInvalid, false
		}
		info := f.g.info.Structs[string(baseType)]
		field, index, _ := info.Field(e.Name)
		ptr := f.newTemp("field.ptr")
		fmt.Fprintf(&f.builder, "  %%%s = getelementptr inbounds %s, ptr %s, i32 0, i32 %d\n", ptr, f.g.llvmType(baseType), basePtr, index)
		return "%" + ptr, field.Type, true
	case *ast.IndexExpr:
		basePtr, baseType, ok := f.addressOfLValueExpr(e.Inner)
		if !ok {
			return "", checker.TypeInvalid, false
		}
		if array, ok := checker.ParseArrayType(baseType); ok {
			index := f.genExpression(e.Index)
			ptr := f.newTemp("elem.ptr")
			fmt.Fprintf(&f.builder, "  %%%s = getelementptr inbounds %s, ptr %s, i32 0, %s %s\n", ptr, f.g.llvmType(baseType), basePtr, f.g.llvmType(index.typ), index.ref)
			return "%" + ptr, array.Elem, true
		}
		slice, _ := checker.ParseSliceType(baseType)
		ptr := f.addressOfSliceElement(basePtr, slice.Elem, f.genExpression(e.Index))
		return ptr, slice.Elem, true
	default:
		return "", checker.TypeInvalid, false
	}
}

func (f *functionEmitter) addressOfAggregateExpr(expr ast.Expression) (string, checker.Type) {
	if ptr, typ, ok := f.addressOfLValueExpr(expr); ok {
		return ptr, typ
	}

	value := f.genExpression(expr)
	slot := f.newTemp("agg.slot")
	fmt.Fprintf(&f.builder, "  %%%s = alloca %s\n", slot, f.g.llvmType(value.typ))
	fmt.Fprintf(&f.builder, "  store %s %s, ptr %%%s\n", f.g.llvmType(value.typ), value.ref, slot)
	return "%" + slot, value.typ
}

func (f *functionEmitter) newLocalSlot(typ checker.Type) string {
	return f.emitAllocType(typ, false)
}

func (f *functionEmitter) loadValue(ptr string, typ checker.Type) exprValue {
	load := f.newTemp("load")
	fmt.Fprintf(&f.builder, "  %%%s = load %s, ptr %s\n", load, f.g.llvmType(typ), ptr)
	return exprValue{ref: "%" + load, typ: typ}
}

func (f *functionEmitter) extractSliceField(sliceRef string, index int, prefix string) exprValue {
	tmp := f.newTemp(prefix)
	switch index {
	case 0:
		fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.slice %s, 0\n", tmp, sliceRef)
		return exprValue{ref: "%" + tmp}
	case 1, 2:
		fmt.Fprintf(&f.builder, "  %%%s = extractvalue %%yar.slice %s, %d\n", tmp, sliceRef, index)
		return exprValue{ref: "%" + tmp, typ: checker.TypeI32}
	default:
		panic("unsupported slice field")
	}
}

func (f *functionEmitter) intToI64(ref string, typ checker.Type) string {
	switch typ {
	case checker.TypeI64:
		return ref
	case checker.TypeI32, checker.TypeBool:
		tmp := f.newTemp("i64")
		fmt.Fprintf(&f.builder, "  %%%s = sext %s %s to i64\n", tmp, f.g.llvmType(typ), ref)
		return "%" + tmp
	default:
		return ref
	}
}

func (f *functionEmitter) emitSliceValue(dataPtr, length, capacity string) string {
	tmp1 := f.newTemp("slice")
	tmp2 := f.newTemp("slice")
	tmp3 := f.newTemp("slice")
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %%yar.slice zeroinitializer, ptr %s, 0\n", tmp1, dataPtr)
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %%yar.slice %%%s, i32 %s, 1\n", tmp2, tmp1, length)
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %%yar.slice %%%s, i32 %s, 2\n", tmp3, tmp2, capacity)
	return "%" + tmp3
}

func (f *functionEmitter) emitClosureValue(codeRef, envRef string, typ checker.Type) exprValue {
	tmp1 := f.newTemp("closure")
	tmp2 := f.newTemp("closure")
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %%yar.closure zeroinitializer, ptr %s, 0\n", tmp1, codeRef)
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %%yar.closure %%%s, ptr %s, 1\n", tmp2, tmp1, envRef)
	return exprValue{ref: "%" + tmp2, typ: typ}
}

func (f *functionEmitter) emitArrayAllocSize(elemType checker.Type, count int) string {
	countRef := fmt.Sprintf("%d", count)
	return f.emitScaledSize(elemType, countRef)
}

func (f *functionEmitter) emitScaledSize(elemType checker.Type, countRef string) string {
	elemSize := f.emitTypeSize(elemType)
	tmp := f.newTemp("size")
	fmt.Fprintf(&f.builder, "  %%%s = mul i64 %s, %s\n", tmp, elemSize, countRef)
	return "%" + tmp
}

func (f *functionEmitter) emitMemcpy(dst, src, size string) {
	fmt.Fprintf(&f.builder, "  call void @llvm.memcpy.p0.p0.i64(ptr %s, ptr %s, i64 %s, i1 false)\n", dst, src, size)
}

func (f *functionEmitter) addressOfSliceElement(slicePtr string, elemType checker.Type, index exprValue) string {
	sliceValue := f.loadValue(slicePtr, checker.MakeSliceType(elemType))
	dataPtr := f.extractSliceField(sliceValue.ref, 0, "slice.data")
	length := f.extractSliceField(sliceValue.ref, 1, "slice.len")
	index64 := f.intToI64(index.ref, index.typ)
	length64 := f.intToI64(length.ref, length.typ)
	fmt.Fprintf(&f.builder, "  call void @yar_slice_index_check(i64 %s, i64 %s)\n", index64, length64)
	ptr := f.newTemp("slice.elem.ptr")
	fmt.Fprintf(&f.builder, "  %%%s = getelementptr %s, ptr %s, i64 %s\n", ptr, f.g.llvmType(elemType), dataPtr.ref, index64)
	return "%" + ptr
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

func (f *functionEmitter) emitAllocBytes(size string, zeroed bool) string {
	helper := "@yar_alloc"
	if zeroed {
		helper = "@yar_alloc_zeroed"
	}
	tmp := f.newTemp("alloc")
	fmt.Fprintf(&f.builder, "  %%%s = call ptr %s(i64 %s)\n", tmp, helper, size)
	return "%" + tmp
}

func (f *functionEmitter) emitTypeSize(typ checker.Type) string {
	sizePtr := f.newTemp("size.ptr")
	size := f.newTemp("size")
	fmt.Fprintf(&f.builder, "  %%%s = getelementptr %s, ptr null, i32 1\n", sizePtr, f.g.llvmType(typ))
	fmt.Fprintf(&f.builder, "  %%%s = ptrtoint ptr %%%s to i64\n", size, sizePtr)
	return "%" + size
}

func (f *functionEmitter) emitAllocType(typ checker.Type, zeroed bool) string {
	return f.emitAllocBytes(f.emitTypeSize(typ), zeroed)
}

func (f *functionEmitter) emitHostStatusResult(fullName string, typ checker.Type, status, successValue string) string {
	isOK := f.newTemp("host.ok")
	okLabel := f.newLabel("host.ok")
	errLabel := f.newLabel("host.err")
	endLabel := f.newLabel("host.end")
	fmt.Fprintf(&f.builder, "  %%%s = icmp eq i32 %s, 0\n", isOK, status)
	fmt.Fprintf(&f.builder, "  br i1 %%%s, label %%%s, label %%%s\n", isOK, okLabel, errLabel)
	f.terminated = true

	f.emitLabel(okLabel)
	var okResult string
	if typ == checker.TypeVoid {
		okResult = f.emitSuccessVoid()
	} else {
		okResult = f.emitSuccessResult(typ, successValue)
	}
	fmt.Fprintf(&f.builder, "  br label %%%s\n", endLabel)
	f.terminated = true

	f.emitLabel(errLabel)
	errCode := f.emitHostErrorCode(fullName, status)
	errResult := f.emitErrorCodeResult(typ, errCode)
	fmt.Fprintf(&f.builder, "  br label %%%s\n", endLabel)
	f.terminated = true

	f.emitLabel(endLabel)
	result := f.newTemp("host.result")
	fmt.Fprintf(&f.builder, "  %%%s = phi %s [%s, %%%s], [%s, %%%s]\n", result, f.g.resultTypeName(typ), okResult, okLabel, errResult, errLabel)
	return "%" + result
}

func (f *functionEmitter) emitHostErrorCode(fullName, status string) string {
	code := fmt.Sprintf("%d", f.mustErrorCode("IO"))

	var items []struct {
		status int
		name   string
	}
	switch fullName {
	case "fs.read_file", "fs.write_file", "fs.read_dir", "fs.stat", "fs.mkdir_all", "fs.remove_all", "fs.temp_dir":
		items = []struct {
			status int
			name   string
		}{
			{status: 4, name: "InvalidPath"},
			{status: 3, name: "AlreadyExists"},
			{status: 2, name: "PermissionDenied"},
			{status: 1, name: "NotFound"},
		}
	case "process.run", "process.run_inherit", "env.lookup":
		items = []struct {
			status int
			name   string
		}{
			{status: 3, name: "InvalidArgument"},
			{status: 2, name: "PermissionDenied"},
			{status: 1, name: "NotFound"},
		}
	default:
		panic("missing host error mapping for " + fullName)
	}

	for _, item := range items {
		match := f.newTemp("host.err.match")
		next := f.newTemp("host.err.code")
		fmt.Fprintf(&f.builder, "  %%%s = icmp eq i32 %s, %d\n", match, status, item.status)
		fmt.Fprintf(&f.builder, "  %%%s = select i1 %%%s, i32 %d, i32 %s\n", next, match, f.mustErrorCode(item.name), code)
		code = "%" + next
	}
	return code
}

func (f *functionEmitter) emitEnumTagValue(enumType checker.Type, tag string) string {
	tmp := f.newTemp("enum.tag")
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s zeroinitializer, i32 %s, 0\n", tmp, f.g.llvmType(enumType), tag)
	return "%" + tmp
}

func (f *functionEmitter) mustErrorCode(name string) int {
	code, ok := f.g.info.ErrorCodes[name]
	if !ok || code == 0 {
		panic("missing error code for " + name)
	}
	return code
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
	return f.emitErrorCodeResult(typ, fmt.Sprintf("%d", code))
}

func (f *functionEmitter) emitErrorCodeResult(typ checker.Type, code string) string {
	tmp1 := f.newTemp("result")
	tmp2 := f.newTemp("result")
	if typ == checker.TypeVoid {
		fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s zeroinitializer, i1 1, 0\n", tmp1, f.g.resultTypeName(checker.TypeVoid))
		fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s %%%s, i32 %s, 1\n", tmp2, f.g.resultTypeName(checker.TypeVoid), tmp1, code)
		return "%" + tmp2
	}
	tmp3 := f.newTemp("result")
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s zeroinitializer, i1 1, 0\n", tmp1, f.g.resultTypeName(typ))
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s %%%s, i32 %s, 1\n", tmp2, f.g.resultTypeName(typ), tmp1, code)
	fmt.Fprintf(&f.builder, "  %%%s = insertvalue %s %%%s, %s %s, 2\n", tmp3, f.g.resultTypeName(typ), tmp2, f.g.llvmType(typ), f.g.zeroValue(typ))
	return "%" + tmp3
}

func (f *functionEmitter) emitEnumValue(enumType checker.Type, enumCase checker.EnumCaseInfo, payload *exprValue) exprValue {
	slot := f.newTemp("enum.slot")
	fmt.Fprintf(&f.builder, "  %%%s = alloca %s\n", slot, f.g.llvmType(enumType))
	fmt.Fprintf(&f.builder, "  store %s zeroinitializer, ptr %%%s\n", f.g.llvmType(enumType), slot)
	tagPtr := f.newTemp("enum.tag.ptr")
	fmt.Fprintf(&f.builder, "  %%%s = getelementptr inbounds %s, ptr %%%s, i32 0, i32 0\n", tagPtr, f.g.llvmType(enumType), slot)
	fmt.Fprintf(&f.builder, "  store i32 %d, ptr %%%s\n", enumCase.Tag, tagPtr)
	if payload != nil && payload.typ != checker.TypeInvalid {
		fmt.Fprintf(&f.builder, "  store %s %s, ptr %s\n", f.g.llvmType(payload.typ), payload.ref, f.enumPayloadPtr("%"+slot, enumType))
	}
	return f.loadValue("%"+slot, enumType)
}

func (g *Generator) zeroValue(typ checker.Type) string {
	switch typ {
	case checker.TypeBool, checker.TypeI32, checker.TypeI64, checker.TypeError:
		return "0"
	case checker.TypeStr:
		return "zeroinitializer"
	default:
		if _, ok := checker.ParsePointerType(typ); ok {
			return "null"
		}
		if _, ok := checker.ParseFunctionType(typ); ok {
			return "zeroinitializer"
		}
		if _, ok := checker.ParseArrayType(typ); ok {
			return "zeroinitializer"
		}
		if _, ok := checker.ParseSliceType(typ); ok {
			return "zeroinitializer"
		}
		if _, ok := checker.ParseMapType(typ); ok {
			return "null"
		}
		if _, ok := g.info.Enums[string(typ)]; ok {
			return "zeroinitializer"
		}
		if _, ok := g.info.Structs[string(typ)]; ok {
			return "zeroinitializer"
		}
		return "0"
	}
}

func (g *Generator) queueFunctionLiteral(lit *ast.FunctionLiteralExpr) string {
	if name, ok := g.literalNames[lit]; ok {
		return name
	}
	name := fmt.Sprintf("closure.%d", g.nextLiteralID)
	g.nextLiteralID++
	g.literalNames[lit] = name
	g.literalQueue = append(g.literalQueue, lit)
	return name
}

func (g *Generator) closureEnvTypeLiteral(captures []checker.CaptureInfo) string {
	if len(captures) == 0 {
		return "{ }"
	}
	fields := make([]string, 0, len(captures))
	for _, capture := range captures {
		fields = append(fields, g.llvmType(capture.Type))
	}
	return "{ " + strings.Join(fields, ", ") + " }"
}

func (g *Generator) closureEnvSize(captures []checker.CaptureInfo) int {
	size := 0
	align := 1
	for _, capture := range captures {
		fieldAlign := g.typeAlign(capture.Type)
		size = alignTo(size, fieldAlign)
		size += g.typeSize(capture.Type)
		if fieldAlign > align {
			align = fieldAlign
		}
	}
	return alignTo(size, align)
}

func (f *functionEmitter) exprType(expr ast.Expression) checker.Type {
	return f.exprInfo(expr).Base
}

func (f *functionEmitter) exprInfo(expr ast.Expression) checker.ExprType {
	exprType, ok := f.g.info.ExprTypes[expr]
	if !ok {
		return checker.ExprType{Base: checker.TypeInvalid}
	}
	return exprType
}

func (f *functionEmitter) enumPayloadPtr(enumPtr string, enumType checker.Type) string {
	ptr := f.newTemp("enum.payload.ptr")
	fmt.Fprintf(&f.builder, "  %%%s = getelementptr inbounds %s, ptr %s, i32 0, i32 1\n", ptr, f.g.llvmType(enumType), enumPtr)
	return "%" + ptr
}

func (f *functionEmitter) localType(node ast.Node) checker.Type {
	typ, ok := f.g.info.Locals[node]
	if !ok {
		return checker.TypeInvalid
	}
	return typ
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
	f.currentLabel = label
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

func (f *functionEmitter) emitPropagatedError(code string) {
	if f.sig.Errorable {
		result := f.emitErrorCodeResult(f.sig.Return, code)
		fmt.Fprintf(&f.builder, "  ret %s %s\n", f.g.resultTypeName(f.sig.Return), result)
		f.terminated = true
		return
	}
	if f.sig.Return == checker.TypeError {
		fmt.Fprintf(&f.builder, "  ret i32 %s\n", code)
		f.terminated = true
		return
	}
	panic("propagation requires an error-capable return")
}
