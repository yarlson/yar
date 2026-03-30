package checker

import (
	"fmt"
	"sort"
	"strconv"
	"strings"

	"yar/internal/ast"
	"yar/internal/diag"
	"yar/internal/token"
)

type Type string

const (
	TypeInvalid    Type = ""
	TypeVoid       Type = "void"
	TypeNoReturn   Type = "noreturn"
	TypeBool       Type = "bool"
	TypeI32        Type = "i32"
	TypeI64        Type = "i64"
	TypeStr        Type = "str"
	TypeError      Type = "error"
	TypeNil        Type = "nil"
	TypeUntypedInt Type = "untyped-int"
)

type ExprType struct {
	Base      Type
	Errorable bool
}

type Signature struct {
	Name          string
	Package       string
	FullName      string
	Method        bool
	ValueCall     bool
	Receiver      Type
	Params        []Type
	Return        Type
	Errorable     bool
	Builtin       bool
	HostIntrinsic bool
	Exported      bool
}

type StructField struct {
	Name string
	Type Type
}

type StructInfo struct {
	Name     string
	Package  string
	FullName string
	Exported bool
	Fields   []StructField
}

func (s StructInfo) Field(name string) (StructField, int, bool) {
	for i, field := range s.Fields {
		if field.Name == name {
			return field, i, true
		}
	}
	return StructField{}, -1, false
}

type EnumCaseInfo struct {
	Name        string
	Tag         int
	PayloadType Type
	Fields      []StructField
}

func (c EnumCaseInfo) Field(name string) (StructField, int, bool) {
	for i, field := range c.Fields {
		if field.Name == name {
			return field, i, true
		}
	}
	return StructField{}, -1, false
}

type EnumInfo struct {
	Name     string
	Package  string
	FullName string
	Exported bool
	Cases    []EnumCaseInfo
}

func (e EnumInfo) Case(name string) (EnumCaseInfo, int, bool) {
	for i, enumCase := range e.Cases {
		if enumCase.Name == name {
			return enumCase, i, true
		}
	}
	return EnumCaseInfo{}, -1, false
}

type InterfaceMethodInfo struct {
	Name      string
	Params    []Type
	Return    Type
	Errorable bool
}

type InterfaceInfo struct {
	Name     string
	Package  string
	FullName string
	Exported bool
	Methods  []InterfaceMethodInfo
}

func (i InterfaceInfo) Method(name string) (InterfaceMethodInfo, int, bool) {
	for idx, method := range i.Methods {
		if method.Name == name {
			return method, idx, true
		}
	}
	return InterfaceMethodInfo{}, -1, false
}

type EnumConstructorInfo struct {
	EnumName string
	CaseName string
}

type Info struct {
	Functions        map[*ast.FunctionDecl]Signature
	FunctionLiterals map[*ast.FunctionLiteralExpr]FunctionLiteralInfo
	Calls            map[*ast.CallExpr]Signature
	ExprTypes        map[ast.Expression]ExprType
	Locals           map[ast.Node]Type
	Structs          map[string]StructInfo
	Interfaces       map[string]InterfaceInfo
	Enums            map[string]EnumInfo
	ErrorCodes       map[string]int
	OrderedErrors    []string
	AutoDeref        map[*ast.SelectorExpr]bool
	EnumConstructors map[*ast.CallExpr]EnumConstructorInfo
}

type CaptureInfo struct {
	Name string
	Type Type
}

type FunctionLiteralInfo struct {
	Signature Signature
	Captures  []CaptureInfo
}

type Checker struct {
	diag       diag.List
	functions  map[string]Signature
	methods    map[Type]map[string]Signature
	structs    map[string]*ast.StructDecl
	interfaces map[string]*ast.InterfaceDecl
	enums      map[string]*ast.EnumDecl
	info       Info
	current    *functionContext
}

type functionContext struct {
	signature Signature
	parent    *functionContext
	literal   *ast.FunctionLiteralExpr
	scopes    []map[string]localBinding
	loopDepth int
}

type localBinding struct {
	typ   Type
	owner *functionContext
	node  ast.Node
}

type coercedIntegers struct {
	Left   ExprType
	Right  ExprType
	Result Type
}

type ArrayType struct {
	Len  int
	Elem Type
}

type SliceType struct {
	Elem Type
}

type PointerType struct {
	Elem Type
}

type MapType struct {
	Key   Type
	Value Type
}

type FunctionType struct {
	Params    []Type
	Return    Type
	Errorable bool
}

func MakeArrayType(length int, elem Type) Type {
	return Type(fmt.Sprintf("[%d]%s", length, elem))
}

func MakeSliceType(elem Type) Type {
	return Type("[]" + string(elem))
}

func MakePointerType(elem Type) Type {
	return Type("*" + string(elem))
}

func MakeMapType(key, value Type) Type {
	return Type("map[" + string(key) + "]" + string(value))
}

func MakeFunctionType(params []Type, ret Type, errorable bool) Type {
	parts := make([]string, 0, len(params))
	for _, param := range params {
		parts = append(parts, string(param))
	}
	text := "fn(" + strings.Join(parts, ", ") + ") "
	if errorable {
		text += "!"
	}
	text += string(ret)
	return Type(text)
}

func ParseMapType(typ Type) (MapType, bool) {
	text := string(typ)
	if !strings.HasPrefix(text, "map[") {
		return MapType{}, false
	}
	depth := 0
	for i := 3; i < len(text); i++ {
		switch text[i] {
		case '[':
			depth++
		case ']':
			depth--
			if depth == 0 {
				key := Type(text[4:i])
				value := Type(text[i+1:])
				if key == TypeInvalid || value == TypeInvalid {
					return MapType{}, false
				}
				return MapType{Key: key, Value: value}, true
			}
		}
	}
	return MapType{}, false
}

func ParseArrayType(typ Type) (ArrayType, bool) {
	text := string(typ)
	if !strings.HasPrefix(text, "[") {
		return ArrayType{}, false
	}
	end := strings.IndexByte(text, ']')
	if end < 0 {
		return ArrayType{}, false
	}
	length, err := strconv.Atoi(text[1:end])
	if err != nil {
		return ArrayType{}, false
	}
	elem := Type(text[end+1:])
	if elem == TypeInvalid {
		return ArrayType{}, false
	}
	return ArrayType{Len: length, Elem: elem}, true
}

func ParseSliceType(typ Type) (SliceType, bool) {
	text := string(typ)
	if !strings.HasPrefix(text, "[]") {
		return SliceType{}, false
	}
	elem := Type(text[2:])
	if elem == TypeInvalid {
		return SliceType{}, false
	}
	return SliceType{Elem: elem}, true
}

func ParsePointerType(typ Type) (PointerType, bool) {
	text := string(typ)
	if !strings.HasPrefix(text, "*") {
		return PointerType{}, false
	}
	elem := Type(text[1:])
	if elem == TypeInvalid {
		return PointerType{}, false
	}
	return PointerType{Elem: elem}, true
}

func ParseFunctionType(typ Type) (FunctionType, bool) {
	text := string(typ)
	if !strings.HasPrefix(text, "fn(") {
		return FunctionType{}, false
	}
	depth := 1
	end := -1
	for i := 3; i < len(text); i++ {
		switch text[i] {
		case '(':
			depth++
		case ')':
			depth--
			if depth == 0 {
				end = i
				break
			}
		}
	}
	if end < 0 || end+2 > len(text) || text[end+1] != ' ' {
		return FunctionType{}, false
	}
	paramsText := text[3:end]
	rest := text[end+2:]
	errorable := false
	if strings.HasPrefix(rest, "!") {
		errorable = true
		rest = rest[1:]
	}
	if rest == "" {
		return FunctionType{}, false
	}
	params, ok := splitFunctionTypeParams(paramsText)
	if !ok {
		return FunctionType{}, false
	}
	return FunctionType{Params: params, Return: Type(rest), Errorable: errorable}, true
}

func splitFunctionTypeParams(text string) ([]Type, bool) {
	if strings.TrimSpace(text) == "" {
		return nil, true
	}
	var (
		params []Type
		start  int
		round  int
		square int
	)
	for i := 0; i < len(text); i++ {
		switch text[i] {
		case '(':
			round++
		case ')':
			if round == 0 {
				return nil, false
			}
			round--
		case '[':
			square++
		case ']':
			if square == 0 {
				return nil, false
			}
			square--
		case ',':
			if round == 0 && square == 0 {
				part := strings.TrimSpace(text[start:i])
				if part == "" {
					return nil, false
				}
				params = append(params, Type(part))
				start = i + 1
			}
		}
	}
	if round != 0 || square != 0 {
		return nil, false
	}
	part := strings.TrimSpace(text[start:])
	if part == "" {
		return nil, false
	}
	params = append(params, Type(part))
	return params, true
}

func methodReceiverBaseType(typ Type) (Type, bool) {
	if pointer, ok := ParsePointerType(typ); ok {
		typ = pointer.Elem
	}
	if typ == TypeInvalid {
		return TypeInvalid, false
	}
	return typ, true
}

func IsBuiltinFunction(name string) bool {
	switch name {
	case "print", "panic", "len", "append", "has", "delete", "keys", "chr", "i32_to_i64", "i64_to_i32", "to_str",
		"sb_new", "sb_write", "sb_string":
		return true
	default:
		return false
	}
}

func IsInternalBuiltin(name string) bool {
	switch name {
	case "chr", "i32_to_i64", "i64_to_i32":
		return true
	default:
		return false
	}
}

func IsHostIntrinsic(file, fullName string) bool {
	if !strings.HasPrefix(file, "<stdlib>/") {
		return false
	}
	switch fullName {
	case "fs.read_file", "fs.write_file", "fs.read_dir", "fs.stat", "fs.mkdir_all", "fs.remove_all", "fs.temp_dir":
		return true
	case "process.args", "process.run", "process.run_inherit", "env.lookup", "stdio.eprint":
		return true
	default:
		return false
	}
}

func isMapKeyType(typ Type) bool {
	switch typ {
	case TypeBool, TypeI32, TypeI64, TypeStr:
		return true
	default:
		return false
	}
}

func isMapType(typ Type) bool {
	_, ok := ParseMapType(typ)
	return ok
}

func Check(program *ast.Program) (Info, []diag.Diagnostic) {
	c := &Checker{
		functions: map[string]Signature{
			"print": {
				Name:     "print",
				FullName: "print",
				Params:   []Type{TypeStr},
				Return:   TypeVoid,
				Builtin:  true,
			},
			"panic": {
				Name:     "panic",
				FullName: "panic",
				Params:   []Type{TypeStr},
				Return:   TypeNoReturn,
				Builtin:  true,
			},
			"append": {
				Name:     "append",
				FullName: "append",
				Params:   []Type{TypeInvalid, TypeInvalid},
				Return:   TypeInvalid,
				Builtin:  true,
			},
			"has": {
				Name:     "has",
				FullName: "has",
				Params:   []Type{TypeInvalid, TypeInvalid},
				Return:   TypeBool,
				Builtin:  true,
			},
			"delete": {
				Name:     "delete",
				FullName: "delete",
				Params:   []Type{TypeInvalid, TypeInvalid},
				Return:   TypeVoid,
				Builtin:  true,
			},
			"keys": {
				Name:     "keys",
				FullName: "keys",
				Params:   []Type{TypeInvalid},
				Return:   TypeInvalid,
				Builtin:  true,
			},
			"chr": {
				Name:     "chr",
				FullName: "chr",
				Params:   []Type{TypeI32},
				Return:   TypeStr,
				Builtin:  true,
			},
			"i32_to_i64": {
				Name:     "i32_to_i64",
				FullName: "i32_to_i64",
				Params:   []Type{TypeI32},
				Return:   TypeI64,
				Builtin:  true,
			},
			"i64_to_i32": {
				Name:     "i64_to_i32",
				FullName: "i64_to_i32",
				Params:   []Type{TypeI64},
				Return:   TypeI32,
				Builtin:  true,
			},
			"sb_new": {
				Name:     "sb_new",
				FullName: "sb_new",
				Params:   nil,
				Return:   TypeI64,
				Builtin:  true,
			},
			"sb_write": {
				Name:     "sb_write",
				FullName: "sb_write",
				Params:   []Type{TypeI64, TypeStr},
				Return:   TypeVoid,
				Builtin:  true,
			},
			"sb_string": {
				Name:     "sb_string",
				FullName: "sb_string",
				Params:   []Type{TypeI64},
				Return:   TypeStr,
				Builtin:  true,
			},
		},
		methods:    make(map[Type]map[string]Signature),
		structs:    make(map[string]*ast.StructDecl),
		interfaces: make(map[string]*ast.InterfaceDecl),
		enums:      make(map[string]*ast.EnumDecl),
		info: Info{
			Functions:        make(map[*ast.FunctionDecl]Signature),
			FunctionLiterals: make(map[*ast.FunctionLiteralExpr]FunctionLiteralInfo),
			Calls:            make(map[*ast.CallExpr]Signature),
			ExprTypes:        make(map[ast.Expression]ExprType),
			Locals:           make(map[ast.Node]Type),
			Structs:          make(map[string]StructInfo),
			Interfaces:       make(map[string]InterfaceInfo),
			Enums:            make(map[string]EnumInfo),
			ErrorCodes:       make(map[string]int),
			AutoDeref:        make(map[*ast.SelectorExpr]bool),
			EnumConstructors: make(map[*ast.CallExpr]EnumConstructorInfo),
		},
	}

	c.checkProgram(program)
	return c.info, c.diag.Items()
}

func (c *Checker) checkProgram(program *ast.Program) {
	if program == nil {
		c.diag.Add(token.Position{Line: 1, Column: 1}, "missing program")
		return
	}
	if program.PackageName != "main" {
		c.diag.Add(program.Pos(), "package must be main")
	}

	for _, decl := range program.Structs {
		if _, exists := c.structs[decl.Name]; exists {
			c.diag.Add(decl.NamePos, "struct %q is already declared", decl.Name)
			continue
		}
		if _, exists := c.interfaces[decl.Name]; exists {
			c.diag.Add(decl.NamePos, "type %q is already declared", decl.Name)
			continue
		}
		if _, exists := c.enums[decl.Name]; exists {
			c.diag.Add(decl.NamePos, "type %q is already declared", decl.Name)
			continue
		}
		c.structs[decl.Name] = decl
	}
	for _, decl := range program.Interfaces {
		if _, exists := c.interfaces[decl.Name]; exists {
			c.diag.Add(decl.NamePos, "interface %q is already declared", decl.Name)
			continue
		}
		if _, exists := c.structs[decl.Name]; exists {
			c.diag.Add(decl.NamePos, "type %q is already declared", decl.Name)
			continue
		}
		if _, exists := c.enums[decl.Name]; exists {
			c.diag.Add(decl.NamePos, "type %q is already declared", decl.Name)
			continue
		}
		c.interfaces[decl.Name] = decl
	}
	for _, decl := range program.Enums {
		if _, exists := c.enums[decl.Name]; exists {
			c.diag.Add(decl.NamePos, "enum %q is already declared", decl.Name)
			continue
		}
		if _, exists := c.interfaces[decl.Name]; exists {
			c.diag.Add(decl.NamePos, "type %q is already declared", decl.Name)
			continue
		}
		if _, exists := c.structs[decl.Name]; exists {
			c.diag.Add(decl.NamePos, "type %q is already declared", decl.Name)
			continue
		}
		c.enums[decl.Name] = decl
	}

	for _, decl := range program.Structs {
		if _, exists := c.info.Structs[decl.Name]; exists {
			continue
		}
		info := StructInfo{Name: decl.Name}
		seenFields := make(map[string]struct{})
		for _, field := range decl.Fields {
			if _, exists := seenFields[field.Name]; exists {
				c.diag.Add(field.NamePos, "field %q is already declared in struct %q", field.Name, decl.Name)
				continue
			}
			seenFields[field.Name] = struct{}{}

			fieldType := c.resolveTypeRef(field.Type)
			if fieldType == TypeVoid || fieldType == TypeNoReturn {
				c.diag.Add(field.Type.Pos, "field %q cannot use type %q", field.Name, field.Type.String())
				continue
			}
			info.Fields = append(info.Fields, StructField{
				Name: field.Name,
				Type: fieldType,
			})
		}
		c.info.Structs[decl.Name] = info
	}

	for _, decl := range program.Interfaces {
		if _, exists := c.info.Interfaces[decl.Name]; exists {
			continue
		}
		info := InterfaceInfo{Name: decl.Name}
		seenMethods := make(map[string]struct{})
		for _, method := range decl.Methods {
			if _, exists := seenMethods[method.Name]; exists {
				c.diag.Add(method.NamePos, "method %q is already declared in interface %q", method.Name, decl.Name)
				continue
			}
			seenMethods[method.Name] = struct{}{}

			methodInfo := InterfaceMethodInfo{
				Name:      method.Name,
				Return:    c.resolveTypeRef(method.Return),
				Errorable: method.ReturnIsBang,
			}
			if methodInfo.Return == TypeNoReturn && methodInfo.Errorable {
				c.diag.Add(method.Return.Pos, "noreturn methods cannot also be errorable")
				continue
			}
			if methodInfo.Return == TypeError && methodInfo.Errorable {
				c.diag.Add(method.Return.Pos, "error methods cannot also be errorable")
				continue
			}
			valid := true
			for _, param := range method.Params {
				paramType := c.resolveTypeRef(param.Type)
				if paramType == TypeVoid || paramType == TypeNoReturn || paramType == TypeInvalid {
					c.diag.Add(param.Type.Pos, "parameter %q cannot use type %q", param.Name, param.Type.String())
					valid = false
					break
				}
				methodInfo.Params = append(methodInfo.Params, paramType)
			}
			if !valid {
				continue
			}
			info.Methods = append(info.Methods, methodInfo)
		}
		c.info.Interfaces[decl.Name] = info
	}

	for _, decl := range program.Enums {
		if _, exists := c.info.Enums[decl.Name]; exists {
			continue
		}
		info := EnumInfo{Name: decl.Name}
		seenCases := make(map[string]struct{})
		for i, enumCase := range decl.Cases {
			if _, exists := seenCases[enumCase.Name]; exists {
				c.diag.Add(enumCase.NamePos, "case %q is already declared in enum %q", enumCase.Name, decl.Name)
				continue
			}
			seenCases[enumCase.Name] = struct{}{}

			caseInfo := EnumCaseInfo{
				Name: enumCase.Name,
				Tag:  i,
			}
			if len(enumCase.Fields) > 0 {
				payloadName := enumPayloadTypeName(decl.Name, enumCase.Name)
				payloadInfo := StructInfo{Name: string(payloadName)}
				seenFields := make(map[string]struct{})
				for _, field := range enumCase.Fields {
					if _, exists := seenFields[field.Name]; exists {
						c.diag.Add(field.NamePos, "field %q is already declared in enum case %q", field.Name, enumCase.Name)
						continue
					}
					seenFields[field.Name] = struct{}{}

					fieldType := c.resolveTypeRef(field.Type)
					if fieldType == TypeVoid || fieldType == TypeNoReturn {
						c.diag.Add(field.Type.Pos, "field %q cannot use type %q", field.Name, field.Type.String())
						continue
					}
					payloadInfo.Fields = append(payloadInfo.Fields, StructField{
						Name: field.Name,
						Type: fieldType,
					})
				}
				c.info.Structs[string(payloadName)] = payloadInfo
				caseInfo.PayloadType = payloadName
				caseInfo.Fields = payloadInfo.Fields
			}
			info.Cases = append(info.Cases, caseInfo)
		}
		c.info.Enums[decl.Name] = info
	}

	c.checkTypeCycles()

	for _, fn := range program.Functions {
		if fn.Receiver == nil && IsBuiltinFunction(fn.Name) {
			c.diag.Add(fn.NamePos, "function %q is already declared", fn.Name)
			continue
		}
		sig := Signature{
			Name:      fn.Name,
			FullName:  fn.Name,
			Return:    c.resolveTypeRef(fn.Return),
			Errorable: fn.ReturnIsBang,
			Exported:  fn.Exported,
		}
		if fn.Receiver != nil {
			sig.Method = true
			sig.Receiver = c.resolveMethodReceiverType(fn.Receiver.Type)
			if sig.Receiver == TypeInvalid {
				continue
			}
			sig.FullName = methodFullName(sig.Receiver, fn.Name)
			sig.Package = packageForMethodReceiver(sig.Receiver)
			sig.Params = append(sig.Params, sig.Receiver)
		} else {
			if _, exists := c.functions[fn.Name]; exists {
				c.diag.Add(fn.NamePos, "function %q is already declared", fn.Name)
				continue
			}
			sig.Package = packageForFunction(sig.FullName)
		}
		sig.HostIntrinsic = IsHostIntrinsic(fn.NamePos.File, sig.FullName)
		if sig.Return == TypeNoReturn && sig.Errorable {
			c.diag.Add(fn.Return.Pos, "noreturn functions cannot also be errorable")
		}
		if sig.Return == TypeError && sig.Errorable {
			c.diag.Add(fn.Return.Pos, "error functions cannot also be errorable")
		}
		for _, param := range fn.Params {
			sig.Params = append(sig.Params, c.resolveTypeRef(param.Type))
		}
		if sig.Method {
			base, _ := methodReceiverBaseType(sig.Receiver)
			if _, ok := c.lookupMethodForBase(base, fn.Name); ok {
				c.diag.Add(fn.NamePos, "method %q is already declared for %q", fn.Name, base)
				continue
			}
			if _, ok := c.methods[sig.Receiver]; !ok {
				c.methods[sig.Receiver] = make(map[string]Signature)
			}
			c.methods[sig.Receiver][fn.Name] = sig
		} else {
			c.functions[fn.Name] = sig
		}
		c.info.Functions[fn] = sig
	}

	mainSig, ok := c.functions["main"]
	if !ok || mainSig.Builtin {
		c.diag.Add(program.Pos(), "missing main function")
	} else if mainSig.Return != TypeI32 {
		c.diag.Add(program.Pos(), "main must return i32 or !i32")
	}

	for _, fn := range program.Functions {
		sig, ok := c.info.Functions[fn]
		if !ok {
			continue
		}
		c.checkFunction(fn, sig)
	}

	var ordered []string
	for name := range c.info.ErrorCodes {
		ordered = append(ordered, name)
	}
	sort.Strings(ordered)
	c.info.OrderedErrors = ordered
	for i, name := range ordered {
		c.info.ErrorCodes[name] = i + 1
	}
}

func (c *Checker) checkTypeCycles() {
	visiting := make(map[string]bool)
	visited := make(map[string]bool)

	var visit func(name string)
	visit = func(name string) {
		if visited[name] {
			return
		}
		if visiting[name] {
			if decl, ok := c.structs[name]; ok {
				c.diag.Add(decl.NamePos, "struct %q cannot contain itself recursively", name)
			} else if decl, ok := c.enums[name]; ok {
				c.diag.Add(decl.NamePos, "enum %q cannot contain itself recursively", name)
			}
			return
		}
		visiting[name] = true
		if info, ok := c.info.Structs[name]; ok {
			for _, field := range info.Fields {
				for _, dep := range c.typeDependencies(field.Type) {
					visit(dep)
				}
			}
		}
		if info, ok := c.info.Enums[name]; ok {
			for _, enumCase := range info.Cases {
				for _, field := range enumCase.Fields {
					for _, dep := range c.typeDependencies(field.Type) {
						visit(dep)
					}
				}
			}
		}
		visiting[name] = false
		visited[name] = true
	}

	for name := range c.structs {
		visit(name)
	}
	for name := range c.enums {
		visit(name)
	}
}

func (c *Checker) typeDependencies(typ Type) []string {
	if array, ok := ParseArrayType(typ); ok {
		return c.typeDependencies(array.Elem)
	}
	if _, ok := ParseSliceType(typ); ok {
		return nil
	}
	if _, ok := ParsePointerType(typ); ok {
		return nil
	}
	if _, ok := ParseMapType(typ); ok {
		return nil
	}
	if _, ok := c.info.Interfaces[string(typ)]; ok {
		return nil
	}
	if _, ok := c.info.Structs[string(typ)]; ok {
		return []string{string(typ)}
	}
	if _, ok := c.info.Enums[string(typ)]; ok {
		return []string{string(typ)}
	}
	return nil
}

func (c *Checker) checkFunction(fn *ast.FunctionDecl, sig Signature) {
	ctx := &functionContext{
		signature: sig,
		scopes:    []map[string]localBinding{{}},
	}
	c.current = ctx
	defer func() {
		c.current = nil
	}()

	paramIndex := 0
	if fn.Receiver != nil {
		ctx.scopes[0][fn.Receiver.Name] = localBinding{typ: sig.Params[0], owner: ctx, node: fn}
		paramIndex = 1
	}

	for i, param := range fn.Params {
		paramType := sig.Params[i+paramIndex]
		if paramType == TypeVoid || paramType == TypeNoReturn || paramType == TypeInvalid {
			c.diag.Add(param.Type.Pos, "parameter %q cannot use type %q", param.Name, param.Type.String())
			continue
		}
		if _, exists := ctx.scopes[0][param.Name]; exists {
			c.diag.Add(param.NamePos, "duplicate parameter %q", param.Name)
			continue
		}
		ctx.scopes[0][param.Name] = localBinding{typ: paramType, owner: ctx, node: fn}
	}

	c.checkBlock(fn.Body)
	if sig.Return == TypeVoid {
		return
	}
	if !c.blockDefinitelyReturns(fn.Body) {
		if sig.Return == TypeNoReturn {
			c.diag.Add(fn.NamePos, "function %q must not fall through", fn.Name)
			return
		}
		c.diag.Add(fn.NamePos, "function %q must return a value on all paths", fn.Name)
	}
}

func (c *Checker) checkBlock(block *ast.BlockStmt) {
	c.pushScope()
	defer c.popScope()
	for _, stmt := range block.Stmts {
		c.checkStatement(stmt)
	}
}

func (c *Checker) checkBlockWithErrorBinding(block *ast.BlockStmt, name string) {
	c.pushScope()
	defer c.popScope()
	c.bindLocal(name, TypeError, block)
	for _, stmt := range block.Stmts {
		c.checkStatement(stmt)
	}
}

func (c *Checker) checkStatement(stmt ast.Statement) {
	switch s := stmt.(type) {
	case *ast.BlockStmt:
		c.checkBlock(s)
	case *ast.LetStmt:
		value := c.checkExpression(s.Value)
		value = c.requireNonErrorableValue(s.Value, value, "errorable value cannot be bound to a local")
		if value.Base == TypeInvalid {
			return
		}
		if value.Base == TypeNil {
			c.diag.Add(s.Value.Pos(), "cannot infer type from nil without a pointer context")
			return
		}
		if value.Base == TypeUntypedInt {
			defaultType := c.defaultUntypedIntegerType(s.Value)
			if defaultType == TypeInvalid {
				c.diag.Add(s.Value.Pos(), "integer literal does not fit a supported integer type")
				return
			}
			value = c.coerceUntypedInteger(s.Value, value, defaultType)
		}
		if c.scopeOwns(s.Name) {
			c.diag.Add(s.NamePos, "local %q is already declared in this scope", s.Name)
			return
		}
		c.bindLocal(s.Name, value.Base, s)
		c.info.Locals[s] = value.Base
	case *ast.VarStmt:
		declaredType := c.resolveTypeRef(s.Type)
		if declaredType == TypeVoid || declaredType == TypeNoReturn || declaredType == TypeInvalid {
			c.diag.Add(s.Type.Pos, "local %q cannot use type %q", s.Name, s.Type.String())
			return
		}
		if c.scopeOwns(s.Name) {
			c.diag.Add(s.NamePos, "local %q is already declared in this scope", s.Name)
			return
		}
		if s.Value != nil {
			value := c.checkExpression(s.Value)
			value = c.requireNonErrorableValue(s.Value, value, "errorable value cannot be bound to a local")
			if value.Base == TypeInvalid {
				return
			}
			value = c.coerceValue(s.Value, value, declaredType)
			if value.Base != declaredType {
				c.diag.Add(s.Value.Pos(), "cannot assign %s to %s", value.Base, declaredType)
				return
			}
		}
		c.bindLocal(s.Name, declaredType, s)
		c.info.Locals[s] = declaredType
	case *ast.AssignStmt:
		if mapAssignType, ok := c.checkMapAssignment(s); ok {
			if mapAssignType == TypeInvalid {
				return
			}
			value := c.checkExpression(s.Value)
			value = c.requireNonErrorableValue(s.Value, value, "errorable value cannot be assigned directly")
			if value.Base == TypeInvalid {
				return
			}
			value = c.coerceValue(s.Value, value, mapAssignType)
			if value.Base != mapAssignType {
				c.diag.Add(s.Value.Pos(), "cannot assign %s to %s", value.Base, mapAssignType)
			}
			return
		}
		targetType := c.checkAssignmentTarget(s.Target)
		if targetType == TypeInvalid {
			return
		}
		value := c.checkExpression(s.Value)
		value = c.requireNonErrorableValue(s.Value, value, "errorable value cannot be assigned directly")
		if value.Base == TypeInvalid {
			return
		}
		value = c.coerceValue(s.Value, value, targetType)
		if value.Base != targetType {
			c.diag.Add(s.Value.Pos(), "cannot assign %s to %s", value.Base, targetType)
		}
	case *ast.IfStmt:
		c.checkCondition(s.Cond, "if condition must be bool", "if condition cannot be errorable")
		c.checkBlock(s.Then)
		if s.Else != nil {
			c.checkStatement(s.Else)
		}
	case *ast.ForStmt:
		c.checkFor(s)
	case *ast.MatchStmt:
		c.checkMatch(s)
	case *ast.BreakStmt:
		if c.current.loopDepth == 0 {
			c.diag.Add(s.BreakPos, "break can only be used inside a loop")
		}
	case *ast.ContinueStmt:
		if c.current.loopDepth == 0 {
			c.diag.Add(s.ContinuePos, "continue can only be used inside a loop")
		}
	case *ast.ReturnStmt:
		c.checkReturn(s)
	case *ast.ExprStmt:
		exprType := c.checkExpression(s.Expr)
		if exprType.Errorable {
			c.diag.Add(s.Expr.Pos(), "errorable value cannot be used as a statement")
		}
	default:
		c.diag.Add(stmt.Pos(), "unsupported statement")
	}
}

func (c *Checker) checkFor(stmt *ast.ForStmt) {
	c.pushScope()
	defer c.popScope()

	if stmt.Init != nil {
		c.checkForClauseStatement(stmt.Init)
	}
	if stmt.Cond == nil {
		c.diag.Add(stmt.ForPos, "for loop requires a condition")
	} else {
		c.checkCondition(stmt.Cond, "for condition must be bool", "for condition cannot be errorable")
	}
	if stmt.Post != nil {
		c.checkForClauseStatement(stmt.Post)
	}

	c.current.loopDepth++
	c.checkBlock(stmt.Body)
	c.current.loopDepth--
}

func (c *Checker) checkMatch(stmt *ast.MatchStmt) {
	value := c.checkExpression(stmt.Value)
	if value.Errorable {
		c.diag.Add(stmt.Value.Pos(), "match value cannot be errorable")
		return
	}

	enumInfo, ok := c.info.Enums[string(value.Base)]
	if !ok {
		c.diag.Add(stmt.Value.Pos(), "match requires an enum value")
		return
	}

	seen := make(map[string]struct{})
	for _, arm := range stmt.Arms {
		armEnum := c.resolveTypeRef(arm.EnumType)
		if armEnum != value.Base {
			c.diag.Add(arm.EnumType.Pos, "match arm must use enum %q", enumInfo.Name)
		}

		enumCase, _, ok := enumInfo.Case(arm.CaseName)
		if !ok {
			c.diag.Add(arm.CaseNamePos, "enum %q has no case %q", enumInfo.Name, arm.CaseName)
			c.checkBlock(arm.Body)
			continue
		}
		if _, exists := seen[enumCase.Name]; exists {
			c.diag.Add(arm.CaseNamePos, "duplicate match arm for %q", enumCase.Name)
		}
		seen[enumCase.Name] = struct{}{}

		switch {
		case len(enumCase.Fields) == 0 && (arm.BindName != "" || arm.BindIgnore):
			c.diag.Add(arm.BindNamePos, "plain enum case %q cannot bind a payload", enumCase.Name)
			c.checkBlock(arm.Body)
		case len(enumCase.Fields) > 0 && arm.BindName != "" && !arm.BindIgnore:
			c.pushScope()
			c.bindLocal(arm.BindName, enumCase.PayloadType, arm.Body)
			for _, nested := range arm.Body.Stmts {
				c.checkStatement(nested)
			}
			c.popScope()
		default:
			c.checkBlock(arm.Body)
		}
	}

	if stmt.ElseBody != nil {
		c.checkBlock(stmt.ElseBody)
		return
	}

	if len(seen) == len(enumInfo.Cases) {
		return
	}
	missing := make([]string, 0, len(enumInfo.Cases)-len(seen))
	for _, enumCase := range enumInfo.Cases {
		if _, ok := seen[enumCase.Name]; ok {
			continue
		}
		missing = append(missing, enumInfo.Name+"."+enumCase.Name)
	}
	c.diag.Add(stmt.MatchPos, "match on %q is not exhaustive; missing %s", enumInfo.Name, strings.Join(missing, ", "))
}

func (c *Checker) checkForClauseStatement(stmt ast.Statement) {
	switch stmt.(type) {
	case *ast.LetStmt, *ast.VarStmt, *ast.AssignStmt, *ast.ExprStmt:
		c.checkStatement(stmt)
	default:
		c.diag.Add(stmt.Pos(), "for clause must be a declaration, assignment, or expression")
	}
}

func (c *Checker) checkCondition(expr ast.Expression, typeMessage, errorMessage string) {
	cond := c.checkExpression(expr)
	if cond.Errorable {
		c.diag.Add(expr.Pos(), "%s", errorMessage)
	}
	if cond.Base != TypeBool {
		c.diag.Add(expr.Pos(), "%s", typeMessage)
	}
}

func (c *Checker) checkMapAssignment(stmt *ast.AssignStmt) (Type, bool) {
	indexExpr, ok := stmt.Target.(*ast.IndexExpr)
	if !ok {
		return TypeInvalid, false
	}
	inner := c.checkExpression(indexExpr.Inner)
	if inner.Errorable {
		c.diag.Add(indexExpr.LBracketPos, "indexing cannot use an errorable value")
		return TypeInvalid, true
	}
	mapType, ok := ParseMapType(inner.Base)
	if !ok {
		return TypeInvalid, false
	}
	keyType := c.checkExpression(indexExpr.Index)
	if keyType.Errorable {
		c.diag.Add(indexExpr.Index.Pos(), "map key expression cannot be errorable")
		return TypeInvalid, true
	}
	keyType = c.coerceValue(indexExpr.Index, keyType, mapType.Key)
	if keyType.Base != mapType.Key {
		c.diag.Add(indexExpr.Index.Pos(), "map key must be %s, got %s", mapType.Key, keyType.Base)
		return TypeInvalid, true
	}
	c.info.ExprTypes[stmt.Target] = ExprType{Base: mapType.Value}
	return mapType.Value, true
}

func (c *Checker) checkAssignmentTarget(target ast.Expression) Type {
	typ, ok := c.checkAddressableExpr(target, false)
	if ok {
		return typ
	}
	c.diag.Add(target.Pos(), "invalid assignment target")
	return TypeInvalid
}

func (c *Checker) checkReturn(stmt *ast.ReturnStmt) {
	sig := c.current.signature
	if sig.Return == TypeNoReturn {
		c.diag.Add(stmt.ReturnPos, "noreturn functions cannot return")
		return
	}
	if stmt.Value == nil {
		if sig.Errorable && sig.Return == TypeVoid {
			return
		}
		if sig.Return == TypeVoid && !sig.Errorable {
			return
		}
		c.diag.Add(stmt.ReturnPos, "return value is required")
		return
	}

	if errLit, ok := stmt.Value.(*ast.ErrorLiteral); ok {
		if !sig.Errorable && sig.Return != TypeError {
			c.diag.Add(errLit.Pos(), "cannot return %s from function returning %s", formatErrorName(errLit.Name), sig.Return)
			return
		}
		c.useErrorName(errLit.Name)
		c.info.ExprTypes[stmt.Value] = ExprType{Base: TypeError}
		return
	}

	value := c.checkExpression(stmt.Value)
	if value.Errorable {
		if sig.Errorable && value.Base == sig.Return {
			return
		}
		c.diag.Add(stmt.Value.Pos(), "return cannot use an errorable value directly")
		return
	}
	if value.Base == TypeNoReturn || value.Base == TypeVoid {
		c.diag.Add(stmt.Value.Pos(), "return requires a value")
		return
	}
	value = c.coerceValue(stmt.Value, value, sig.Return)
	if value.Base != sig.Return {
		c.diag.Add(stmt.Value.Pos(), "cannot return %s from function returning %s", value.Base, sig.Return)
	}
}

func (c *Checker) checkExpression(expr ast.Expression) ExprType {
	if expr == nil {
		return ExprType{Base: TypeInvalid}
	}
	switch e := expr.(type) {
	case *ast.IdentExpr:
		binding, ok, captured := c.lookupLocalBinding(e.Name)
		if !ok {
			c.diag.Add(e.NamePos, "unknown local %q", e.Name)
			return ExprType{Base: TypeInvalid}
		}
		if captured {
			c.recordCapture(e.Name, binding.typ)
		}
		et := ExprType{Base: binding.typ}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.IntLiteral:
		et := ExprType{Base: TypeUntypedInt}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.CharLiteral:
		et := ExprType{Base: TypeI32}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.StringLiteral:
		et := ExprType{Base: TypeStr}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.BoolLiteral:
		et := ExprType{Base: TypeBool}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.NilLiteral:
		et := ExprType{Base: TypeNil}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.ErrorLiteral:
		c.useErrorName(e.Name)
		et := ExprType{Base: TypeError}
		c.info.ExprTypes[expr] = et
		return et
	case *ast.GroupExpr:
		et := c.checkExpression(e.Inner)
		c.info.ExprTypes[expr] = et
		return et
	case *ast.FunctionLiteralExpr:
		return c.checkFunctionLiteral(expr, e)
	case *ast.TypeApplicationExpr:
		c.diag.Add(e.LBracketPos, "type arguments are not valid in expression position")
		return ExprType{Base: TypeInvalid}
	case *ast.UnaryExpr:
		return c.checkUnary(expr, e)
	case *ast.PropagateExpr:
		return c.checkPropagate(expr, e)
	case *ast.HandleExpr:
		return c.checkHandle(expr, e)
	case *ast.BinaryExpr:
		return c.checkBinary(expr, e)
	case *ast.SelectorExpr:
		return c.checkSelector(expr, e)
	case *ast.IndexExpr:
		return c.checkIndex(expr, e)
	case *ast.SliceExpr:
		return c.checkSlice(expr, e)
	case *ast.StructLiteralExpr:
		return c.checkStructLiteral(expr, e)
	case *ast.ArrayLiteralExpr:
		return c.checkArrayLiteral(expr, e)
	case *ast.SliceLiteralExpr:
		return c.checkSliceLiteral(expr, e)
	case *ast.MapLiteralExpr:
		return c.checkMapLiteral(expr, e)
	case *ast.CallExpr:
		return c.checkCall(expr, e)
	default:
		c.diag.Add(expr.Pos(), "unsupported expression")
		return ExprType{Base: TypeInvalid}
	}
}

func (c *Checker) checkUnary(expr ast.Expression, unary *ast.UnaryExpr) ExprType {
	switch unary.Operator {
	case token.Amp:
		if name, ok := c.addressRootCapturedOuterLocal(unary.Inner); ok {
			c.diag.Add(unary.OpPos, "captured outer local %q is not addressable", name)
			return ExprType{Base: TypeInvalid}
		}
		typ, ok := c.checkAddressableExpr(unary.Inner, true)
		if !ok {
			c.diag.Add(unary.OpPos, "address-of requires an addressable operand or composite literal")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: MakePointerType(typ)}
		c.info.ExprTypes[expr] = et
		return et
	case token.Star:
		inner := c.checkExpression(unary.Inner)
		if inner.Errorable {
			c.diag.Add(unary.OpPos, "dereference cannot use an errorable operand")
			return ExprType{Base: TypeInvalid}
		}
		pointer, ok := ParsePointerType(inner.Base)
		if !ok {
			c.diag.Add(unary.OpPos, "dereference requires a pointer operand")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: pointer.Elem}
		c.info.ExprTypes[expr] = et
		return et
	case token.Minus:
		inner := c.checkExpression(unary.Inner)
		if inner.Errorable {
			c.diag.Add(unary.OpPos, "unary operators cannot use errorable operands")
			return ExprType{Base: TypeInvalid}
		}
		if !isIntegerType(inner.Base) {
			c.diag.Add(unary.OpPos, "unary - requires an integer operand")
			return ExprType{Base: TypeInvalid}
		}
		c.info.ExprTypes[expr] = inner
		return inner
	case token.Bang:
		inner := c.checkExpression(unary.Inner)
		if inner.Errorable {
			c.diag.Add(unary.OpPos, "unary operators cannot use errorable operands")
			return ExprType{Base: TypeInvalid}
		}
		if inner.Base != TypeBool {
			c.diag.Add(unary.OpPos, "unary ! requires a bool operand")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeBool}
		c.info.ExprTypes[expr] = et
		return et
	default:
		c.diag.Add(unary.OpPos, "unsupported unary operator")
		return ExprType{Base: TypeInvalid}
	}
}

func (c *Checker) checkSelector(expr ast.Expression, selector *ast.SelectorExpr) ExprType {
	if ident, ok := selector.Inner.(*ast.IdentExpr); ok {
		if _, exists := c.lookupLocal(ident.Name); !exists {
			if enumInfo, ok := c.info.Enums[ident.Name]; ok {
				enumCase, _, ok := enumInfo.Case(selector.Name)
				if !ok {
					c.diag.Add(selector.NamePos, "enum %q has no case %q", enumInfo.Name, selector.Name)
					return ExprType{Base: TypeInvalid}
				}
				if len(enumCase.Fields) > 0 {
					c.diag.Add(selector.NamePos, "payload case %q requires a constructor body", selector.Name)
					return ExprType{Base: TypeInvalid}
				}
				et := ExprType{Base: Type(enumInfo.Name)}
				c.info.ExprTypes[expr] = et
				return et
			}
		}
	}

	inner := c.checkExpression(selector.Inner)
	if inner.Errorable {
		c.diag.Add(selector.DotPos, "field access cannot use an errorable value")
		return ExprType{Base: TypeInvalid}
	}

	base := inner.Base
	if pointer, ok := ParsePointerType(base); ok {
		if _, isStruct := c.info.Structs[string(pointer.Elem)]; isStruct {
			base = pointer.Elem
			c.info.AutoDeref[selector] = true
		}
	}

	info, ok := c.info.Structs[string(base)]
	if ok {
		field, _, hasField := info.Field(selector.Name)
		if hasField {
			et := ExprType{Base: field.Type}
			c.info.ExprTypes[expr] = et
			return et
		}
	}
	if iface, ok := c.info.Interfaces[string(base)]; ok {
		if _, _, ok := iface.Method(selector.Name); ok {
			c.diag.Add(selector.NamePos, "method values are not supported")
			return ExprType{Base: TypeInvalid}
		}
		c.diag.Add(selector.NamePos, "interface %q has no method %q", iface.Name, selector.Name)
		return ExprType{Base: TypeInvalid}
	}
	if _, ok := c.lookupMethod(base, selector.Name); ok {
		c.diag.Add(selector.NamePos, "method values are not supported")
		return ExprType{Base: TypeInvalid}
	}
	if !ok {
		c.diag.Add(selector.DotPos, "field access requires a struct value")
		return ExprType{Base: TypeInvalid}
	}
	c.diag.Add(selector.NamePos, "struct %q has no field %q", info.Name, selector.Name)
	return ExprType{Base: TypeInvalid}
}

func (c *Checker) checkIndex(expr ast.Expression, index *ast.IndexExpr) ExprType {
	inner := c.checkExpression(index.Inner)
	if inner.Errorable {
		c.diag.Add(index.LBracketPos, "indexing cannot use an errorable value")
		return ExprType{Base: TypeInvalid}
	}

	if mapType, ok := ParseMapType(inner.Base); ok {
		return c.checkMapIndex(expr, index, mapType)
	}

	if inner.Base == TypeStr {
		indexType := c.checkExpression(index.Index)
		if indexType.Errorable {
			c.diag.Add(index.Index.Pos(), "index expression cannot be errorable")
			return ExprType{Base: TypeInvalid}
		}
		if !isIntegerType(indexType.Base) {
			c.diag.Add(index.Index.Pos(), "index expression must be an integer")
			return ExprType{Base: TypeInvalid}
		}
		if indexType.Base == TypeUntypedInt {
			indexType = c.coerceUntypedInteger(index.Index, indexType, TypeI32)
			if indexType.Base == TypeUntypedInt {
				c.diag.Add(index.Index.Pos(), "index expression must fit in i32")
				return ExprType{Base: TypeInvalid}
			}
		}
		et := ExprType{Base: TypeI32}
		c.info.ExprTypes[expr] = et
		return et
	}

	elemType, ok := sequenceElementType(inner.Base)
	if !ok {
		c.diag.Add(index.LBracketPos, "indexing requires an array, slice, map, or str value")
		return ExprType{Base: TypeInvalid}
	}

	indexType := c.checkExpression(index.Index)
	if indexType.Errorable {
		c.diag.Add(index.Index.Pos(), "index expression cannot be errorable")
		return ExprType{Base: TypeInvalid}
	}
	if !isIntegerType(indexType.Base) {
		c.diag.Add(index.Index.Pos(), "index expression must be an integer")
		return ExprType{Base: TypeInvalid}
	}
	if indexType.Base == TypeUntypedInt {
		indexType = c.coerceUntypedInteger(index.Index, indexType, TypeI32)
		if indexType.Base == TypeUntypedInt {
			c.diag.Add(index.Index.Pos(), "index expression must fit in i32")
			return ExprType{Base: TypeInvalid}
		}
	}

	et := ExprType{Base: elemType}
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) checkMapIndex(expr ast.Expression, index *ast.IndexExpr, mapType MapType) ExprType {
	keyType := c.checkExpression(index.Index)
	if keyType.Errorable {
		c.diag.Add(index.Index.Pos(), "map key expression cannot be errorable")
		return ExprType{Base: TypeInvalid}
	}
	keyType = c.coerceValue(index.Index, keyType, mapType.Key)
	if keyType.Base != mapType.Key {
		c.diag.Add(index.Index.Pos(), "map key must be %s, got %s", mapType.Key, keyType.Base)
		return ExprType{Base: TypeInvalid}
	}
	c.useErrorName("MissingKey")
	et := ExprType{Base: mapType.Value, Errorable: true}
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) checkSlice(expr ast.Expression, slice *ast.SliceExpr) ExprType {
	inner := c.checkExpression(slice.Inner)
	if inner.Errorable {
		c.diag.Add(slice.LBracketPos, "slicing cannot use an errorable value")
		return ExprType{Base: TypeInvalid}
	}

	if inner.Base == TypeStr {
		if !c.checkSliceBound(slice.Start) || !c.checkSliceBound(slice.End) {
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeStr}
		c.info.ExprTypes[expr] = et
		return et
	}

	sliceType, ok := ParseSliceType(inner.Base)
	if !ok {
		c.diag.Add(slice.LBracketPos, "slicing requires a slice or str value")
		return ExprType{Base: TypeInvalid}
	}

	if !c.checkSliceBound(slice.Start) || !c.checkSliceBound(slice.End) {
		return ExprType{Base: TypeInvalid}
	}

	et := ExprType{Base: MakeSliceType(sliceType.Elem)}
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) checkStructLiteral(expr ast.Expression, lit *ast.StructLiteralExpr) ExprType {
	if enumInfo, enumCase, ok := c.lookupEnumCaseType(lit.Type.String()); ok {
		if len(enumCase.Fields) == 0 {
			c.diag.Add(lit.Type.Pos, "plain enum case %q cannot use a constructor body", enumCase.Name)
			return ExprType{Base: TypeInvalid}
		}

		seen := make(map[string]struct{})
		for _, field := range lit.Fields {
			if _, exists := seen[field.Name]; exists {
				c.diag.Add(field.NamePos, "field %q is already initialized", field.Name)
				continue
			}
			seen[field.Name] = struct{}{}

			fieldInfo, _, ok := enumCase.Field(field.Name)
			if !ok {
				c.diag.Add(field.NamePos, "enum case %q has no field %q", enumCase.Name, field.Name)
				continue
			}
			value := c.checkExpression(field.Value)
			value = c.requireNonErrorableValue(field.Value, value, "errorable value cannot be used in an enum constructor")
			if value.Base == TypeInvalid {
				continue
			}
			value = c.coerceValue(field.Value, value, fieldInfo.Type)
			if value.Base != fieldInfo.Type {
				c.diag.Add(field.Value.Pos(), "cannot assign %s to %s", value.Base, fieldInfo.Type)
			}
		}

		et := ExprType{Base: Type(enumInfo.Name)}
		c.info.ExprTypes[expr] = et
		return et
	}

	typ := c.resolveTypeRef(lit.Type)
	info, ok := c.info.Structs[string(typ)]
	if !ok {
		c.diag.Add(lit.Type.Pos, "struct literal requires a struct type")
		return ExprType{Base: TypeInvalid}
	}

	seen := make(map[string]struct{})
	for _, field := range lit.Fields {
		if _, exists := seen[field.Name]; exists {
			c.diag.Add(field.NamePos, "field %q is already initialized", field.Name)
			continue
		}
		seen[field.Name] = struct{}{}

		fieldInfo, _, ok := info.Field(field.Name)
		if !ok {
			c.diag.Add(field.NamePos, "struct %q has no field %q", info.Name, field.Name)
			continue
		}
		value := c.checkExpression(field.Value)
		value = c.requireNonErrorableValue(field.Value, value, "errorable value cannot be used in a struct literal")
		if value.Base == TypeInvalid {
			continue
		}
		value = c.coerceValue(field.Value, value, fieldInfo.Type)
		if value.Base != fieldInfo.Type {
			c.diag.Add(field.Value.Pos(), "cannot assign %s to %s", value.Base, fieldInfo.Type)
		}
	}

	et := ExprType{Base: typ}
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) checkArrayLiteral(expr ast.Expression, lit *ast.ArrayLiteralExpr) ExprType {
	typ := c.resolveTypeRef(lit.Type)
	array, ok := ParseArrayType(typ)
	if !ok {
		c.diag.Add(lit.Type.Pos, "array literal requires an array type")
		return ExprType{Base: TypeInvalid}
	}
	if len(lit.Elements) > array.Len {
		c.diag.Add(lit.Pos(), "array literal has too many elements for %s", typ)
	}
	for _, element := range lit.Elements {
		value := c.checkExpression(element)
		value = c.requireNonErrorableValue(element, value, "errorable value cannot be used in an array literal")
		if value.Base == TypeInvalid {
			continue
		}
		value = c.coerceValue(element, value, array.Elem)
		if value.Base != array.Elem {
			c.diag.Add(element.Pos(), "cannot assign %s to %s", value.Base, array.Elem)
		}
	}

	et := ExprType{Base: typ}
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) checkSliceLiteral(expr ast.Expression, lit *ast.SliceLiteralExpr) ExprType {
	typ := c.resolveTypeRef(lit.Type)
	sliceType, ok := ParseSliceType(typ)
	if !ok {
		c.diag.Add(lit.Type.Pos, "slice literal requires a slice type")
		return ExprType{Base: TypeInvalid}
	}
	for _, element := range lit.Elements {
		value := c.checkExpression(element)
		value = c.requireNonErrorableValue(element, value, "errorable value cannot be used in a slice literal")
		if value.Base == TypeInvalid {
			continue
		}
		value = c.coerceValue(element, value, sliceType.Elem)
		if value.Base != sliceType.Elem {
			c.diag.Add(element.Pos(), "cannot assign %s to %s", value.Base, sliceType.Elem)
		}
	}

	et := ExprType{Base: typ}
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) checkMapLiteral(expr ast.Expression, lit *ast.MapLiteralExpr) ExprType {
	typ := c.resolveTypeRef(lit.Type)
	mapType, ok := ParseMapType(typ)
	if !ok {
		c.diag.Add(lit.Type.Pos, "map literal requires a map type")
		return ExprType{Base: TypeInvalid}
	}
	for _, pair := range lit.Pairs {
		keyType := c.checkExpression(pair.Key)
		keyType = c.requireNonErrorableValue(pair.Key, keyType, "errorable value cannot be used as a map key")
		if keyType.Base == TypeInvalid {
			continue
		}
		keyType = c.coerceValue(pair.Key, keyType, mapType.Key)
		if keyType.Base != mapType.Key {
			c.diag.Add(pair.Key.Pos(), "cannot use %s as map key type %s", keyType.Base, mapType.Key)
		}
		valueType := c.checkExpression(pair.Value)
		valueType = c.requireNonErrorableValue(pair.Value, valueType, "errorable value cannot be used as a map value")
		if valueType.Base == TypeInvalid {
			continue
		}
		valueType = c.coerceValue(pair.Value, valueType, mapType.Value)
		if valueType.Base != mapType.Value {
			c.diag.Add(pair.Value.Pos(), "cannot assign %s to %s", valueType.Base, mapType.Value)
		}
	}
	et := ExprType{Base: typ}
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) checkCall(expr ast.Expression, call *ast.CallExpr) ExprType {
	name, namePos, ok := callName(call.Callee)
	if ok && name == "len" {
		if len(call.Args) != 1 {
			c.diag.Add(namePos, "function %q expects 1 arguments, got %d", name, len(call.Args))
			return ExprType{Base: TypeInvalid}
		}
		argType := c.checkExpression(call.Args[0])
		if argType.Errorable {
			c.diag.Add(call.Args[0].Pos(), "errorable value cannot be passed as an argument")
			return ExprType{Base: TypeInvalid}
		}
		if !isSequenceType(argType.Base) && !isMapType(argType.Base) && argType.Base != TypeStr {
			c.diag.Add(call.Args[0].Pos(), "len requires an array, slice, map, or str argument")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeI32}
		c.info.Calls[call] = Signature{Name: name, FullName: name, Params: []Type{argType.Base}, Return: TypeI32, Builtin: true}
		c.info.ExprTypes[expr] = et
		return et
	}
	if ok && name == "to_str" {
		if len(call.Args) != 1 {
			c.diag.Add(namePos, "function %q expects 1 arguments, got %d", name, len(call.Args))
			return ExprType{Base: TypeInvalid}
		}
		argType := c.checkExpression(call.Args[0])
		if argType.Errorable {
			c.diag.Add(call.Args[0].Pos(), "errorable value cannot be passed as an argument")
			return ExprType{Base: TypeInvalid}
		}
		if argType.Base == TypeUntypedInt {
			argType = c.coerceValue(call.Args[0], argType, TypeI32)
		}
		switch argType.Base {
		case TypeI32, TypeI64, TypeBool, TypeStr, TypeError:
			// ok
		default:
			c.diag.Add(call.Args[0].Pos(), "to_str requires an i32, i64, bool, str, or error argument")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeStr}
		c.info.Calls[call] = Signature{Name: name, FullName: name, Params: []Type{argType.Base}, Return: TypeStr, Builtin: true}
		c.info.ExprTypes[expr] = et
		return et
	}
	if ok && name == "has" {
		if len(call.Args) != 2 {
			c.diag.Add(namePos, "function %q expects 2 arguments, got %d", name, len(call.Args))
			return ExprType{Base: TypeInvalid}
		}
		mapArg := c.checkExpression(call.Args[0])
		if mapArg.Errorable {
			c.diag.Add(call.Args[0].Pos(), "errorable value cannot be passed as an argument")
			return ExprType{Base: TypeInvalid}
		}
		mapType, ok := ParseMapType(mapArg.Base)
		if !ok {
			c.diag.Add(call.Args[0].Pos(), "has requires a map as its first argument")
			return ExprType{Base: TypeInvalid}
		}
		keyArg := c.checkExpression(call.Args[1])
		if keyArg.Errorable {
			c.diag.Add(call.Args[1].Pos(), "errorable value cannot be passed as an argument")
			return ExprType{Base: TypeInvalid}
		}
		keyArg = c.coerceValue(call.Args[1], keyArg, mapType.Key)
		if keyArg.Base != mapType.Key {
			c.diag.Add(call.Args[1].Pos(), "argument 2 to %q must be %s, got %s", name, mapType.Key, keyArg.Base)
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeBool}
		c.info.Calls[call] = Signature{Name: name, FullName: name, Params: []Type{mapArg.Base, mapType.Key}, Return: TypeBool, Builtin: true}
		c.info.ExprTypes[expr] = et
		return et
	}
	if ok && name == "delete" {
		if len(call.Args) != 2 {
			c.diag.Add(namePos, "function %q expects 2 arguments, got %d", name, len(call.Args))
			return ExprType{Base: TypeInvalid}
		}
		mapArg := c.checkExpression(call.Args[0])
		if mapArg.Errorable {
			c.diag.Add(call.Args[0].Pos(), "errorable value cannot be passed as an argument")
			return ExprType{Base: TypeInvalid}
		}
		mapType, ok := ParseMapType(mapArg.Base)
		if !ok {
			c.diag.Add(call.Args[0].Pos(), "delete requires a map as its first argument")
			return ExprType{Base: TypeInvalid}
		}
		keyArg := c.checkExpression(call.Args[1])
		if keyArg.Errorable {
			c.diag.Add(call.Args[1].Pos(), "errorable value cannot be passed as an argument")
			return ExprType{Base: TypeInvalid}
		}
		keyArg = c.coerceValue(call.Args[1], keyArg, mapType.Key)
		if keyArg.Base != mapType.Key {
			c.diag.Add(call.Args[1].Pos(), "argument 2 to %q must be %s, got %s", name, mapType.Key, keyArg.Base)
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeVoid}
		c.info.Calls[call] = Signature{Name: name, FullName: name, Params: []Type{mapArg.Base, mapType.Key}, Return: TypeVoid, Builtin: true}
		c.info.ExprTypes[expr] = et
		return et
	}
	if ok && name == "append" {
		if len(call.Args) != 2 {
			c.diag.Add(namePos, "function %q expects 2 arguments, got %d", name, len(call.Args))
			return ExprType{Base: TypeInvalid}
		}
		sliceArg := c.checkExpression(call.Args[0])
		if sliceArg.Errorable {
			c.diag.Add(call.Args[0].Pos(), "errorable value cannot be passed as an argument")
			return ExprType{Base: TypeInvalid}
		}
		sliceType, ok := ParseSliceType(sliceArg.Base)
		if !ok {
			c.diag.Add(call.Args[0].Pos(), "append requires a slice as its first argument")
			return ExprType{Base: TypeInvalid}
		}
		valueArg := c.checkExpression(call.Args[1])
		if valueArg.Errorable {
			c.diag.Add(call.Args[1].Pos(), "errorable value cannot be passed as an argument")
			return ExprType{Base: TypeInvalid}
		}
		if valueArg.Base == TypeNoReturn || valueArg.Base == TypeVoid {
			c.diag.Add(call.Args[1].Pos(), "argument 2 to %q requires a value", name)
			return ExprType{Base: TypeInvalid}
		}
		valueArg = c.coerceValue(call.Args[1], valueArg, sliceType.Elem)
		if valueArg.Base != sliceType.Elem {
			c.diag.Add(call.Args[1].Pos(), "argument 2 to %q must be %s, got %s", name, sliceType.Elem, valueArg.Base)
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: sliceArg.Base}
		c.info.Calls[call] = Signature{Name: name, FullName: name, Params: []Type{sliceArg.Base, sliceType.Elem}, Return: sliceArg.Base, Builtin: true}
		c.info.ExprTypes[expr] = et
		return et
	}
	if ok && name == "keys" {
		if len(call.Args) != 1 {
			c.diag.Add(namePos, "function %q expects 1 arguments, got %d", name, len(call.Args))
			return ExprType{Base: TypeInvalid}
		}
		mapArg := c.checkExpression(call.Args[0])
		if mapArg.Errorable {
			c.diag.Add(call.Args[0].Pos(), "errorable value cannot be passed as an argument")
			return ExprType{Base: TypeInvalid}
		}
		mapType, ok := ParseMapType(mapArg.Base)
		if !ok {
			c.diag.Add(call.Args[0].Pos(), "keys requires a map as its first argument")
			return ExprType{Base: TypeInvalid}
		}
		resultType := MakeSliceType(mapType.Key)
		et := ExprType{Base: resultType}
		c.info.Calls[call] = Signature{Name: name, FullName: name, Params: []Type{mapArg.Base}, Return: resultType, Builtin: true}
		c.info.ExprTypes[expr] = et
		return et
	}

	argOffset := 0
	var sig Signature

	switch callee := call.Callee.(type) {
	case *ast.IdentExpr:
		if binding, ok, _ := c.lookupLocalBinding(callee.Name); ok {
			functionType, isFunc := ParseFunctionType(binding.typ)
			if !isFunc {
				c.diag.Add(callee.NamePos, "call target must be callable")
				return ExprType{Base: TypeInvalid}
			}
			sig = Signature{
				Name:      "function value",
				FullName:  "function value",
				Params:    append([]Type(nil), functionType.Params...),
				Return:    functionType.Return,
				Errorable: functionType.Errorable,
				ValueCall: true,
			}
			name = sig.Name
			namePos = callee.NamePos
			break
		}
		sig, ok = c.functions[callee.Name]
		if !ok {
			c.diag.Add(callee.NamePos, "unknown function %q", callee.Name)
			return ExprType{Base: TypeInvalid}
		}
		name = callee.Name
		namePos = callee.NamePos
	case *ast.SelectorExpr:
		if ident, isIdent := callee.Inner.(*ast.IdentExpr); isIdent {
			if _, isLocal, _ := c.lookupLocalBinding(ident.Name); !isLocal {
				if enumInfo, isEnum := c.info.Enums[ident.Name]; isEnum {
					enumCase, _, caseOK := enumInfo.Case(callee.Name)
					if caseOK && len(enumCase.Fields) == 1 && len(call.Args) == 1 {
						field := enumCase.Fields[0]
						argType := c.checkExpression(call.Args[0])
						argType = c.requireNonErrorableValue(call.Args[0], argType, "errorable value cannot be used in an enum constructor")
						if argType.Base != TypeInvalid {
							argType = c.coerceValue(call.Args[0], argType, field.Type)
							if argType.Base != field.Type {
								c.diag.Add(call.Args[0].Pos(), "cannot assign %s to %s", argType.Base, field.Type)
							}
						}
						et := ExprType{Base: Type(enumInfo.Name)}
						c.info.ExprTypes[expr] = et
						c.info.EnumConstructors[call] = EnumConstructorInfo{
							EnumName: enumInfo.Name,
							CaseName: callee.Name,
						}
						return et
					}
				}
			}
		}
		receiver := c.checkExpression(callee.Inner)
		if receiver.Errorable {
			c.diag.Add(callee.DotPos, "method call cannot use an errorable receiver")
			return ExprType{Base: TypeInvalid}
		}
		if iface, ok := c.info.Interfaces[string(receiver.Base)]; ok {
			method, _, ok := iface.Method(callee.Name)
			if !ok {
				c.diag.Add(callee.NamePos, "interface %q has no method %q", iface.Name, callee.Name)
				return ExprType{Base: TypeInvalid}
			}
			sig = Signature{
				Name:      callee.Name,
				FullName:  string(receiver.Base) + "." + callee.Name,
				Method:    true,
				Receiver:  receiver.Base,
				Params:    append([]Type{receiver.Base}, method.Params...),
				Return:    method.Return,
				Errorable: method.Errorable,
			}
			name = callee.Name
			namePos = callee.NamePos
			argOffset = 1
			break
		}
		if functionType, ok := ParseFunctionType(receiver.Base); ok {
			sig = Signature{
				Name:      "function value",
				FullName:  "function value",
				Params:    append([]Type(nil), functionType.Params...),
				Return:    functionType.Return,
				Errorable: functionType.Errorable,
				ValueCall: true,
			}
			name = sig.Name
			namePos = callee.NamePos
			break
		}
		sig, ok = c.lookupMethod(receiver.Base, callee.Name)
		if !ok {
			calleeType := c.checkExpression(call.Callee)
			if functionType, ok := ParseFunctionType(calleeType.Base); ok && !calleeType.Errorable {
				sig = Signature{
					Name:      "function value",
					FullName:  "function value",
					Params:    append([]Type(nil), functionType.Params...),
					Return:    functionType.Return,
					Errorable: functionType.Errorable,
					ValueCall: true,
				}
				name = sig.Name
				namePos = callee.NamePos
				break
			}
			c.diag.Add(callee.NamePos, "type %q has no method %q", receiver.Base, callee.Name)
			return ExprType{Base: TypeInvalid}
		}
		if sig.Package != "" && sig.Package != c.current.signature.Package && !sig.Exported {
			c.diag.Add(callee.NamePos, "package %q does not export method %q", packageDisplayName(sig.Package), callee.Name)
			return ExprType{Base: TypeInvalid}
		}
		name = callee.Name
		namePos = callee.NamePos
		argOffset = 1
	default:
		calleeType := c.checkExpression(call.Callee)
		if calleeType.Errorable {
			c.diag.Add(call.Callee.Pos(), "call target cannot be errorable")
			return ExprType{Base: TypeInvalid}
		}
		functionType, ok := ParseFunctionType(calleeType.Base)
		if !ok {
			c.diag.Add(call.Pos(), "call target must be callable")
			return ExprType{Base: TypeInvalid}
		}
		sig = Signature{
			Name:      "function value",
			FullName:  "function value",
			Params:    append([]Type(nil), functionType.Params...),
			Return:    functionType.Return,
			Errorable: functionType.Errorable,
			ValueCall: true,
		}
		name = sig.Name
		namePos = call.Callee.Pos()
	}

	wantArgs := len(sig.Params) - argOffset
	if len(call.Args) != wantArgs {
		c.diag.Add(namePos, "function %q expects %d arguments, got %d", name, wantArgs, len(call.Args))
	}
	for i, arg := range call.Args {
		argType := c.checkExpression(arg)
		if argType.Errorable {
			c.diag.Add(arg.Pos(), "errorable value cannot be passed as an argument")
			continue
		}
		if argType.Base == TypeNoReturn || argType.Base == TypeVoid {
			c.diag.Add(arg.Pos(), "argument %d to %q requires a value", i+1, name)
			continue
		}
		paramIndex := i + argOffset
		if paramIndex >= len(sig.Params) {
			continue
		}
		argType = c.coerceValue(arg, argType, sig.Params[paramIndex])
		if argType.Base != sig.Params[paramIndex] {
			c.diag.Add(arg.Pos(), "argument %d to %q must be %s, got %s", i+1, name, sig.Params[paramIndex], argType.Base)
		}
	}
	et := ExprType{Base: sig.Return, Errorable: sig.Errorable}
	if sig.HostIntrinsic {
		c.registerHostErrorNames(sig.FullName)
	}
	c.info.Calls[call] = sig
	c.info.ExprTypes[expr] = et
	return et
}

func (c *Checker) registerHostErrorNames(fullName string) {
	switch fullName {
	case "fs.read_file", "fs.write_file", "fs.read_dir", "fs.stat", "fs.mkdir_all", "fs.remove_all", "fs.temp_dir":
		for _, name := range []string{
			"AlreadyExists",
			"IO",
			"InvalidPath",
			"NotFound",
			"PermissionDenied",
		} {
			c.info.ErrorCodes[name] = 0
		}
	case "process.run", "process.run_inherit", "env.lookup":
		for _, name := range []string{
			"IO",
			"InvalidArgument",
			"NotFound",
			"PermissionDenied",
		} {
			c.info.ErrorCodes[name] = 0
		}
	}
}

func callName(expr ast.Expression) (string, token.Position, bool) {
	ident, ok := expr.(*ast.IdentExpr)
	if !ok {
		return "", token.Position{}, false
	}
	return ident.Name, ident.NamePos, true
}

func (c *Checker) checkBinary(expr ast.Expression, binary *ast.BinaryExpr) ExprType {
	left := c.checkExpression(binary.Left)
	right := c.checkExpression(binary.Right)
	if left.Errorable || right.Errorable {
		c.diag.Add(binary.OpPos, "binary operators cannot use errorable operands")
		return ExprType{Base: TypeInvalid}
	}

	switch binary.Operator {
	case token.AmpAmp, token.PipePipe:
		if left.Base != TypeBool || right.Base != TypeBool {
			c.diag.Add(binary.OpPos, "logical operators require bool operands")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeBool}
		c.info.ExprTypes[expr] = et
		return et
	case token.Plus:
		if left.Base == TypeStr && right.Base == TypeStr {
			et := ExprType{Base: TypeStr}
			c.info.ExprTypes[expr] = et
			return et
		}
		coerced, ok := c.coerceBinaryIntegers(binary.Left, left, binary.Right, right)
		if !ok {
			c.diag.Add(binary.OpPos, "'+' requires matching integer or str operands")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: coerced.Result}
		c.info.ExprTypes[expr] = et
		return et
	case token.Minus, token.Star, token.Slash, token.Percent:
		coerced, ok := c.coerceBinaryIntegers(binary.Left, left, binary.Right, right)
		if !ok {
			c.diag.Add(binary.OpPos, "arithmetic operators require matching integer operands")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: coerced.Result}
		c.info.ExprTypes[expr] = et
		return et
	case token.Less, token.LessEqual, token.Greater, token.GreaterEqual:
		coerced, ok := c.coerceBinaryIntegers(binary.Left, left, binary.Right, right)
		if !ok {
			c.diag.Add(binary.OpPos, "relational operators require matching integer operands")
			return ExprType{Base: TypeInvalid}
		}
		if coerced.Result == TypeUntypedInt {
			c.resolveUntypedBinaryOperands(binary, left, right)
		}
		et := ExprType{Base: TypeBool}
		c.info.ExprTypes[expr] = et
		return et
	case token.EqualEqual, token.BangEqual:
		if isIntegerType(left.Base) || isIntegerType(right.Base) {
			coerced, ok := c.coerceBinaryIntegers(binary.Left, left, binary.Right, right)
			if !ok {
				c.diag.Add(binary.OpPos, "comparison operands must have the same type")
				return ExprType{Base: TypeInvalid}
			}
			if coerced.Result == TypeUntypedInt {
				c.resolveUntypedBinaryOperands(binary, left, right)
			}
			et := ExprType{Base: TypeBool}
			c.info.ExprTypes[expr] = et
			return et
		}
		if left.Base == TypeNil && isPointerType(right.Base) {
			left = c.coerceValue(binary.Left, left, right.Base)
		}
		if right.Base == TypeNil && isPointerType(left.Base) {
			right = c.coerceValue(binary.Right, right, left.Base)
		}
		if isPointerType(left.Base) || isPointerType(right.Base) {
			if left.Base != right.Base || !isPointerType(left.Base) {
				c.diag.Add(binary.OpPos, "comparison operands must have the same type")
				return ExprType{Base: TypeInvalid}
			}
			et := ExprType{Base: TypeBool}
			c.info.ExprTypes[expr] = et
			return et
		}
		if left.Base != right.Base {
			c.diag.Add(binary.OpPos, "comparison operands must have the same type")
			return ExprType{Base: TypeInvalid}
		}
		if _, ok := c.info.Enums[string(left.Base)]; ok {
			c.diag.Add(binary.OpPos, "comparison is not supported for enum values in v0.4")
			return ExprType{Base: TypeInvalid}
		}
		if left.Base == TypeStr {
			et := ExprType{Base: TypeBool}
			c.info.ExprTypes[expr] = et
			return et
		}
		if left.Base == TypeError {
			et := ExprType{Base: TypeBool}
			c.info.ExprTypes[expr] = et
			return et
		}
		if left.Base != TypeBool {
			c.diag.Add(binary.OpPos, "comparison is only supported for bool, integers, pointers, str, and error")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeBool}
		c.info.ExprTypes[expr] = et
		return et
	default:
		c.diag.Add(binary.OpPos, "unsupported operator")
		return ExprType{Base: TypeInvalid}
	}
}

func (c *Checker) resolveTypeRef(ref ast.TypeRef) Type {
	switch ref.Kind {
	case ast.PointerTypeRef:
		if ref.Elem == nil {
			c.diag.Add(ref.Pos, "pointer type is missing an element type")
			return TypeInvalid
		}
		elemType := c.resolveTypeRef(*ref.Elem)
		if elemType == TypeVoid || elemType == TypeNoReturn || elemType == TypeInvalid {
			c.diag.Add(ref.Pos, "pointer target type %q is not allowed", ref.Elem.String())
			return TypeInvalid
		}
		return MakePointerType(elemType)
	case ast.SliceTypeRef:
		if ref.Elem == nil {
			c.diag.Add(ref.Pos, "slice type is missing an element type")
			return TypeInvalid
		}
		elemType := c.resolveTypeRef(*ref.Elem)
		if elemType == TypeVoid || elemType == TypeNoReturn || elemType == TypeInvalid {
			c.diag.Add(ref.Pos, "slice element type %q is not allowed", ref.Elem.String())
			return TypeInvalid
		}
		return MakeSliceType(elemType)
	case ast.MapTypeRef:
		if ref.Key == nil || ref.Value == nil {
			c.diag.Add(ref.Pos, "map type is incomplete")
			return TypeInvalid
		}
		keyType := c.resolveTypeRef(*ref.Key)
		if !isMapKeyType(keyType) {
			c.diag.Add(ref.Pos, "map key type %q is not supported; must be bool, i32, i64, or str", ref.Key.String())
			return TypeInvalid
		}
		valueType := c.resolveTypeRef(*ref.Value)
		if valueType == TypeVoid || valueType == TypeNoReturn || valueType == TypeInvalid {
			c.diag.Add(ref.Pos, "map value type %q is not allowed", ref.Value.String())
			return TypeInvalid
		}
		return MakeMapType(keyType, valueType)
	case ast.ArrayTypeRef:
		if ref.ArrayLen < 0 || ref.ArrayLen > 2147483647 {
			c.diag.Add(ref.Pos, "array length %d is out of range", ref.ArrayLen)
			return TypeInvalid
		}
		if ref.Elem == nil {
			c.diag.Add(ref.Pos, "array type is missing an element type")
			return TypeInvalid
		}
		elemType := c.resolveTypeRef(*ref.Elem)
		if elemType == TypeVoid || elemType == TypeNoReturn || elemType == TypeInvalid {
			c.diag.Add(ref.Pos, "array element type %q is not allowed", ref.Elem.String())
			return TypeInvalid
		}
		return MakeArrayType(ref.ArrayLen, elemType)
	case ast.FunctionTypeRef:
		params := make([]Type, 0, len(ref.Params))
		for _, param := range ref.Params {
			paramType := c.resolveTypeRef(param)
			if paramType == TypeVoid || paramType == TypeNoReturn || paramType == TypeInvalid {
				c.diag.Add(param.Pos, "function parameter type %q is not allowed", param.String())
				return TypeInvalid
			}
			params = append(params, paramType)
		}
		if ref.Return == nil {
			c.diag.Add(ref.Pos, "function type is missing a return type")
			return TypeInvalid
		}
		retType := c.resolveTypeRef(*ref.Return)
		if retType == TypeNoReturn && ref.Errorable {
			c.diag.Add(ref.Pos, "noreturn functions cannot also be errorable")
			return TypeInvalid
		}
		if retType == TypeError && ref.Errorable {
			c.diag.Add(ref.Pos, "error functions cannot also be errorable")
			return TypeInvalid
		}
		return MakeFunctionType(params, retType, ref.Errorable)
	}

	if len(ref.TypeArgs) > 0 {
		c.diag.Add(ref.Pos, "generic type %q must be monomorphized before checking", ref.String())
		return TypeInvalid
	}

	switch Type(ref.Name) {
	case TypeVoid, TypeNoReturn, TypeBool, TypeI32, TypeI64, TypeStr, TypeError:
		return Type(ref.Name)
	}
	if _, ok := c.structs[ref.Name]; ok {
		return Type(ref.Name)
	}
	if _, ok := c.interfaces[ref.Name]; ok {
		return Type(ref.Name)
	}
	if _, ok := c.enums[ref.Name]; ok {
		return Type(ref.Name)
	}

	c.diag.Add(ref.Pos, "unknown type %q", ref.String())
	return TypeInvalid
}

func (c *Checker) resolveMethodReceiverType(ref ast.TypeRef) Type {
	typ := c.resolveTypeRef(ref)
	if typ == TypeInvalid {
		return TypeInvalid
	}
	base, _ := methodReceiverBaseType(typ)
	if _, ok := c.info.Structs[string(base)]; ok {
		return typ
	}
	c.diag.Add(ref.Pos, "method receiver must be a named struct type or pointer to one")
	return TypeInvalid
}

func packageForFunction(fullName string) string {
	if fullName == "main" {
		return "main"
	}
	idx := lastTopLevelDot(fullName)
	if idx <= 0 {
		return ""
	}
	return fullName[:idx]
}

func methodFullName(receiver Type, name string) string {
	base, ok := methodReceiverBaseType(receiver)
	if !ok {
		return name
	}
	return string(base) + "." + name
}

func packageForMethodReceiver(receiver Type) string {
	base, ok := methodReceiverBaseType(receiver)
	if !ok {
		return ""
	}
	return packageForType(base)
}

func packageForType(typ Type) string {
	text := string(typ)
	idx := lastTopLevelDot(text)
	if idx <= 0 {
		return ""
	}
	return text[:idx]
}

func lastTopLevelDot(text string) int {
	depth := 0
	for i := len(text) - 1; i >= 0; i-- {
		switch text[i] {
		case ']':
			depth++
		case '[':
			if depth > 0 {
				depth--
			}
		case '.':
			if depth == 0 {
				return i
			}
		}
	}
	return -1
}

func packageDisplayName(pkg string) string {
	if pkg == "" {
		return ""
	}
	idx := strings.LastIndex(pkg, ".")
	if idx < 0 {
		return pkg
	}
	return pkg[idx+1:]
}

func (c *Checker) pushScope() {
	c.current.scopes = append(c.current.scopes, map[string]localBinding{})
}

func (c *Checker) popScope() {
	c.current.scopes = c.current.scopes[:len(c.current.scopes)-1]
}

func (c *Checker) bindLocal(name string, typ Type, node ast.Node) {
	c.current.scopes[len(c.current.scopes)-1][name] = localBinding{
		typ:   typ,
		owner: c.current,
		node:  node,
	}
}

func (c *Checker) scopeOwns(name string) bool {
	_, ok := c.current.scopes[len(c.current.scopes)-1][name]
	return ok
}

func (c *Checker) lookupLocal(name string) (Type, bool) {
	binding, ok, _ := c.lookupLocalBinding(name)
	if !ok {
		return TypeInvalid, false
	}
	return binding.typ, true
}

func (c *Checker) lookupLocalBinding(name string) (localBinding, bool, bool) {
	if c.current == nil {
		return localBinding{}, false, false
	}
	return c.lookupLocalBindingFromContext(c.current, name)
}

func (c *Checker) lookupLocalBindingFromContext(ctx *functionContext, name string) (localBinding, bool, bool) {
	for i := len(ctx.scopes) - 1; i >= 0; i-- {
		if binding, ok := ctx.scopes[i][name]; ok {
			return binding, true, ctx != c.current
		}
	}
	if ctx.parent == nil {
		return localBinding{}, false, false
	}
	binding, ok, _ := c.lookupLocalBindingFromContext(ctx.parent, name)
	if !ok {
		return localBinding{}, false, false
	}
	return binding, true, true
}

func (c *Checker) lookupMethod(receiver Type, name string) (Signature, bool) {
	methods, ok := c.methods[receiver]
	if !ok {
		return Signature{}, false
	}
	sig, ok := methods[name]
	return sig, ok
}

func (c *Checker) lookupMethodForBase(base Type, name string) (Signature, bool) {
	for receiver, methods := range c.methods {
		receiverBase, ok := methodReceiverBaseType(receiver)
		if !ok || receiverBase != base {
			continue
		}
		sig, ok := methods[name]
		if ok {
			return sig, true
		}
	}
	return Signature{}, false
}

func (c *Checker) recordCapture(name string, typ Type) {
	if c.current == nil || c.current.literal == nil {
		return
	}
	info := c.info.FunctionLiterals[c.current.literal]
	for _, capture := range info.Captures {
		if capture.Name == name {
			return
		}
	}
	info.Captures = append(info.Captures, CaptureInfo{Name: name, Type: typ})
	c.info.FunctionLiterals[c.current.literal] = info
}

func (c *Checker) useErrorName(name string) {
	if _, ok := c.info.ErrorCodes[name]; ok {
		return
	}
	c.info.ErrorCodes[name] = 0
}

func (c *Checker) blockDefinitelyReturns(block *ast.BlockStmt) bool {
	for _, stmt := range block.Stmts {
		if c.stmtDefinitelyReturns(stmt) {
			return true
		}
	}
	return false
}

func (c *Checker) stmtDefinitelyReturns(stmt ast.Statement) bool {
	switch s := stmt.(type) {
	case *ast.ReturnStmt:
		return true
	case *ast.BlockStmt:
		return c.blockDefinitelyReturns(s)
	case *ast.ExprStmt:
		exprType, ok := c.info.ExprTypes[s.Expr]
		return ok && exprType.Base == TypeNoReturn
	case *ast.IfStmt:
		if s.Else == nil {
			return false
		}
		return c.blockDefinitelyReturns(s.Then) && c.stmtDefinitelyReturns(s.Else)
	case *ast.MatchStmt:
		if len(s.Arms) == 0 && s.ElseBody == nil {
			return false
		}
		for _, arm := range s.Arms {
			if !c.blockDefinitelyReturns(arm.Body) {
				return false
			}
		}
		if s.ElseBody != nil {
			return c.blockDefinitelyReturns(s.ElseBody)
		}
		return true
	default:
		return false
	}
}

func (c *Checker) blockTerminatesControlFlow(block *ast.BlockStmt) bool {
	for _, stmt := range block.Stmts {
		if c.stmtTerminatesControlFlow(stmt) {
			return true
		}
	}
	return false
}

func (c *Checker) stmtTerminatesControlFlow(stmt ast.Statement) bool {
	switch s := stmt.(type) {
	case *ast.ReturnStmt, *ast.BreakStmt, *ast.ContinueStmt:
		return true
	case *ast.BlockStmt:
		return c.blockTerminatesControlFlow(s)
	case *ast.ExprStmt:
		exprType, ok := c.info.ExprTypes[s.Expr]
		return ok && exprType.Base == TypeNoReturn
	case *ast.IfStmt:
		if s.Else == nil {
			return false
		}
		return c.blockTerminatesControlFlow(s.Then) && c.stmtTerminatesControlFlow(s.Else)
	case *ast.MatchStmt:
		if len(s.Arms) == 0 && s.ElseBody == nil {
			return false
		}
		for _, arm := range s.Arms {
			if !c.blockTerminatesControlFlow(arm.Body) {
				return false
			}
		}
		if s.ElseBody != nil {
			return c.blockTerminatesControlFlow(s.ElseBody)
		}
		return true
	default:
		return false
	}
}

func (c *Checker) checkPropagate(expr ast.Expression, propagate *ast.PropagateExpr) ExprType {
	inner := c.checkExpression(propagate.Inner)
	if inner.Errorable {
		if !c.currentCanPropagateError() {
			c.diag.Add(propagate.QuestionPos, "cannot use ? in a function that cannot return an error")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: inner.Base}
		c.info.ExprTypes[expr] = et
		return et
	}
	if inner.Base == TypeError {
		if !c.currentCanPropagateError() {
			c.diag.Add(propagate.QuestionPos, "cannot use ? in a function that cannot return an error")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: TypeVoid}
		c.info.ExprTypes[expr] = et
		return et
	}
	c.diag.Add(propagate.QuestionPos, "? requires an errorable expression or error value")
	return ExprType{Base: TypeInvalid}
}

func (c *Checker) checkHandle(expr ast.Expression, handle *ast.HandleExpr) ExprType {
	inner := c.checkExpression(handle.Inner)
	switch {
	case inner.Errorable && inner.Base != TypeVoid:
		c.checkBlockWithErrorBinding(handle.Handler, handle.ErrName)
		if !c.blockTerminatesControlFlow(handle.Handler) {
			c.diag.Add(handle.OrPos, "or handler for a value result must terminate control flow")
			return ExprType{Base: TypeInvalid}
		}
		et := ExprType{Base: inner.Base}
		c.info.ExprTypes[expr] = et
		return et
	case inner.Errorable:
		c.checkBlockWithErrorBinding(handle.Handler, handle.ErrName)
		et := ExprType{Base: TypeVoid}
		c.info.ExprTypes[expr] = et
		return et
	case inner.Base == TypeError:
		c.checkBlockWithErrorBinding(handle.Handler, handle.ErrName)
		et := ExprType{Base: TypeVoid}
		c.info.ExprTypes[expr] = et
		return et
	default:
		c.diag.Add(handle.OrPos, "or requires an errorable expression or error value")
		return ExprType{Base: TypeInvalid}
	}
}

func (c *Checker) checkFunctionLiteral(expr ast.Expression, lit *ast.FunctionLiteralExpr) ExprType {
	params := make([]Type, 0, len(lit.Params))
	for _, param := range lit.Params {
		paramType := c.resolveTypeRef(param.Type)
		if paramType == TypeVoid || paramType == TypeNoReturn || paramType == TypeInvalid {
			c.diag.Add(param.Type.Pos, "parameter %q cannot use type %q", param.Name, param.Type.String())
			return ExprType{Base: TypeInvalid}
		}
		params = append(params, paramType)
	}

	retType := c.resolveTypeRef(lit.Return)
	if retType == TypeNoReturn && lit.ReturnIsBang {
		c.diag.Add(lit.Return.Pos, "noreturn functions cannot also be errorable")
		return ExprType{Base: TypeInvalid}
	}
	if retType == TypeError && lit.ReturnIsBang {
		c.diag.Add(lit.Return.Pos, "error functions cannot also be errorable")
		return ExprType{Base: TypeInvalid}
	}

	sig := Signature{
		Name:      "function value",
		Return:    retType,
		Errorable: lit.ReturnIsBang,
		Params:    params,
		ValueCall: true,
	}
	ctx := &functionContext{
		signature: sig,
		parent:    c.current,
		literal:   lit,
		scopes:    []map[string]localBinding{{}},
	}

	prev := c.current
	c.current = ctx
	for i, param := range lit.Params {
		if _, exists := ctx.scopes[0][param.Name]; exists {
			c.diag.Add(param.NamePos, "duplicate parameter %q", param.Name)
			continue
		}
		ctx.scopes[0][param.Name] = localBinding{typ: params[i], owner: ctx, node: lit}
	}

	c.checkBlock(lit.Body)
	c.current = prev

	if retType != TypeVoid && !c.blockDefinitelyReturns(lit.Body) {
		if retType == TypeNoReturn {
			c.diag.Add(lit.FnPos, "function literal must not fall through")
		} else {
			c.diag.Add(lit.FnPos, "function literal must return a value on all paths")
		}
	}

	info := c.info.FunctionLiterals[lit]
	info.Signature = sig
	c.info.FunctionLiterals[lit] = info

	et := ExprType{Base: MakeFunctionType(params, retType, lit.ReturnIsBang)}
	c.info.ExprTypes[expr] = et
	return et
}

func formatErrorName(name string) string {
	return fmt.Sprintf("error.%s", name)
}

func isPointerType(typ Type) bool {
	_, ok := ParsePointerType(typ)
	return ok
}

func (c *Checker) isInterfaceType(typ Type) bool {
	_, ok := c.info.Interfaces[string(typ)]
	return ok
}

func sequenceElementType(typ Type) (Type, bool) {
	if array, ok := ParseArrayType(typ); ok {
		return array.Elem, true
	}
	if slice, ok := ParseSliceType(typ); ok {
		return slice.Elem, true
	}
	return TypeInvalid, false
}

func isSequenceType(typ Type) bool {
	_, ok := sequenceElementType(typ)
	return ok
}

func isIntegerType(typ Type) bool {
	return typ == TypeI32 || typ == TypeI64 || typ == TypeUntypedInt
}

func (c *Checker) checkAddressableExpr(expr ast.Expression, allowCompositeLiteral bool) (Type, bool) {
	switch e := expr.(type) {
	case *ast.IdentExpr:
		binding, ok, captured := c.lookupLocalBinding(e.Name)
		if !ok {
			c.diag.Add(e.NamePos, "unknown local %q", e.Name)
			return TypeInvalid, false
		}
		if captured {
			c.recordCapture(e.Name, binding.typ)
			if !allowCompositeLiteral {
				c.diag.Add(e.NamePos, "cannot assign to captured outer local %q", e.Name)
				return TypeInvalid, false
			}
		}
		return binding.typ, true
	case *ast.GroupExpr:
		typ, ok := c.checkAddressableExpr(e.Inner, allowCompositeLiteral)
		if ok {
			c.info.ExprTypes[expr] = ExprType{Base: typ}
		}
		return typ, ok
	case *ast.SelectorExpr:
		base, ok := c.checkAddressableExpr(e.Inner, false)
		if !ok {
			return TypeInvalid, false
		}
		resolved := base
		if pointer, ok := ParsePointerType(base); ok {
			if _, isStruct := c.info.Structs[string(pointer.Elem)]; isStruct {
				resolved = pointer.Elem
				c.info.AutoDeref[e] = true
			}
		}
		info, ok := c.info.Structs[string(resolved)]
		if !ok {
			c.diag.Add(e.DotPos, "field access requires a struct value")
			return TypeInvalid, false
		}
		field, _, ok := info.Field(e.Name)
		if !ok {
			c.diag.Add(e.NamePos, "struct %q has no field %q", info.Name, e.Name)
			return TypeInvalid, false
		}
		return field.Type, true
	case *ast.IndexExpr:
		base, ok := c.checkAddressableExpr(e.Inner, false)
		if !ok {
			return TypeInvalid, false
		}
		elemType, ok := sequenceElementType(base)
		if !ok {
			c.diag.Add(e.LBracketPos, "indexing requires an array or slice value")
			return TypeInvalid, false
		}
		indexType := c.checkExpression(e.Index)
		if indexType.Errorable {
			c.diag.Add(e.Index.Pos(), "index expression cannot be errorable")
			return TypeInvalid, false
		}
		if !isIntegerType(indexType.Base) {
			c.diag.Add(e.Index.Pos(), "index expression must be an integer")
			return TypeInvalid, false
		}
		if indexType.Base == TypeUntypedInt {
			indexType = c.coerceUntypedInteger(e.Index, indexType, TypeI32)
			if indexType.Base == TypeUntypedInt {
				c.diag.Add(e.Index.Pos(), "index expression must fit in i32")
				return TypeInvalid, false
			}
		}
		return elemType, true
	case *ast.UnaryExpr:
		if e.Operator != token.Star {
			return TypeInvalid, false
		}
		inner := c.checkExpression(e.Inner)
		if inner.Errorable {
			c.diag.Add(e.OpPos, "dereference cannot use an errorable operand")
			return TypeInvalid, false
		}
		pointer, ok := ParsePointerType(inner.Base)
		if !ok {
			c.diag.Add(e.OpPos, "dereference requires a pointer operand")
			return TypeInvalid, false
		}
		return pointer.Elem, true
	case *ast.StructLiteralExpr:
		if !allowCompositeLiteral {
			return TypeInvalid, false
		}
		typ := c.checkStructLiteral(expr, e)
		return typ.Base, typ.Base != TypeInvalid
	case *ast.ArrayLiteralExpr:
		if !allowCompositeLiteral {
			return TypeInvalid, false
		}
		typ := c.checkArrayLiteral(expr, e)
		return typ.Base, typ.Base != TypeInvalid
	case *ast.SliceLiteralExpr:
		if !allowCompositeLiteral {
			return TypeInvalid, false
		}
		typ := c.checkSliceLiteral(expr, e)
		return typ.Base, typ.Base != TypeInvalid
	case *ast.MapLiteralExpr:
		if !allowCompositeLiteral {
			return TypeInvalid, false
		}
		typ := c.checkMapLiteral(expr, e)
		return typ.Base, typ.Base != TypeInvalid
	default:
		return TypeInvalid, false
	}
}

func (c *Checker) addressRootCapturedOuterLocal(expr ast.Expression) (string, bool) {
	switch e := expr.(type) {
	case *ast.IdentExpr:
		binding, ok, captured := c.lookupLocalBinding(e.Name)
		if ok && captured {
			c.recordCapture(e.Name, binding.typ)
			return e.Name, true
		}
		return "", false
	case *ast.GroupExpr:
		return c.addressRootCapturedOuterLocal(e.Inner)
	case *ast.SelectorExpr:
		return c.addressRootCapturedOuterLocal(e.Inner)
	case *ast.IndexExpr:
		return c.addressRootCapturedOuterLocal(e.Inner)
	default:
		return "", false
	}
}

func (c *Checker) checkSliceBound(expr ast.Expression) bool {
	if expr == nil {
		return true
	}
	boundType := c.checkExpression(expr)
	if boundType.Errorable {
		c.diag.Add(expr.Pos(), "slice bounds cannot be errorable")
		return false
	}
	if !isIntegerType(boundType.Base) {
		c.diag.Add(expr.Pos(), "slice bounds must be integers")
		return false
	}
	if boundType.Base == TypeUntypedInt {
		boundType = c.coerceUntypedInteger(expr, boundType, TypeI32)
		if boundType.Base == TypeUntypedInt {
			c.diag.Add(expr.Pos(), "slice bounds must fit in i32")
			return false
		}
	}
	return true
}

func (c *Checker) requireNonErrorableValue(expr ast.Expression, exprType ExprType, errorMessage string) ExprType {
	if exprType.Errorable {
		c.diag.Add(expr.Pos(), "%s", errorMessage)
		return ExprType{Base: TypeInvalid}
	}
	if exprType.Base == TypeInvalid || exprType.Base == TypeVoid || exprType.Base == TypeNoReturn {
		c.diag.Add(expr.Pos(), "declaration requires a value")
		return ExprType{Base: TypeInvalid}
	}
	return exprType
}

func (c *Checker) coerceUntypedInteger(expr ast.Expression, exprType ExprType, target Type) ExprType {
	if exprType.Base != TypeUntypedInt {
		return exprType
	}
	if target != TypeI32 && target != TypeI64 {
		return exprType
	}
	if !c.intLiteralFits(expr, target) {
		return exprType
	}
	c.setExprType(expr, ExprType{Base: target})
	return ExprType{Base: target}
}

func (c *Checker) coerceValue(expr ast.Expression, exprType ExprType, target Type) ExprType {
	if exprType.Base == TypeNil {
		if !isPointerType(target) {
			return exprType
		}
		coerced := ExprType{Base: target}
		c.setExprType(expr, coerced)
		return coerced
	}
	if c.isInterfaceType(target) {
		if exprType.Base == target {
			return ExprType{Base: target}
		}
		if _, ok := c.info.Interfaces[string(exprType.Base)]; ok {
			return exprType
		}
		if ok, message := c.satisfiesInterface(exprType.Base, target); ok {
			return ExprType{Base: target}
		} else if message != "" {
			c.diag.Add(expr.Pos(), "%s", message)
			return ExprType{Base: TypeInvalid}
		}
	}
	return c.coerceUntypedInteger(expr, exprType, target)
}

func (c *Checker) resolveUntypedBinaryOperands(binary *ast.BinaryExpr, left, right ExprType) {
	target := c.defaultUntypedIntegerType(binary)
	if target == TypeInvalid {
		target = TypeI32
	}
	if left.Base == TypeUntypedInt {
		c.coerceUntypedInteger(binary.Left, left, target)
	}
	if right.Base == TypeUntypedInt {
		c.coerceUntypedInteger(binary.Right, right, target)
	}
}

func (c *Checker) coerceBinaryIntegers(leftExpr ast.Expression, left ExprType, rightExpr ast.Expression, right ExprType) (coercedIntegers, bool) {
	out := coercedIntegers{
		Left:  left,
		Right: right,
	}
	if !isIntegerType(left.Base) || !isIntegerType(right.Base) {
		return out, false
	}
	switch {
	case left.Base == TypeUntypedInt && right.Base == TypeUntypedInt:
		out.Result = TypeUntypedInt
		return out, true
	case left.Base == TypeUntypedInt:
		if !c.intLiteralFits(leftExpr, right.Base) {
			return out, false
		}
		out.Left = c.coerceUntypedInteger(leftExpr, out.Left, right.Base)
		out.Result = right.Base
		return out, true
	case right.Base == TypeUntypedInt:
		if !c.intLiteralFits(rightExpr, left.Base) {
			return out, false
		}
		out.Right = c.coerceUntypedInteger(rightExpr, out.Right, left.Base)
		out.Result = left.Base
		return out, true
	case left.Base == right.Base:
		out.Result = left.Base
		return out, true
	default:
		return out, false
	}
}

func (c *Checker) setExprType(expr ast.Expression, exprType ExprType) {
	c.info.ExprTypes[expr] = exprType
	switch e := expr.(type) {
	case *ast.GroupExpr:
		c.setExprType(e.Inner, exprType)
	case *ast.BinaryExpr:
		if c.info.ExprTypes[e.Left].Base == TypeUntypedInt {
			c.setExprType(e.Left, exprType)
		}
		if c.info.ExprTypes[e.Right].Base == TypeUntypedInt {
			c.setExprType(e.Right, exprType)
		}
	case *ast.UnaryExpr:
		if e.Operator == token.Minus && c.info.ExprTypes[e.Inner].Base == TypeUntypedInt {
			c.setExprType(e.Inner, exprType)
		}
	}
}

func (c *Checker) intLiteralFits(expr ast.Expression, target Type) bool {
	literal, ok := unwrapIntLiteral(expr)
	if !ok {
		return false
	}
	switch target {
	case TypeI32:
		return literal.Value >= -2147483648 && literal.Value <= 2147483647
	case TypeI64:
		return true
	default:
		return false
	}
}

func unwrapIntLiteral(expr ast.Expression) (*ast.IntLiteral, bool) {
	switch e := expr.(type) {
	case *ast.IntLiteral:
		return e, true
	case *ast.CharLiteral:
		return &ast.IntLiteral{Value: int64(e.Value), LitPos: e.LitPos}, true
	case *ast.GroupExpr:
		return unwrapIntLiteral(e.Inner)
	case *ast.UnaryExpr:
		if e.Operator != token.Minus {
			return nil, false
		}
		inner, ok := unwrapIntLiteral(e.Inner)
		if !ok {
			return nil, false
		}
		return &ast.IntLiteral{Value: -inner.Value, LitPos: e.OpPos}, true
	case *ast.BinaryExpr:
		left, lok := unwrapIntLiteral(e.Left)
		right, rok := unwrapIntLiteral(e.Right)
		if !lok || !rok {
			return nil, false
		}
		var result int64
		switch e.Operator {
		case token.Plus:
			result = left.Value + right.Value
		case token.Minus:
			result = left.Value - right.Value
		case token.Star:
			result = left.Value * right.Value
		case token.Slash:
			if right.Value == 0 {
				return nil, false
			}
			result = left.Value / right.Value
		case token.Percent:
			if right.Value == 0 {
				return nil, false
			}
			result = left.Value % right.Value
		default:
			return nil, false
		}
		return &ast.IntLiteral{Value: result, LitPos: e.OpPos}, true
	default:
		return nil, false
	}
}

func (c *Checker) defaultUntypedIntegerType(expr ast.Expression) Type {
	switch {
	case c.intLiteralFits(expr, TypeI32):
		return TypeI32
	case c.intLiteralFits(expr, TypeI64):
		return TypeI64
	default:
		return TypeInvalid
	}
}

func (c *Checker) currentCanPropagateError() bool {
	if c.current == nil {
		return false
	}
	return c.current.signature.Errorable || c.current.signature.Return == TypeError
}

func enumPayloadTypeName(enumName, caseName string) Type {
	return Type(enumName + "." + caseName)
}

func (c *Checker) lookupEnumCaseType(name string) (EnumInfo, EnumCaseInfo, bool) {
	idx := strings.LastIndex(name, ".")
	if idx <= 0 {
		return EnumInfo{}, EnumCaseInfo{}, false
	}
	enumName := name[:idx]
	caseName := name[idx+1:]
	enumInfo, ok := c.info.Enums[enumName]
	if !ok {
		return EnumInfo{}, EnumCaseInfo{}, false
	}
	enumCase, _, ok := enumInfo.Case(caseName)
	if !ok {
		return EnumInfo{}, EnumCaseInfo{}, false
	}
	return enumInfo, enumCase, true
}

func (c *Checker) satisfiesInterface(concrete, iface Type) (ok bool, message string) {
	if concrete == iface {
		return true, ""
	}
	if concrete == TypeInvalid || iface == TypeInvalid {
		return false, ""
	}
	if _, ok := c.info.Interfaces[string(concrete)]; ok {
		return false, fmt.Sprintf("cannot assign %s to %s", concrete, iface)
	}
	info, ok := c.info.Interfaces[string(iface)]
	if !ok {
		return false, ""
	}
	for _, method := range info.Methods {
		sig, ok := c.lookupMethod(concrete, method.Name)
		if !ok {
			return false, fmt.Sprintf("type %q does not satisfy interface %q (missing method %q)", concrete, iface, method.Name)
		}
		if len(sig.Params) != len(method.Params)+1 {
			return false, fmt.Sprintf("method %q on %q does not match interface %q", method.Name, concrete, iface)
		}
		if sig.Return != method.Return || sig.Errorable != method.Errorable {
			return false, fmt.Sprintf("method %q on %q does not match interface %q", method.Name, concrete, iface)
		}
		for i, param := range method.Params {
			if sig.Params[i+1] != param {
				return false, fmt.Sprintf("method %q on %q does not match interface %q", method.Name, concrete, iface)
			}
		}
	}
	return true, ""
}
