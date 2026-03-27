package ast

import "yar/internal/token"

type Node interface {
	Pos() token.Position
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
	Functions   []*FunctionDecl
}

func (p *Program) Pos() token.Position {
	return p.PackagePos
}

type FunctionDecl struct {
	Name         string
	NamePos      token.Position
	Params       []Param
	Return       TypeRef
	ReturnIsBang bool
	Body         *BlockStmt
}

func (f *FunctionDecl) Pos() token.Position {
	return f.NamePos
}

type Param struct {
	Name    string
	NamePos token.Position
	Type    TypeRef
}

type TypeRef struct {
	Name string
	Pos  token.Position
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

type AssignStmt struct {
	Name    string
	NamePos token.Position
	Value   Expression
}

func (s *AssignStmt) Pos() token.Position {
	return s.NamePos
}

func (*AssignStmt) stmtNode() {}

type IfStmt struct {
	IfPos token.Position
	Cond  Expression
	Then  *BlockStmt
}

func (s *IfStmt) Pos() token.Position {
	return s.IfPos
}

func (*IfStmt) stmtNode() {}

type ReturnStmt struct {
	ReturnPos token.Position
	Value     Expression
}

func (s *ReturnStmt) Pos() token.Position {
	return s.ReturnPos
}

func (*ReturnStmt) stmtNode() {}

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

type ErrorLiteral struct {
	Name   string
	ErrPos token.Position
}

func (e *ErrorLiteral) Pos() token.Position {
	return e.ErrPos
}

func (*ErrorLiteral) exprNode() {}

type CallExpr struct {
	Name    string
	NamePos token.Position
	Args    []Expression
}

func (e *CallExpr) Pos() token.Position {
	return e.NamePos
}

func (*CallExpr) exprNode() {}

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
