package lexer

import (
	"strconv"
	"unicode"
	"unicode/utf8"
	"yar/internal/diag"
	"yar/internal/token"
)

type Lexer struct {
	src    string
	offset int
	line   int
	column int
	diag   diag.List
}

func New(src string) *Lexer {
	return &Lexer{
		src:    src,
		line:   1,
		column: 1,
	}
}

func (l *Lexer) Diagnostics() []diag.Diagnostic {
	return l.diag.Items()
}

func (l *Lexer) Lex() []token.Token {
	var tokens []token.Token
	for {
		l.skipTrivia()
		pos := token.Position{Line: l.line, Column: l.column}
		if l.offset >= len(l.src) {
			tokens = append(tokens, token.Token{Kind: token.EOF, Pos: pos})
			return tokens
		}

		r, width := utf8.DecodeRuneInString(l.src[l.offset:])
		switch {
		case isIdentStart(r):
			text := l.lexIdent()
			tokens = append(tokens, token.Token{
				Kind: lookupKeyword(text),
				Text: text,
				Pos:  pos,
			})
		case unicode.IsDigit(r):
			text := l.lexDigits()
			tokens = append(tokens, token.Token{Kind: token.Int, Text: text, Pos: pos})
		default:
			l.advanceWidth(width)
			switch r {
			case '=':
				if l.match('=') {
					tokens = append(tokens, token.Token{Kind: token.EqualEqual, Text: "==", Pos: pos})
				} else {
					tokens = append(tokens, token.Token{Kind: token.Assign, Text: "=", Pos: pos})
				}
			case '!':
				if l.match('=') {
					tokens = append(tokens, token.Token{Kind: token.BangEqual, Text: "!=", Pos: pos})
				} else {
					tokens = append(tokens, token.Token{Kind: token.Bang, Text: "!", Pos: pos})
				}
			case ',':
				tokens = append(tokens, token.Token{Kind: token.Comma, Text: ",", Pos: pos})
			case '.':
				tokens = append(tokens, token.Token{Kind: token.Dot, Text: ".", Pos: pos})
			case '(':
				tokens = append(tokens, token.Token{Kind: token.LParen, Text: "(", Pos: pos})
			case ')':
				tokens = append(tokens, token.Token{Kind: token.RParen, Text: ")", Pos: pos})
			case '{':
				tokens = append(tokens, token.Token{Kind: token.LBrace, Text: "{", Pos: pos})
			case '}':
				tokens = append(tokens, token.Token{Kind: token.RBrace, Text: "}", Pos: pos})
			case '+':
				tokens = append(tokens, token.Token{Kind: token.Plus, Text: "+", Pos: pos})
			case '-':
				tokens = append(tokens, token.Token{Kind: token.Minus, Text: "-", Pos: pos})
			case '*':
				tokens = append(tokens, token.Token{Kind: token.Star, Text: "*", Pos: pos})
			case '/':
				tokens = append(tokens, token.Token{Kind: token.Slash, Text: "/", Pos: pos})
			case '"':
				value := l.lexString(pos)
				tokens = append(tokens, token.Token{Kind: token.String, Text: value, Pos: pos})
			default:
				l.diag.Add(pos, "unexpected character %q", r)
				tokens = append(tokens, token.Token{Kind: token.Illegal, Text: string(r), Pos: pos})
			}
		}
	}
}

func (l *Lexer) skipTrivia() {
	for l.offset < len(l.src) {
		r, width := utf8.DecodeRuneInString(l.src[l.offset:])
		if r == '/' && l.peekNextRune(width) == '/' {
			l.advanceWidth(width)
			l.advanceWidth(1)
			for l.offset < len(l.src) {
				r2, width2 := utf8.DecodeRuneInString(l.src[l.offset:])
				l.advanceWidth(width2)
				if r2 == '\n' {
					break
				}
			}
			continue
		}
		if !unicode.IsSpace(r) {
			return
		}
		l.advanceWidth(width)
	}
}

func (l *Lexer) lexIdent() string {
	start := l.offset
	for l.offset < len(l.src) {
		r, width := utf8.DecodeRuneInString(l.src[l.offset:])
		if !isIdentPart(r) {
			break
		}
		l.advanceWidth(width)
	}
	return l.src[start:l.offset]
}

func (l *Lexer) lexDigits() string {
	start := l.offset
	for l.offset < len(l.src) {
		r, width := utf8.DecodeRuneInString(l.src[l.offset:])
		if !unicode.IsDigit(r) {
			break
		}
		l.advanceWidth(width)
	}
	return l.src[start:l.offset]
}

func (l *Lexer) lexString(pos token.Position) string {
	var out []rune
	for l.offset < len(l.src) {
		r, width := utf8.DecodeRuneInString(l.src[l.offset:])
		l.advanceWidth(width)
		switch r {
		case '"':
			return string(out)
		case '\\':
			if l.offset >= len(l.src) {
				l.diag.Add(pos, "unterminated string literal")
				return string(out)
			}
			esc, escWidth := utf8.DecodeRuneInString(l.src[l.offset:])
			l.advanceWidth(escWidth)
			switch esc {
			case 'n':
				out = append(out, '\n')
			case 't':
				out = append(out, '\t')
			case '\\':
				out = append(out, '\\')
			case '"':
				out = append(out, '"')
			default:
				l.diag.Add(pos, "unsupported string escape %q", "\\"+string(esc))
			}
		case '\n':
			l.diag.Add(pos, "unterminated string literal")
			return string(out)
		default:
			out = append(out, r)
		}
	}
	l.diag.Add(pos, "unterminated string literal")
	return string(out)
}

func (l *Lexer) match(expected rune) bool {
	if l.offset >= len(l.src) {
		return false
	}
	r, width := utf8.DecodeRuneInString(l.src[l.offset:])
	if r != expected {
		return false
	}
	l.advanceWidth(width)
	return true
}

func (l *Lexer) peekNextRune(currentWidth int) rune {
	next := l.offset + currentWidth
	if next >= len(l.src) {
		return utf8.RuneError
	}
	r, _ := utf8.DecodeRuneInString(l.src[next:])
	return r
}

func (l *Lexer) advanceWidth(width int) {
	text := l.src[l.offset : l.offset+width]
	l.offset += width
	for _, r := range text {
		if r == '\n' {
			l.line++
			l.column = 1
			continue
		}
		l.column++
	}
}

func isIdentStart(r rune) bool {
	return r == '_' || unicode.IsLetter(r)
}

func isIdentPart(r rune) bool {
	return isIdentStart(r) || unicode.IsDigit(r)
}

func lookupKeyword(text string) token.Kind {
	switch text {
	case "package":
		return token.Package
	case "fn":
		return token.Fn
	case "let":
		return token.Let
	case "if":
		return token.If
	case "return":
		return token.Return
	case "true":
		return token.True
	case "false":
		return token.False
	case "error":
		return token.Error
	default:
		return token.Ident
	}
}

func ParseIntLiteral(tok token.Token) (int32, error) {
	v, err := strconv.ParseInt(tok.Text, 10, 32)
	if err != nil {
		return 0, err
	}
	return int32(v), nil
}
