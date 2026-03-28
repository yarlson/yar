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
	Colon
	ColonAssign
	Semicolon
	Bang
	Question
	Comma
	Dot
	Amp
	LParen
	RParen
	LBrace
	RBrace
	LBracket
	RBracket
	Pipe
	AmpAmp
	PipePipe
	Plus
	Minus
	Star
	Slash
	Percent
	Less
	LessEqual
	Greater
	GreaterEqual
	EqualEqual
	BangEqual

	Package
	Import
	Fn
	Pub
	Let
	Var
	Struct
	Enum
	Or
	If
	Else
	For
	Break
	Continue
	Return
	Match
	Case
	True
	False
	Nil
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
	case Colon:
		return ":"
	case ColonAssign:
		return ":="
	case Semicolon:
		return ";"
	case Bang:
		return "!"
	case Question:
		return "?"
	case Comma:
		return ","
	case Dot:
		return "."
	case Amp:
		return "&"
	case LParen:
		return "("
	case RParen:
		return ")"
	case LBrace:
		return "{"
	case RBrace:
		return "}"
	case LBracket:
		return "["
	case RBracket:
		return "]"
	case Pipe:
		return "|"
	case AmpAmp:
		return "&&"
	case PipePipe:
		return "||"
	case Plus:
		return "+"
	case Minus:
		return "-"
	case Star:
		return "*"
	case Slash:
		return "/"
	case Percent:
		return "%"
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
	case Import:
		return "import"
	case Fn:
		return "fn"
	case Pub:
		return "pub"
	case Let:
		return "let"
	case Var:
		return "var"
	case Struct:
		return "struct"
	case Enum:
		return "enum"
	case Or:
		return "or"
	case If:
		return "if"
	case Else:
		return "else"
	case For:
		return "for"
	case Break:
		return "break"
	case Continue:
		return "continue"
	case Return:
		return "return"
	case Match:
		return "match"
	case Case:
		return "case"
	case True:
		return "true"
	case False:
		return "false"
	case Nil:
		return "nil"
	case Error:
		return "error"
	default:
		return fmt.Sprintf("token(%d)", k)
	}
}

type Position struct {
	File   string
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
