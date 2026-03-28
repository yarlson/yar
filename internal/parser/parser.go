package parser

import (
	"fmt"
	"strings"
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
	return ParseFile("", src)
}

func ParseFile(path, src string) (*ast.Program, []diag.Diagnostic) {
	lex := lexer.NewFile(src, path)
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

	for p.at(token.Import) {
		program.Imports = append(program.Imports, p.parseImport())
	}

	for !p.at(token.EOF) {
		exported := false
		if p.at(token.Pub) {
			exported = true
			p.advance()
		}
		switch p.current().Kind {
		case token.Struct:
			if decl := p.parseStruct(exported); decl != nil {
				program.Structs = append(program.Structs, decl)
			}
		case token.Enum:
			if decl := p.parseEnum(exported); decl != nil {
				program.Enums = append(program.Enums, decl)
			}
		case token.Fn:
			if fn := p.parseFunction(exported); fn != nil {
				program.Functions = append(program.Functions, fn)
			}
		default:
			p.errorCurrent("expected function, struct, or enum declaration")
			p.advance()
		}
	}
	return program
}

func (p *Parser) parseImport() ast.ImportDecl {
	importTok := p.expect(token.Import, "expected import")
	pathTok := p.expect(token.String, "expected import path string")
	return ast.ImportDecl{
		ImportPos: importTok.Pos,
		Path:      pathTok.Text,
		PathPos:   pathTok.Pos,
	}
}

func (p *Parser) parseStruct(exported bool) *ast.StructDecl {
	structTok := p.expect(token.Struct, "expected struct")
	nameTok := p.expect(token.Ident, "expected struct name")
	p.expect(token.LBrace, "expected '{' after struct name")

	decl := &ast.StructDecl{
		StructPos: structTok.Pos,
		Exported:  exported,
		Name:      nameTok.Text,
		NamePos:   nameTok.Pos,
	}
	for !p.at(token.RBrace) && !p.at(token.EOF) {
		fieldName := p.expect(token.Ident, "expected field name")
		fieldType := p.parseTypeRef()
		decl.Fields = append(decl.Fields, ast.StructField{
			Name:    fieldName.Text,
			NamePos: fieldName.Pos,
			Type:    fieldType,
		})
	}
	p.expect(token.RBrace, "expected '}' after struct body")
	return decl
}

func (p *Parser) parseEnum(exported bool) *ast.EnumDecl {
	enumTok := p.expect(token.Enum, "expected enum")
	nameTok := p.expect(token.Ident, "expected enum name")
	p.expect(token.LBrace, "expected '{' after enum name")

	decl := &ast.EnumDecl{
		EnumPos:  enumTok.Pos,
		Exported: exported,
		Name:     nameTok.Text,
		NamePos:  nameTok.Pos,
	}
	for !p.at(token.RBrace) && !p.at(token.EOF) {
		caseName := p.expect(token.Ident, "expected enum case name")
		enumCase := ast.EnumCaseDecl{
			Name:    caseName.Text,
			NamePos: caseName.Pos,
		}
		if p.at(token.LBrace) {
			p.advance()
			for !p.at(token.RBrace) && !p.at(token.EOF) {
				fieldName := p.expect(token.Ident, "expected payload field name")
				fieldType := p.parseTypeRef()
				enumCase.Fields = append(enumCase.Fields, ast.StructField{
					Name:    fieldName.Text,
					NamePos: fieldName.Pos,
					Type:    fieldType,
				})
			}
			p.expect(token.RBrace, "expected '}' after enum payload")
		}
		decl.Cases = append(decl.Cases, enumCase)
	}
	p.expect(token.RBrace, "expected '}' after enum body")
	return decl
}

func (p *Parser) parseFunction(exported bool) *ast.FunctionDecl {
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
		Exported:     exported,
		Name:         nameTok.Text,
		NamePos:      nameTok.Pos,
		Params:       params,
		Return:       returnType,
		ReturnIsBang: returnIsBang,
		Body:         body,
	}
}

func (p *Parser) parseTypeRef() ast.TypeRef {
	if p.at(token.Star) {
		star := p.expect(token.Star, "expected '*'")
		inner := p.parseTypeRef()
		return ast.TypeRef{
			Name: "*" + inner.Name,
			Pos:  star.Pos,
		}
	}

	if p.at(token.LBracket) {
		lbracket := p.expect(token.LBracket, "expected '['")
		if p.at(token.RBracket) {
			p.advance()
			elem := p.parseTypeRef()
			return ast.TypeRef{
				Name: "[]" + elem.Name,
				Pos:  lbracket.Pos,
			}
		}
		lengthTok := p.expect(token.Int, "expected array length")
		p.expect(token.RBracket, "expected ']' after array length")
		elem := p.parseTypeRef()
		return ast.TypeRef{
			Name: fmt.Sprintf("[%s]%s", lengthTok.Text, elem.Name),
			Pos:  lbracket.Pos,
		}
	}

	if p.at(token.Map) {
		mapTok := p.expect(token.Map, "expected map")
		p.expect(token.LBracket, "expected '[' after map")
		keyType := p.parseTypeRef()
		p.expect(token.RBracket, "expected ']' after map key type")
		valueType := p.parseTypeRef()
		return ast.TypeRef{
			Name: "map[" + keyType.Name + "]" + valueType.Name,
			Pos:  mapTok.Pos,
		}
	}

	tok := p.current()
	if tok.Kind != token.Ident && tok.Kind != token.Error {
		tok = p.expect(token.Ident, "expected type name")
		return ast.TypeRef{Name: tok.Text, Pos: tok.Pos}
	}
	p.advance()
	name := tok.Text
	if tok.Kind == token.Ident {
		for p.at(token.Dot) {
			p.advance()
			partTok := p.expect(token.Ident, "expected type name after '.'")
			name += "." + partTok.Text
		}
	}
	return ast.TypeRef{Name: name, Pos: tok.Pos}
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
		p.errorCurrent("use ':=' for local declarations; 'let' is no longer supported")
		p.advance()
		return nil
	case token.Var:
		return p.parseVarDecl()
	case token.If:
		return p.parseIf()
	case token.For:
		return p.parseFor()
	case token.Break:
		return p.parseBreak()
	case token.Continue:
		return p.parseContinue()
	case token.Return:
		return p.parseReturn()
	case token.Match:
		return p.parseMatch()
	case token.LBrace:
		return p.parseBlock()
	case token.Ident:
		if p.peek().Kind == token.ColonAssign {
			return p.parseShortDecl()
		}
	}

	expr := p.parseExpression()
	if expr == nil {
		return nil
	}
	if p.at(token.Assign) {
		return p.parseAssign(expr)
	}
	return &ast.ExprStmt{Expr: expr}
}

func (p *Parser) parseShortDecl() ast.Statement {
	nameTok := p.expect(token.Ident, "expected local name")
	p.expect(token.ColonAssign, "expected ':=' in declaration")
	value := p.parseExpression()
	return &ast.LetStmt{
		LetPos:  nameTok.Pos,
		Name:    nameTok.Text,
		NamePos: nameTok.Pos,
		Value:   value,
	}
}

func (p *Parser) parseVarDecl() ast.Statement {
	varTok := p.expect(token.Var, "expected var")
	nameTok := p.expect(token.Ident, "expected local name")
	typ := p.parseTypeRef()

	var value ast.Expression
	if p.at(token.Assign) {
		p.advance()
		value = p.parseExpression()
	}

	return &ast.VarStmt{
		VarPos:  varTok.Pos,
		Name:    nameTok.Text,
		NamePos: nameTok.Pos,
		Type:    typ,
		Value:   value,
	}
}

func (p *Parser) parseAssign(target ast.Expression) ast.Statement {
	p.expect(token.Assign, "expected '=' in assignment")
	value := p.parseExpression()
	return &ast.AssignStmt{
		Target: target,
		Value:  value,
	}
}

func (p *Parser) parseIf() ast.Statement {
	ifTok := p.expect(token.If, "expected if")
	cond := p.parseExpression()
	then := p.parseBlock()

	var elseStmt ast.Statement
	if p.at(token.Else) {
		p.advance()
		if p.at(token.If) {
			elseStmt = p.parseIf()
		} else {
			elseStmt = p.parseBlock()
		}
	}

	return &ast.IfStmt{
		IfPos: ifTok.Pos,
		Cond:  cond,
		Then:  then,
		Else:  elseStmt,
	}
}

func (p *Parser) parseFor() ast.Statement {
	forTok := p.expect(token.For, "expected for")

	if p.at(token.Var) || (p.at(token.Ident) && p.peek().Kind == token.ColonAssign) {
		init := p.parseForClauseStmt(true)
		if p.at(token.Semicolon) {
			p.advance()
			cond := p.parseExpression()
			p.expect(token.Semicolon, "expected ';' after for condition")
			post := p.parseForClauseStmt(false)
			body := p.parseBlock()
			return &ast.ForStmt{
				ForPos: forTok.Pos,
				Init:   init,
				Cond:   cond,
				Post:   post,
				Body:   body,
			}
		}
		p.diag.Add(forTok.Pos, "for declarations and assignments require ';' clauses")
		body := p.parseBlock()
		return &ast.ForStmt{
			ForPos: forTok.Pos,
			Body:   body,
		}
	}

	first := p.parseExpression()
	if first != nil && p.at(token.Assign) {
		init := p.parseAssign(first)
		if p.at(token.Semicolon) {
			p.advance()
			cond := p.parseExpression()
			p.expect(token.Semicolon, "expected ';' after for condition")
			post := p.parseForClauseStmt(false)
			body := p.parseBlock()
			return &ast.ForStmt{
				ForPos: forTok.Pos,
				Init:   init,
				Cond:   cond,
				Post:   post,
				Body:   body,
			}
		}
		p.diag.Add(forTok.Pos, "for declarations and assignments require ';' clauses")
		body := p.parseBlock()
		return &ast.ForStmt{
			ForPos: forTok.Pos,
			Body:   body,
		}
	}

	cond := first
	if p.at(token.Semicolon) {
		p.diag.Add(forTok.Pos, "for init clause must be a declaration or assignment")
		for !p.at(token.LBrace) && !p.at(token.EOF) {
			p.advance()
		}
	}
	body := p.parseBlock()
	return &ast.ForStmt{
		ForPos: forTok.Pos,
		Cond:   cond,
		Body:   body,
	}
}

func (p *Parser) parseForClauseStmt(allowDecl bool) ast.Statement {
	switch p.current().Kind {
	case token.Var:
		if !allowDecl {
			p.errorCurrent("for post clause cannot declare a variable")
			return nil
		}
		return p.parseVarDecl()
	case token.Ident:
		if p.peek().Kind == token.ColonAssign {
			if !allowDecl {
				p.errorCurrent("for post clause cannot declare a variable")
				return nil
			}
			return p.parseShortDecl()
		}
		fallthrough
	default:
		expr := p.parseExpression()
		if expr == nil {
			return nil
		}
		if p.at(token.Assign) {
			return p.parseAssign(expr)
		}
		return &ast.ExprStmt{Expr: expr}
	}
}

func (p *Parser) parseBreak() ast.Statement {
	breakTok := p.expect(token.Break, "expected break")
	return &ast.BreakStmt{BreakPos: breakTok.Pos}
}

func (p *Parser) parseContinue() ast.Statement {
	continueTok := p.expect(token.Continue, "expected continue")
	return &ast.ContinueStmt{ContinuePos: continueTok.Pos}
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

func (p *Parser) parseMatch() ast.Statement {
	matchTok := p.expect(token.Match, "expected match")
	value := p.parseExpression()
	p.expect(token.LBrace, "expected '{' after match value")

	stmt := &ast.MatchStmt{
		MatchPos: matchTok.Pos,
		Value:    value,
	}
	for !p.at(token.RBrace) && !p.at(token.EOF) {
		stmt.Arms = append(stmt.Arms, p.parseMatchArm())
	}
	p.expect(token.RBrace, "expected '}' after match")
	return stmt
}

func (p *Parser) parseMatchArm() ast.MatchArm {
	caseTok := p.expect(token.Case, "expected case")
	enumType, caseName, casePos := p.parseMatchCasePattern()
	arm := ast.MatchArm{
		CasePos:     caseTok.Pos,
		EnumType:    enumType,
		CaseName:    caseName,
		CaseNamePos: casePos,
	}
	if p.at(token.LParen) {
		p.advance()
		bindTok := p.expect(token.Ident, "expected payload binding")
		arm.BindName = bindTok.Text
		arm.BindNamePos = bindTok.Pos
		arm.BindIgnore = bindTok.Text == "_"
		p.expect(token.RParen, "expected ')' after payload binding")
	}
	arm.Body = p.parseBlock()
	return arm
}

func (p *Parser) parseMatchCasePattern() (ast.TypeRef, string, token.Position) {
	nameTok := p.expect(token.Ident, "expected enum case")
	parts := []string{nameTok.Text}
	for p.at(token.Dot) {
		p.advance()
		partTok := p.expect(token.Ident, "expected identifier after '.'")
		parts = append(parts, partTok.Text)
		if len(parts) == 3 {
			break
		}
	}
	if len(parts) < 2 {
		p.diag.Add(nameTok.Pos, "match case must name Enum.Case")
		return ast.TypeRef{Name: nameTok.Text, Pos: nameTok.Pos}, "", nameTok.Pos
	}
	enumType := strings.Join(parts[:len(parts)-1], ".")
	caseName := parts[len(parts)-1]
	return ast.TypeRef{Name: enumType, Pos: nameTok.Pos}, caseName, nameTok.Pos
}

func (p *Parser) parseExpression() ast.Expression {
	return p.parseHandle()
}

func (p *Parser) parseHandle() ast.Expression {
	expr := p.parseLogicalOr()
	if !p.at(token.Or) {
		return expr
	}

	orTok := p.expect(token.Or, "expected or")
	p.expect(token.Pipe, "expected '|' after or")
	errTok := p.expect(token.Ident, "expected handler error name")
	p.expect(token.Pipe, "expected '|' after handler error name")
	handler := p.parseBlock()
	return &ast.HandleExpr{
		Inner:   expr,
		OrPos:   orTok.Pos,
		ErrName: errTok.Text,
		ErrPos:  errTok.Pos,
		Handler: handler,
	}
}

func (p *Parser) parseLogicalOr() ast.Expression {
	expr := p.parseLogicalAnd()
	for p.at(token.PipePipe) {
		op := p.current()
		p.advance()
		right := p.parseLogicalAnd()
		expr = &ast.BinaryExpr{
			Left:     expr,
			Operator: op.Kind,
			OpPos:    op.Pos,
			Right:    right,
		}
	}
	return expr
}

func (p *Parser) parseLogicalAnd() ast.Expression {
	expr := p.parseEquality()
	for p.at(token.AmpAmp) {
		op := p.current()
		p.advance()
		right := p.parseEquality()
		expr = &ast.BinaryExpr{
			Left:     expr,
			Operator: op.Kind,
			OpPos:    op.Pos,
			Right:    right,
		}
	}
	return expr
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
	expr := p.parseUnary()
	for p.at(token.Star) || p.at(token.Slash) || p.at(token.Percent) {
		op := p.current()
		p.advance()
		right := p.parseUnary()
		expr = &ast.BinaryExpr{
			Left:     expr,
			Operator: op.Kind,
			OpPos:    op.Pos,
			Right:    right,
		}
	}
	return expr
}

func (p *Parser) parseUnary() ast.Expression {
	if p.at(token.Bang) || p.at(token.Minus) || p.at(token.Star) || p.at(token.Amp) {
		op := p.current()
		p.advance()
		return &ast.UnaryExpr{
			Operator: op.Kind,
			OpPos:    op.Pos,
			Inner:    p.parseUnary(),
		}
	}
	return p.parsePostfix()
}

func (p *Parser) parsePostfix() ast.Expression {
	expr := p.parsePrimary()
	for {
		switch {
		case p.at(token.LParen):
			expr = p.finishCall(expr)
		case p.at(token.LBrace):
			typeRef, ok := typeRefFromExpression(expr)
			if !ok || !p.looksLikeStructLiteral() {
				return expr
			}
			expr = p.finishStructLiteral(typeRef)
		case p.at(token.Dot):
			dotTok := p.expect(token.Dot, "expected '.'")
			fieldTok := p.expect(token.Ident, "expected field name")
			expr = &ast.SelectorExpr{
				Inner:   expr,
				DotPos:  dotTok.Pos,
				Name:    fieldTok.Text,
				NamePos: fieldTok.Pos,
			}
		case p.at(token.LBracket):
			expr = p.finishIndexOrSlice(expr)
		case p.at(token.Question):
			questionTok := p.expect(token.Question, "expected '?'")
			expr = &ast.PropagateExpr{
				Inner:       expr,
				QuestionPos: questionTok.Pos,
			}
		default:
			return expr
		}
	}
}

func (p *Parser) parsePrimary() ast.Expression {
	tok := p.current()
	switch tok.Kind {
	case token.Ident:
		p.advance()
		if p.at(token.LBrace) && p.looksLikeStructLiteral() {
			return p.finishStructLiteral(ast.TypeRef{Name: tok.Text, Pos: tok.Pos})
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
	case token.Nil:
		p.advance()
		return &ast.NilLiteral{LitPos: tok.Pos}
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
	case token.LBracket:
		return p.parseSequenceLiteral()
	case token.Map:
		return p.parseMapLiteral()
	default:
		p.errorCurrent("expected expression")
		p.advance()
		return nil
	}
}

func (p *Parser) finishStructLiteral(typeRef ast.TypeRef) ast.Expression {
	lbrace := p.expect(token.LBrace, "expected '{'")
	expr := &ast.StructLiteralExpr{
		Type:   typeRef,
		LBrace: lbrace.Pos,
	}

	for !p.at(token.RBrace) && !p.at(token.EOF) {
		fieldTok := p.expect(token.Ident, "expected field name")
		p.expect(token.Colon, "expected ':' after field name")
		value := p.parseExpression()
		expr.Fields = append(expr.Fields, ast.StructLiteralField{
			Name:    fieldTok.Text,
			NamePos: fieldTok.Pos,
			Value:   value,
		})
		if !p.at(token.Comma) {
			break
		}
		p.advance()
		if p.at(token.RBrace) {
			break
		}
	}
	p.expect(token.RBrace, "expected '}' after struct literal")
	return expr
}

func (p *Parser) parseSequenceLiteral() ast.Expression {
	typeRef := p.parseTypeRef()
	lbrace := p.expect(token.LBrace, "expected '{' after array type")
	var expr ast.Expression
	if strings.HasPrefix(typeRef.Name, "[]") {
		expr = &ast.SliceLiteralExpr{Type: typeRef, LBrace: lbrace.Pos}
	} else {
		expr = &ast.ArrayLiteralExpr{Type: typeRef, LBrace: lbrace.Pos}
	}

	for !p.at(token.RBrace) && !p.at(token.EOF) {
		element := p.parseExpression()
		switch e := expr.(type) {
		case *ast.ArrayLiteralExpr:
			e.Elements = append(e.Elements, element)
		case *ast.SliceLiteralExpr:
			e.Elements = append(e.Elements, element)
		}
		if !p.at(token.Comma) {
			break
		}
		p.advance()
		if p.at(token.RBrace) {
			break
		}
	}
	p.expect(token.RBrace, "expected '}' after array literal")
	return expr
}

func (p *Parser) parseMapLiteral() ast.Expression {
	typeRef := p.parseTypeRef()
	lbrace := p.expect(token.LBrace, "expected '{' after map type")
	expr := &ast.MapLiteralExpr{Type: typeRef, LBrace: lbrace.Pos}

	for !p.at(token.RBrace) && !p.at(token.EOF) {
		keyPos := p.current().Pos
		key := p.parseExpression()
		p.expect(token.Colon, "expected ':' after map key")
		valuePos := p.current().Pos
		value := p.parseExpression()
		expr.Pairs = append(expr.Pairs, ast.MapLiteralPair{
			Key:      key,
			KeyPos:   keyPos,
			Value:    value,
			ValuePos: valuePos,
		})
		if !p.at(token.Comma) {
			break
		}
		p.advance()
		if p.at(token.RBrace) {
			break
		}
	}
	p.expect(token.RBrace, "expected '}' after map literal")
	return expr
}

func (p *Parser) finishIndexOrSlice(expr ast.Expression) ast.Expression {
	lbracket := p.expect(token.LBracket, "expected '['")
	start := p.parseExpression()
	if p.at(token.Colon) {
		colon := p.expect(token.Colon, "expected ':'")
		end := p.parseExpression()
		p.expect(token.RBracket, "expected ']'")
		return &ast.SliceExpr{
			Inner:       expr,
			LBracketPos: lbracket.Pos,
			Start:       start,
			ColonPos:    colon.Pos,
			End:         end,
		}
	}
	p.expect(token.RBracket, "expected ']'")
	return &ast.IndexExpr{
		Inner:       expr,
		LBracketPos: lbracket.Pos,
		Index:       start,
	}
}

func (p *Parser) finishCall(callee ast.Expression) ast.Expression {
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
		Callee: callee,
		Args:   args,
	}
}

func typeRefFromExpression(expr ast.Expression) (ast.TypeRef, bool) {
	switch e := expr.(type) {
	case *ast.IdentExpr:
		return ast.TypeRef{Name: e.Name, Pos: e.NamePos}, true
	case *ast.SelectorExpr:
		inner, ok := typeRefFromExpression(e.Inner)
		if !ok {
			return ast.TypeRef{}, false
		}
		return ast.TypeRef{Name: inner.Name + "." + e.Name, Pos: inner.Pos}, true
	default:
		return ast.TypeRef{}, false
	}
}

func (p *Parser) looksLikeStructLiteral() bool {
	if !p.at(token.LBrace) {
		return false
	}
	if p.index+1 >= len(p.tokens) {
		return false
	}
	next := p.tokens[p.index+1]
	if next.Kind == token.RBrace {
		return true
	}
	if next.Kind != token.Ident {
		return false
	}
	if p.index+2 >= len(p.tokens) {
		return false
	}
	return p.tokens[p.index+2].Kind == token.Colon
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

func (p *Parser) errorCurrent(message string) {
	p.diag.Add(p.current().Pos, "%s", message)
}

func describeToken(tok token.Token) string {
	if tok.Kind == token.Ident || tok.Kind == token.Int || tok.Kind == token.String {
		return fmt.Sprintf("%s %q", tok.Kind, tok.Text)
	}
	return tok.Kind.String()
}
