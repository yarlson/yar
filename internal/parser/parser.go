package parser

import (
	"fmt"
	"yar/internal/ast"
	"yar/internal/diag"
	"yar/internal/lexer"
	"yar/internal/token"
)

type Parser struct {
	tokens []token.Token
	index  int
	diag   diag.List
}

func Parse(src string) (*ast.Program, []diag.Diagnostic) {
	lex := lexer.New(src)
	tokens := lex.Lex()

	p := &Parser{tokens: tokens}
	prog := p.parseProgram()

	lexDiagnostics := lex.Diagnostics()
	parserDiagnostics := p.diag.Items()
	diagnostics := make([]diag.Diagnostic, 0, len(lexDiagnostics)+len(parserDiagnostics))
	diagnostics = append(diagnostics, lexDiagnostics...)
	diagnostics = append(diagnostics, parserDiagnostics...)
	return prog, diagnostics
}

func (p *Parser) parseProgram() *ast.Program {
	packageTok := p.expect(token.Package, "expected package declaration")
	nameTok := p.expect(token.Ident, "expected package name")

	program := &ast.Program{
		PackagePos:  packageTok.Pos,
		PackageName: nameTok.Text,
	}

	for !p.at(token.EOF) {
		if !p.at(token.Fn) {
			p.errorCurrent("expected function declaration")
			p.advance()
			continue
		}
		if fn := p.parseFunction(); fn != nil {
			program.Functions = append(program.Functions, fn)
		}
	}
	return program
}

func (p *Parser) parseFunction() *ast.FunctionDecl {
	p.expect(token.Fn, "expected fn")
	nameTok := p.expect(token.Ident, "expected function name")
	p.expect(token.LParen, "expected '(' after function name")

	var params []ast.Param
	for !p.at(token.RParen) && !p.at(token.EOF) {
		paramName := p.expect(token.Ident, "expected parameter name")
		paramType := p.parseTypeRef()
		params = append(params, ast.Param{
			Name:    paramName.Text,
			NamePos: paramName.Pos,
			Type:    paramType,
		})
		if !p.at(token.Comma) {
			break
		}
		p.advance()
	}
	p.expect(token.RParen, "expected ')' after parameters")

	returnIsBang := false
	if p.at(token.Bang) {
		returnIsBang = true
		p.advance()
	}
	returnType := p.parseTypeRef()
	body := p.parseBlock()

	return &ast.FunctionDecl{
		Name:         nameTok.Text,
		NamePos:      nameTok.Pos,
		Params:       params,
		Return:       returnType,
		ReturnIsBang: returnIsBang,
		Body:         body,
	}
}

func (p *Parser) parseTypeRef() ast.TypeRef {
	tok := p.expect(token.Ident, "expected type name")
	return ast.TypeRef{Name: tok.Text, Pos: tok.Pos}
}

func (p *Parser) parseBlock() *ast.BlockStmt {
	lbrace := p.expect(token.LBrace, "expected '{'")
	block := &ast.BlockStmt{LBrace: lbrace.Pos}
	for !p.at(token.RBrace) && !p.at(token.EOF) {
		stmt := p.parseStatement()
		if stmt != nil {
			block.Stmts = append(block.Stmts, stmt)
		}
	}
	p.expect(token.RBrace, "expected '}'")
	return block
}

func (p *Parser) parseStatement() ast.Statement {
	switch p.current().Kind {
	case token.Let:
		return p.parseLet()
	case token.If:
		return p.parseIf()
	case token.Return:
		return p.parseReturn()
	case token.LBrace:
		return p.parseBlock()
	case token.Ident:
		if p.peek().Kind == token.Assign {
			return p.parseAssign()
		}
		fallthrough
	default:
		expr := p.parseExpression()
		if expr == nil {
			return nil
		}
		return &ast.ExprStmt{Expr: expr}
	}
}

func (p *Parser) parseLet() ast.Statement {
	letTok := p.expect(token.Let, "expected let")
	nameTok := p.expect(token.Ident, "expected local name")
	p.expect(token.Assign, "expected '=' in let statement")
	value := p.parseExpression()
	return &ast.LetStmt{
		LetPos:  letTok.Pos,
		Name:    nameTok.Text,
		NamePos: nameTok.Pos,
		Value:   value,
	}
}

func (p *Parser) parseAssign() ast.Statement {
	nameTok := p.expect(token.Ident, "expected local name")
	p.expect(token.Assign, "expected '=' in assignment")
	value := p.parseExpression()
	return &ast.AssignStmt{
		Name:    nameTok.Text,
		NamePos: nameTok.Pos,
		Value:   value,
	}
}

func (p *Parser) parseIf() ast.Statement {
	ifTok := p.expect(token.If, "expected if")
	cond := p.parseExpression()
	then := p.parseBlock()
	return &ast.IfStmt{
		IfPos: ifTok.Pos,
		Cond:  cond,
		Then:  then,
	}
}

func (p *Parser) parseReturn() ast.Statement {
	returnTok := p.expect(token.Return, "expected return")
	if p.at(token.RBrace) || p.at(token.EOF) {
		return &ast.ReturnStmt{ReturnPos: returnTok.Pos}
	}
	return &ast.ReturnStmt{
		ReturnPos: returnTok.Pos,
		Value:     p.parseExpression(),
	}
}

func (p *Parser) parseExpression() ast.Expression {
	return p.parseEquality()
}

func (p *Parser) parseEquality() ast.Expression {
	expr := p.parseComparison()
	for p.at(token.EqualEqual) || p.at(token.BangEqual) {
		op := p.current()
		p.advance()
		right := p.parseComparison()
		expr = &ast.BinaryExpr{
			Left:     expr,
			Operator: op.Kind,
			OpPos:    op.Pos,
			Right:    right,
		}
	}
	return expr
}

func (p *Parser) parseComparison() ast.Expression {
	expr := p.parseAdditive()
	for p.at(token.Less) || p.at(token.LessEqual) || p.at(token.Greater) || p.at(token.GreaterEqual) {
		op := p.current()
		p.advance()
		right := p.parseAdditive()
		expr = &ast.BinaryExpr{
			Left:     expr,
			Operator: op.Kind,
			OpPos:    op.Pos,
			Right:    right,
		}
	}
	return expr
}

func (p *Parser) parseAdditive() ast.Expression {
	expr := p.parseMultiplicative()
	for p.at(token.Plus) || p.at(token.Minus) {
		op := p.current()
		p.advance()
		right := p.parseMultiplicative()
		expr = &ast.BinaryExpr{
			Left:     expr,
			Operator: op.Kind,
			OpPos:    op.Pos,
			Right:    right,
		}
	}
	return expr
}

func (p *Parser) parseMultiplicative() ast.Expression {
	expr := p.parsePrimary()
	for p.at(token.Star) || p.at(token.Slash) {
		op := p.current()
		p.advance()
		right := p.parsePrimary()
		expr = &ast.BinaryExpr{
			Left:     expr,
			Operator: op.Kind,
			OpPos:    op.Pos,
			Right:    right,
		}
	}
	return expr
}

func (p *Parser) parsePrimary() ast.Expression {
	tok := p.current()
	switch tok.Kind {
	case token.Ident:
		p.advance()
		if p.at(token.LParen) {
			return p.finishCall(tok)
		}
		return &ast.IdentExpr{Name: tok.Text, NamePos: tok.Pos}
	case token.Int:
		p.advance()
		value, err := lexer.ParseIntLiteral(tok)
		if err != nil {
			p.diag.Add(tok.Pos, "invalid integer literal %q", tok.Text)
			value = 0
		}
		return &ast.IntLiteral{Value: value, LitPos: tok.Pos}
	case token.String:
		p.advance()
		return &ast.StringLiteral{Value: tok.Text, LitPos: tok.Pos}
	case token.True:
		p.advance()
		return &ast.BoolLiteral{Value: true, LitPos: tok.Pos}
	case token.False:
		p.advance()
		return &ast.BoolLiteral{Value: false, LitPos: tok.Pos}
	case token.Error:
		p.advance()
		p.expect(token.Dot, "expected '.' after error")
		nameTok := p.expect(token.Ident, "expected error name")
		return &ast.ErrorLiteral{Name: nameTok.Text, ErrPos: tok.Pos}
	case token.LParen:
		p.advance()
		inner := p.parseExpression()
		p.expect(token.RParen, "expected ')'")
		return &ast.GroupExpr{Inner: inner}
	default:
		p.errorCurrent("expected expression")
		p.advance()
		return nil
	}
}

func (p *Parser) finishCall(name token.Token) ast.Expression {
	p.expect(token.LParen, "expected '('")
	var args []ast.Expression
	for !p.at(token.RParen) && !p.at(token.EOF) {
		args = append(args, p.parseExpression())
		if !p.at(token.Comma) {
			break
		}
		p.advance()
	}
	p.expect(token.RParen, "expected ')'")
	return &ast.CallExpr{
		Name:    name.Text,
		NamePos: name.Pos,
		Args:    args,
	}
}

func (p *Parser) current() token.Token {
	if p.index >= len(p.tokens) {
		return token.Token{Kind: token.EOF}
	}
	return p.tokens[p.index]
}

func (p *Parser) peek() token.Token {
	if p.index+1 >= len(p.tokens) {
		return token.Token{Kind: token.EOF}
	}
	return p.tokens[p.index+1]
}

func (p *Parser) at(kind token.Kind) bool {
	return p.current().Kind == kind
}

func (p *Parser) advance() {
	if p.index < len(p.tokens) {
		p.index++
	}
}

func (p *Parser) expect(kind token.Kind, message string) token.Token {
	tok := p.current()
	if tok.Kind != kind {
		p.diag.Add(tok.Pos, "%s, got %s", message, describeToken(tok))
		return token.Token{Kind: kind, Pos: tok.Pos}
	}
	p.advance()
	return tok
}

func (p *Parser) errorCurrent(format string, args ...any) {
	p.diag.Add(p.current().Pos, format, args...)
}

func describeToken(tok token.Token) string {
	if tok.Kind == token.Ident || tok.Kind == token.Int || tok.Kind == token.String {
		return fmt.Sprintf("%s %q", tok.Kind, tok.Text)
	}
	return tok.Kind.String()
}
