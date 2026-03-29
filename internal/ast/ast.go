package ast

import (
	"fmt"
	"strings"
	"yar/internal/token"
)

type Node interface {
	Pos() token.Position
}

type Declaration interface {
	Node
	declNode()
}

type Statement interface {
	Node
	stmtNode()
}

type Expression interface {
	Node
	exprNode()
}

type Program struct {
	PackagePos  token.Position
	PackageName string
	Imports     []ImportDecl
	Structs     []*StructDecl
	Interfaces  []*InterfaceDecl
	Enums       []*EnumDecl
	Functions   []*FunctionDecl
}

func (p *Program) Pos() token.Position {
	return p.PackagePos
}

type TypeRefKind int

const (
	NamedTypeRef TypeRefKind = iota
	PointerTypeRef
	ArrayTypeRef
	SliceTypeRef
	MapTypeRef
	FunctionTypeRef
)

type TypeRef struct {
	Kind      TypeRefKind
	Name      string
	TypeArgs  []TypeRef
	ArrayLen  int
	Elem      *TypeRef
	Key       *TypeRef
	Value     *TypeRef
	Params    []TypeRef
	Return    *TypeRef
	Errorable bool
	Pos       token.Position
}

func (r TypeRef) String() string {
	switch r.Kind {
	case PointerTypeRef:
		if r.Elem == nil {
			return "*"
		}
		return "*" + r.Elem.String()
	case ArrayTypeRef:
		if r.Elem == nil {
			return fmt.Sprintf("[%d]", r.ArrayLen)
		}
		return fmt.Sprintf("[%d]%s", r.ArrayLen, r.Elem.String())
	case SliceTypeRef:
		if r.Elem == nil {
			return "[]"
		}
		return "[]" + r.Elem.String()
	case MapTypeRef:
		var key, value string
		if r.Key != nil {
			key = r.Key.String()
		}
		if r.Value != nil {
			value = r.Value.String()
		}
		return "map[" + key + "]" + value
	case FunctionTypeRef:
		params := make([]string, 0, len(r.Params))
		for _, param := range r.Params {
			params = append(params, param.String())
		}
		ret := ""
		if r.Return != nil {
			ret = r.Return.String()
		}
		if r.Errorable {
			ret = "!" + ret
		}
		return "fn(" + strings.Join(params, ", ") + ") " + ret
	default:
		if len(r.TypeArgs) == 0 {
			return r.Name
		}
		parts := make([]string, 0, len(r.TypeArgs))
		for _, arg := range r.TypeArgs {
			parts = append(parts, arg.String())
		}
		return r.Name + "[" + strings.Join(parts, ", ") + "]"
	}
}

type ImportDecl struct {
	ImportPos token.Position
	Path      string
	PathPos   token.Position
}

type TypeParam struct {
	Name string
	Pos  token.Position
}

type StructDecl struct {
	StructPos  token.Position
	Exported   bool
	Name       string
	NamePos    token.Position
	TypeParams []TypeParam
	Fields     []StructField
}

func (d *StructDecl) Pos() token.Position {
	return d.StructPos
}

func (*StructDecl) declNode() {}

type StructField struct {
	Name    string
	NamePos token.Position
	Type    TypeRef
}

type InterfaceDecl struct {
	InterfacePos token.Position
	Exported     bool
	Name         string
	NamePos      token.Position
	Methods      []InterfaceMethodDecl
}

func (d *InterfaceDecl) Pos() token.Position {
	return d.InterfacePos
}

func (*InterfaceDecl) declNode() {}

type InterfaceMethodDecl struct {
	Name         string
	NamePos      token.Position
	Params       []Param
	Return       TypeRef
	ReturnIsBang bool
}

type EnumDecl struct {
	EnumPos  token.Position
	Exported bool
	Name     string
	NamePos  token.Position
	Cases    []EnumCaseDecl
}

func (d *EnumDecl) Pos() token.Position {
	return d.EnumPos
}

func (*EnumDecl) declNode() {}

type EnumCaseDecl struct {
	Name    string
	NamePos token.Position
	Fields  []StructField
}

type FunctionDecl struct {
	Exported     bool
	Name         string
	NamePos      token.Position
	TypeParams   []TypeParam
	Receiver     *ReceiverDecl
	Params       []Param
	Return       TypeRef
	ReturnIsBang bool
	Body         *BlockStmt
}

func (f *FunctionDecl) Pos() token.Position {
	return f.NamePos
}

func (*FunctionDecl) declNode() {}

type Param struct {
	Name    string
	NamePos token.Position
	Type    TypeRef
}

type ReceiverDecl struct {
	Name    string
	NamePos token.Position
	Type    TypeRef
}

type BlockStmt struct {
	LBrace token.Position
	Stmts  []Statement
}

func (b *BlockStmt) Pos() token.Position {
	return b.LBrace
}

func (*BlockStmt) stmtNode() {}

type LetStmt struct {
	LetPos  token.Position
	Name    string
	NamePos token.Position
	Value   Expression
}

func (s *LetStmt) Pos() token.Position {
	return s.LetPos
}

func (*LetStmt) stmtNode() {}

type VarStmt struct {
	VarPos  token.Position
	Name    string
	NamePos token.Position
	Type    TypeRef
	Value   Expression
}

func (s *VarStmt) Pos() token.Position {
	return s.VarPos
}

func (*VarStmt) stmtNode() {}

type AssignStmt struct {
	Target Expression
	Value  Expression
}

func (s *AssignStmt) Pos() token.Position {
	return s.Target.Pos()
}

func (*AssignStmt) stmtNode() {}

type IfStmt struct {
	IfPos token.Position
	Cond  Expression
	Then  *BlockStmt
	Else  Statement
}

func (s *IfStmt) Pos() token.Position {
	return s.IfPos
}

func (*IfStmt) stmtNode() {}

type ForStmt struct {
	ForPos token.Position
	Init   Statement
	Cond   Expression
	Post   Statement
	Body   *BlockStmt
}

func (s *ForStmt) Pos() token.Position {
	return s.ForPos
}

func (*ForStmt) stmtNode() {}

type BreakStmt struct {
	BreakPos token.Position
}

func (s *BreakStmt) Pos() token.Position {
	return s.BreakPos
}

func (*BreakStmt) stmtNode() {}

type ContinueStmt struct {
	ContinuePos token.Position
}

func (s *ContinueStmt) Pos() token.Position {
	return s.ContinuePos
}

func (*ContinueStmt) stmtNode() {}

type ReturnStmt struct {
	ReturnPos token.Position
	Value     Expression
}

func (s *ReturnStmt) Pos() token.Position {
	return s.ReturnPos
}

func (*ReturnStmt) stmtNode() {}

type MatchStmt struct {
	MatchPos token.Position
	Value    Expression
	Arms     []MatchArm
}

func (s *MatchStmt) Pos() token.Position {
	return s.MatchPos
}

func (*MatchStmt) stmtNode() {}

type MatchArm struct {
	CasePos     token.Position
	EnumType    TypeRef
	CaseName    string
	CaseNamePos token.Position
	BindName    string
	BindNamePos token.Position
	BindIgnore  bool
	Body        *BlockStmt
}

type ExprStmt struct {
	Expr Expression
}

func (s *ExprStmt) Pos() token.Position {
	return s.Expr.Pos()
}

func (*ExprStmt) stmtNode() {}

type IdentExpr struct {
	Name    string
	NamePos token.Position
}

func (e *IdentExpr) Pos() token.Position {
	return e.NamePos
}

func (*IdentExpr) exprNode() {}

type IntLiteral struct {
	Value  int64
	LitPos token.Position
}

func (e *IntLiteral) Pos() token.Position {
	return e.LitPos
}

func (*IntLiteral) exprNode() {}

type StringLiteral struct {
	Value  string
	LitPos token.Position
}

func (e *StringLiteral) Pos() token.Position {
	return e.LitPos
}

func (*StringLiteral) exprNode() {}

type BoolLiteral struct {
	Value  bool
	LitPos token.Position
}

func (e *BoolLiteral) Pos() token.Position {
	return e.LitPos
}

func (*BoolLiteral) exprNode() {}

type NilLiteral struct {
	LitPos token.Position
}

func (e *NilLiteral) Pos() token.Position {
	return e.LitPos
}

func (*NilLiteral) exprNode() {}

type ErrorLiteral struct {
	Name   string
	ErrPos token.Position
}

func (e *ErrorLiteral) Pos() token.Position {
	return e.ErrPos
}

func (*ErrorLiteral) exprNode() {}

type TypeApplicationExpr struct {
	Inner       Expression
	LBracketPos token.Position
	TypeArgs    []TypeRef
}

func (e *TypeApplicationExpr) Pos() token.Position {
	return e.Inner.Pos()
}

func (*TypeApplicationExpr) exprNode() {}

type CallExpr struct {
	Callee Expression
	Args   []Expression
}

func (e *CallExpr) Pos() token.Position {
	return e.Callee.Pos()
}

func (*CallExpr) exprNode() {}

type FunctionLiteralExpr struct {
	FnPos        token.Position
	Params       []Param
	Return       TypeRef
	ReturnIsBang bool
	Body         *BlockStmt
}

func (e *FunctionLiteralExpr) Pos() token.Position {
	return e.FnPos
}

func (*FunctionLiteralExpr) exprNode() {}

type UnaryExpr struct {
	Operator token.Kind
	OpPos    token.Position
	Inner    Expression
}

func (e *UnaryExpr) Pos() token.Position {
	return e.OpPos
}

func (*UnaryExpr) exprNode() {}

type BinaryExpr struct {
	Left     Expression
	Operator token.Kind
	OpPos    token.Position
	Right    Expression
}

func (e *BinaryExpr) Pos() token.Position {
	return e.Left.Pos()
}

func (*BinaryExpr) exprNode() {}

type GroupExpr struct {
	Inner Expression
}

func (e *GroupExpr) Pos() token.Position {
	return e.Inner.Pos()
}

func (*GroupExpr) exprNode() {}

type SelectorExpr struct {
	Inner   Expression
	DotPos  token.Position
	Name    string
	NamePos token.Position
}

func (e *SelectorExpr) Pos() token.Position {
	return e.Inner.Pos()
}

func (*SelectorExpr) exprNode() {}

type IndexExpr struct {
	Inner       Expression
	LBracketPos token.Position
	Index       Expression
}

func (e *IndexExpr) Pos() token.Position {
	return e.Inner.Pos()
}

func (*IndexExpr) exprNode() {}

type SliceExpr struct {
	Inner       Expression
	LBracketPos token.Position
	Start       Expression
	ColonPos    token.Position
	End         Expression
}

func (e *SliceExpr) Pos() token.Position {
	return e.Inner.Pos()
}

func (*SliceExpr) exprNode() {}

type StructLiteralExpr struct {
	Type   TypeRef
	LBrace token.Position
	Fields []StructLiteralField
}

func (e *StructLiteralExpr) Pos() token.Position {
	return e.Type.Pos
}

func (*StructLiteralExpr) exprNode() {}

type StructLiteralField struct {
	Name    string
	NamePos token.Position
	Value   Expression
}

type ArrayLiteralExpr struct {
	Type     TypeRef
	LBrace   token.Position
	Elements []Expression
}

func (e *ArrayLiteralExpr) Pos() token.Position {
	return e.Type.Pos
}

func (*ArrayLiteralExpr) exprNode() {}

type SliceLiteralExpr struct {
	Type     TypeRef
	LBrace   token.Position
	Elements []Expression
}

func (e *SliceLiteralExpr) Pos() token.Position {
	return e.Type.Pos
}

func (*SliceLiteralExpr) exprNode() {}

type MapLiteralExpr struct {
	Type   TypeRef
	LBrace token.Position
	Pairs  []MapLiteralPair
}

func (e *MapLiteralExpr) Pos() token.Position {
	return e.Type.Pos
}

func (*MapLiteralExpr) exprNode() {}

type MapLiteralPair struct {
	Key      Expression
	KeyPos   token.Position
	Value    Expression
	ValuePos token.Position
}

type PropagateExpr struct {
	Inner       Expression
	QuestionPos token.Position
}

func (e *PropagateExpr) Pos() token.Position {
	return e.Inner.Pos()
}

func (*PropagateExpr) exprNode() {}

type HandleExpr struct {
	Inner   Expression
	OrPos   token.Position
	ErrName string
	ErrPos  token.Position
	Handler *BlockStmt
}

func (e *HandleExpr) Pos() token.Position {
	return e.Inner.Pos()
}

func (*HandleExpr) exprNode() {}
