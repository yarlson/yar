package token

import "fmt"

type Kind int

const (
	Illegal Kind = iota
	EOF

	Ident
	Int
	String

	Assign
	ColonAssign
	Bang
	Question
	Comma
	Dot
	LParen
	RParen
	LBrace
	RBrace
	Pipe
	Plus
	Minus
	Star
	Slash
	Less
	LessEqual
	Greater
	GreaterEqual
	EqualEqual
	BangEqual

	Package
	Fn
	Let
	Or
	If
	Return
	True
	False
	Error
)

func (k Kind) String() string {
	switch k {
	case Illegal:
		return "illegal"
	case EOF:
		return "eof"
	case Ident:
		return "identifier"
	case Int:
		return "integer"
	case String:
		return "string"
	case Assign:
		return "="
	case ColonAssign:
		return ":="
	case Bang:
		return "!"
	case Question:
		return "?"
	case Comma:
		return ","
	case Dot:
		return "."
	case LParen:
		return "("
	case RParen:
		return ")"
	case LBrace:
		return "{"
	case RBrace:
		return "}"
	case Pipe:
		return "|"
	case Plus:
		return "+"
	case Minus:
		return "-"
	case Star:
		return "*"
	case Slash:
		return "/"
	case Less:
		return "<"
	case LessEqual:
		return "<="
	case Greater:
		return ">"
	case GreaterEqual:
		return ">="
	case EqualEqual:
		return "=="
	case BangEqual:
		return "!="
	case Package:
		return "package"
	case Fn:
		return "fn"
	case Let:
		return "let"
	case Or:
		return "or"
	case If:
		return "if"
	case Return:
		return "return"
	case True:
		return "true"
	case False:
		return "false"
	case Error:
		return "error"
	default:
		return fmt.Sprintf("token(%d)", k)
	}
}

type Position struct {
	Line   int
	Column int
}

func (p Position) String() string {
	return fmt.Sprintf("%d:%d", p.Line, p.Column)
}

type Token struct {
	Kind Kind
	Text string
	Pos  Position
}
