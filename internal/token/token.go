package token

import "fmt"

type Kind int

const (
	Illegal Kind = iota
	EOF

	Ident
	Int
	String
	Char

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
	PlusAssign
	MinusAssign
	StarAssign
	SlashAssign
	PercentAssign
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
	Interface
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
	Taskgroup
	Spawn
	True
	False
	Nil
	Error
	Map
	Chan
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
	case Char:
		return "character"
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
	case PlusAssign:
		return "+="
	case MinusAssign:
		return "-="
	case StarAssign:
		return "*="
	case SlashAssign:
		return "/="
	case PercentAssign:
		return "%="
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
	case Interface:
		return "interface"
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
	case Taskgroup:
		return "taskgroup"
	case Spawn:
		return "spawn"
	case True:
		return "true"
	case False:
		return "false"
	case Nil:
		return "nil"
	case Error:
		return "error"
	case Map:
		return "map"
	case Chan:
		return "chan"
	default:
		return fmt.Sprintf("token(%d)", k)
	}
}

func CompoundAssignOp(k Kind) (Kind, bool) {
	switch k {
	case PlusAssign:
		return Plus, true
	case MinusAssign:
		return Minus, true
	case StarAssign:
		return Star, true
	case SlashAssign:
		return Slash, true
	case PercentAssign:
		return Percent, true
	default:
		return Illegal, false
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
