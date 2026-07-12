use crate::{
    ast::*,
    diag::{Diagnostic, List},
    lexer::{Lexer, parse_char_literal, parse_int_literal},
    token::{Kind, Position, Token, compound_assign_op},
};

pub fn parse(src: &str) -> (Program, Vec<Diagnostic>) {
    parse_file("", src)
}

pub fn parse_file(path: &str, src: &str) -> (Program, Vec<Diagnostic>) {
    let mut lexer = Lexer::new_file(src, path);
    let tokens = lexer.lex();
    let lex_diagnostics = lexer.diagnostics();

    let mut parser = Parser {
        tokens,
        index: 0,
        diag: List::default(),
    };
    let program = parser.parse_program();

    let mut diagnostics = Vec::new();
    diagnostics.extend(lex_diagnostics);
    diagnostics.extend(parser.diag.items());
    (program, diagnostics)
}

#[derive(Clone)]
struct Parser {
    tokens: Vec<Token>,
    index: usize,
    diag: List,
}

impl Parser {
    fn parse_program(&mut self) -> Program {
        let package_tok = self.expect(Kind::Package, "expected package declaration");
        let name_tok = self.expect(Kind::Ident, "expected package name");

        let mut program = Program {
            package_pos: package_tok.pos,
            package_name: name_tok.text,
            ..Program::default()
        };

        while self.at(Kind::Import) {
            program.imports.push(self.parse_import());
        }

        while !self.at(Kind::Eof) {
            let exported = if self.at(Kind::Pub) {
                self.advance();
                true
            } else {
                false
            };

            match self.current().kind {
                Kind::Struct => program.structs.push(self.parse_struct(exported)),
                Kind::Interface => program.interfaces.push(self.parse_interface(exported)),
                Kind::Enum => program.enums.push(self.parse_enum(exported)),
                Kind::Fn => program.functions.push(self.parse_function(exported)),
                _ => {
                    self.error_current("expected function, struct, interface, or enum declaration");
                    self.advance();
                }
            }
        }

        program
    }

    fn parse_import(&mut self) -> ImportDecl {
        let import_tok = self.expect(Kind::Import, "expected import");
        let path_tok = self.expect(Kind::String, "expected import path string");
        ImportDecl {
            import_pos: import_tok.pos,
            path: path_tok.text,
            path_pos: path_tok.pos,
        }
    }

    fn parse_struct(&mut self, exported: bool) -> StructDecl {
        let struct_tok = self.expect(Kind::Struct, "expected struct");
        let name_tok = self.expect(Kind::Ident, "expected struct name");
        let type_params = self.parse_optional_type_param_list();
        self.expect(Kind::LBrace, "expected '{' after struct name");

        let mut decl = StructDecl {
            struct_pos: struct_tok.pos,
            exported,
            resource: false,
            name: name_tok.text,
            name_pos: name_tok.pos,
            type_params,
            fields: Vec::new(),
        };
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            let exported = if self.at(Kind::Pub) {
                self.advance();
                true
            } else {
                false
            };
            let field_name = self.expect(Kind::Ident, "expected field name");
            let field_type = self.parse_type_ref();
            decl.fields.push(StructField {
                exported,
                name: field_name.text,
                name_pos: field_name.pos,
                type_ref: field_type,
            });
        }
        self.expect(Kind::RBrace, "expected '}' after struct body");
        decl
    }

    fn parse_interface(&mut self, exported: bool) -> InterfaceDecl {
        let interface_tok = self.expect(Kind::Interface, "expected interface");
        let name_tok = self.expect(Kind::Ident, "expected interface name");
        self.expect(Kind::LBrace, "expected '{' after interface name");

        let mut decl = InterfaceDecl {
            interface_pos: interface_tok.pos,
            exported,
            name: name_tok.text,
            name_pos: name_tok.pos,
            methods: Vec::new(),
        };
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            let method_name = self.expect(Kind::Ident, "expected interface method name");
            self.expect(Kind::LParen, "expected '(' after interface method name");
            let params = self.parse_param_list();
            let (return_is_bang, return_type) = self.parse_return_type();
            decl.methods.push(InterfaceMethodDecl {
                name: method_name.text,
                name_pos: method_name.pos,
                params,
                return_type,
                return_is_bang,
            });
        }
        self.expect(Kind::RBrace, "expected '}' after interface body");
        decl
    }

    fn parse_enum(&mut self, exported: bool) -> EnumDecl {
        let enum_tok = self.expect(Kind::Enum, "expected enum");
        let name_tok = self.expect(Kind::Ident, "expected enum name");
        self.expect(Kind::LBrace, "expected '{' after enum name");

        let mut decl = EnumDecl {
            enum_pos: enum_tok.pos,
            exported,
            name: name_tok.text,
            name_pos: name_tok.pos,
            cases: Vec::new(),
        };
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            let case_name = self.expect(Kind::Ident, "expected enum case name");
            let mut enum_case = EnumCaseDecl {
                name: case_name.text,
                name_pos: case_name.pos,
                fields: Vec::new(),
            };
            if self.at(Kind::LBrace) {
                self.advance();
                while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
                    if self.at(Kind::Pub) {
                        self.error_current(
                            "enum payload fields are inherently public and do not accept 'pub'",
                        );
                        self.advance();
                    }
                    let field_name = self.expect(Kind::Ident, "expected payload field name");
                    let field_type = self.parse_type_ref();
                    enum_case.fields.push(StructField {
                        exported: true,
                        name: field_name.text,
                        name_pos: field_name.pos,
                        type_ref: field_type,
                    });
                }
                self.expect(Kind::RBrace, "expected '}' after enum payload");
            }
            decl.cases.push(enum_case);
        }
        self.expect(Kind::RBrace, "expected '}' after enum body");
        decl
    }

    fn parse_function(&mut self, exported: bool) -> FunctionDecl {
        self.expect(Kind::Fn, "expected fn");
        let receiver = if self.at(Kind::LParen) {
            self.advance();
            let recv_name = self.expect(Kind::Ident, "expected receiver name");
            let recv_type = self.parse_type_ref();
            self.expect(Kind::RParen, "expected ')' after receiver");
            Some(ReceiverDecl {
                name: recv_name.text,
                name_pos: recv_name.pos,
                type_ref: recv_type,
            })
        } else {
            None
        };
        let name_tok = self.expect(Kind::Ident, "expected function name");
        let type_params = self.parse_optional_type_param_list();
        self.expect(Kind::LParen, "expected '(' after function name");
        let params = self.parse_param_list();
        let (return_is_bang, return_type) = self.parse_return_type();
        let body = self.parse_block();

        FunctionDecl {
            exported,
            host_intrinsic: false,
            name: name_tok.text,
            name_pos: name_tok.pos,
            type_params,
            receiver,
            params,
            return_type,
            return_is_bang,
            body,
        }
    }

    fn parse_type_ref(&mut self) -> TypeRef {
        if self.at(Kind::Bang) {
            let bang = self.expect(Kind::Bang, "expected '!'");
            let inner = self.parse_type_ref();
            return TypeRef {
                kind: TypeRefKind::Errorable,
                elem: Some(Box::new(inner)),
                pos: bang.pos,
                ..TypeRef::named("", Position::default())
            };
        }

        if self.at(Kind::Star) {
            let star = self.expect(Kind::Star, "expected '*'");
            let inner = self.parse_type_ref();
            return TypeRef {
                kind: TypeRefKind::Pointer,
                elem: Some(Box::new(inner)),
                pos: star.pos,
                ..TypeRef::named("", Position::default())
            };
        }

        if self.at(Kind::LBracket) {
            let lbracket = self.expect(Kind::LBracket, "expected '['");
            if self.at(Kind::RBracket) {
                self.advance();
                let elem = self.parse_type_ref();
                return TypeRef {
                    kind: TypeRefKind::Slice,
                    elem: Some(Box::new(elem)),
                    pos: lbracket.pos,
                    ..TypeRef::named("", Position::default())
                };
            }
            let length_tok = self.expect(Kind::Int, "expected array length");
            self.expect(Kind::RBracket, "expected ']' after array length");
            let elem = self.parse_type_ref();
            let length = match parse_int_literal(&length_tok) {
                Ok(value) => value,
                Err(_) => {
                    self.diag.add(
                        length_tok.pos,
                        format!("invalid integer literal {:?}", length_tok.text),
                    );
                    0
                }
            };
            return TypeRef {
                kind: TypeRefKind::Array,
                array_len: length,
                elem: Some(Box::new(elem)),
                pos: lbracket.pos,
                ..TypeRef::named("", Position::default())
            };
        }

        if self.at(Kind::Map) {
            let map_tok = self.expect(Kind::Map, "expected map");
            self.expect(Kind::LBracket, "expected '[' after map");
            let key_type = self.parse_type_ref();
            self.expect(Kind::RBracket, "expected ']' after map key type");
            let value_type = self.parse_type_ref();
            return TypeRef {
                kind: TypeRefKind::Map,
                key: Some(Box::new(key_type)),
                value: Some(Box::new(value_type)),
                pos: map_tok.pos,
                ..TypeRef::named("", Position::default())
            };
        }

        if self.at(Kind::Chan) {
            let chan_tok = self.expect(Kind::Chan, "expected chan");
            self.expect(Kind::LBracket, "expected '[' after chan");
            let elem_type = self.parse_type_ref();
            self.expect(Kind::RBracket, "expected ']' after chan element type");
            return TypeRef {
                kind: TypeRefKind::Chan,
                elem: Some(Box::new(elem_type)),
                pos: chan_tok.pos,
                ..TypeRef::named("", Position::default())
            };
        }

        if self.at(Kind::Fn) {
            let fn_tok = self.expect(Kind::Fn, "expected fn");
            self.expect(Kind::LParen, "expected '(' after fn");
            let mut params = Vec::new();
            while !self.at(Kind::RParen) && !self.at(Kind::Eof) {
                params.push(self.parse_type_ref());
                if !self.at(Kind::Comma) {
                    break;
                }
                self.advance();
            }
            self.expect(Kind::RParen, "expected ')' after function type parameters");
            let (return_is_bang, return_type) = self.parse_return_type();
            return TypeRef {
                kind: TypeRefKind::Function,
                params,
                return_type: Some(Box::new(return_type)),
                errorable: return_is_bang,
                pos: fn_tok.pos,
                ..TypeRef::named("", Position::default())
            };
        }

        let tok = self.current();
        if tok.kind != Kind::Ident && tok.kind != Kind::Error {
            let tok = self.expect(Kind::Ident, "expected type name");
            return TypeRef::named(tok.text, tok.pos);
        }
        self.advance();
        let mut name = tok.text;
        if tok.kind == Kind::Ident {
            while self.at(Kind::Dot) {
                self.advance();
                let part_tok = self.expect(Kind::Ident, "expected type name after '.'");
                name.push('.');
                name.push_str(&part_tok.text);
            }
        }
        let mut type_ref = TypeRef::named(name, tok.pos);
        type_ref.type_args = self.parse_optional_type_arg_list();
        type_ref
    }

    fn parse_block(&mut self) -> BlockStmt {
        let lbrace = self.expect(Kind::LBrace, "expected '{'");
        let mut block = BlockStmt {
            lbrace: lbrace.pos,
            stmts: Vec::new(),
        };
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            if let Some(stmt) = self.parse_statement() {
                block.stmts.push(stmt);
            }
        }
        self.expect(Kind::RBrace, "expected '}'");
        block
    }

    fn parse_statement(&mut self) -> Option<Statement> {
        match self.current().kind {
            Kind::Let => {
                self.error_current("use ':=' for local declarations; 'let' is no longer supported");
                self.advance();
                None
            }
            Kind::Var => Some(self.parse_var_decl()),
            Kind::If => Some(self.parse_if()),
            Kind::For => Some(self.parse_for()),
            Kind::Break => Some(self.parse_break()),
            Kind::Continue => Some(self.parse_continue()),
            Kind::Return => Some(self.parse_return()),
            Kind::Match => Some(self.parse_match()),
            Kind::Spawn => Some(self.parse_spawn()),
            Kind::LBrace => Some(Statement::Block(Box::new(self.parse_block()))),
            Kind::Ident if self.peek().kind == Kind::ColonAssign => Some(self.parse_short_decl()),
            _ => {
                let expr = self.parse_expression();
                if self.is_assign_op() {
                    Some(self.parse_assign(expr))
                } else {
                    Some(Statement::Expr(Box::new(ExprStmt { expr })))
                }
            }
        }
    }

    fn parse_short_decl(&mut self) -> Statement {
        let name_tok = self.expect(Kind::Ident, "expected local name");
        self.expect(Kind::ColonAssign, "expected ':=' in declaration");
        let value = self.parse_expression();
        Statement::Let(Box::new(LetStmt {
            let_pos: name_tok.pos.clone(),
            name: name_tok.text,
            name_pos: name_tok.pos,
            value,
        }))
    }

    fn parse_var_decl(&mut self) -> Statement {
        let var_tok = self.expect(Kind::Var, "expected var");
        let name_tok = self.expect(Kind::Ident, "expected local name");
        let type_ref = self.parse_type_ref();
        let value = if self.at(Kind::Assign) {
            self.advance();
            Some(self.parse_expression())
        } else {
            None
        };
        Statement::Var(Box::new(VarStmt {
            var_pos: var_tok.pos,
            name: name_tok.text,
            name_pos: name_tok.pos,
            type_ref,
            value,
        }))
    }

    fn is_assign_op(&self) -> bool {
        self.current().kind == Kind::Assign || compound_assign_op(self.current().kind).is_some()
    }

    fn parse_assign(&mut self, target: Expression) -> Statement {
        let tok = self.current();
        if let Some(bin_op) = compound_assign_op(tok.kind) {
            self.advance();
            let value = self.parse_expression();
            return Statement::CompoundAssign(Box::new(CompoundAssignStmt {
                target,
                operator: bin_op,
                op_pos: tok.pos,
                value,
            }));
        }
        self.expect(Kind::Assign, "expected '=' in assignment");
        let value = self.parse_expression();
        Statement::Assign(Box::new(AssignStmt { target, value }))
    }

    fn parse_if(&mut self) -> Statement {
        let if_tok = self.expect(Kind::If, "expected if");
        let cond = self.parse_expression();
        let then_block = self.parse_block();
        let else_stmt = if self.at(Kind::Else) {
            self.advance();
            if self.at(Kind::If) {
                Some(self.parse_if())
            } else {
                Some(Statement::Block(Box::new(self.parse_block())))
            }
        } else {
            None
        };
        Statement::If(Box::new(IfStmt {
            if_pos: if_tok.pos,
            cond,
            then_block,
            else_stmt,
        }))
    }

    fn parse_for(&mut self) -> Statement {
        let for_tok = self.expect(Kind::For, "expected for");
        if self.at(Kind::LBrace) {
            return Statement::For(Box::new(ForStmt {
                for_pos: for_tok.pos,
                init: None,
                cond: None,
                post: None,
                body: self.parse_block(),
            }));
        }

        if self.at(Kind::Var) || (self.at(Kind::Ident) && self.peek().kind == Kind::ColonAssign) {
            let init = self.parse_for_clause_stmt(true);
            if self.at(Kind::Semicolon) {
                self.advance();
                let cond = self.parse_expression();
                self.expect(Kind::Semicolon, "expected ';' after for condition");
                let post = self.parse_for_clause_stmt(false);
                let body = self.parse_block();
                return Statement::For(Box::new(ForStmt {
                    for_pos: for_tok.pos,
                    init,
                    cond: Some(cond),
                    post,
                    body,
                }));
            }
            self.diag.add(
                for_tok.pos.clone(),
                "for declarations and assignments require ';' clauses",
            );
            return Statement::For(Box::new(ForStmt {
                for_pos: for_tok.pos,
                init: None,
                cond: None,
                post: None,
                body: self.parse_block(),
            }));
        }

        let first = self.parse_expression();
        if self.is_assign_op() {
            let init = Some(self.parse_assign(first));
            if self.at(Kind::Semicolon) {
                self.advance();
                let cond = self.parse_expression();
                self.expect(Kind::Semicolon, "expected ';' after for condition");
                let post = self.parse_for_clause_stmt(false);
                let body = self.parse_block();
                return Statement::For(Box::new(ForStmt {
                    for_pos: for_tok.pos,
                    init,
                    cond: Some(cond),
                    post,
                    body,
                }));
            }
            self.diag.add(
                for_tok.pos.clone(),
                "for declarations and assignments require ';' clauses",
            );
            return Statement::For(Box::new(ForStmt {
                for_pos: for_tok.pos,
                init: None,
                cond: None,
                post: None,
                body: self.parse_block(),
            }));
        }

        if self.at(Kind::Semicolon) {
            self.diag.add(
                for_tok.pos.clone(),
                "for init clause must be a declaration or assignment",
            );
            while !self.at(Kind::LBrace) && !self.at(Kind::Eof) {
                self.advance();
            }
        }
        Statement::For(Box::new(ForStmt {
            for_pos: for_tok.pos,
            init: None,
            cond: Some(first),
            post: None,
            body: self.parse_block(),
        }))
    }

    fn parse_for_clause_stmt(&mut self, allow_decl: bool) -> Option<Statement> {
        match self.current().kind {
            Kind::Var => {
                if !allow_decl {
                    self.error_current("for post clause cannot declare a variable");
                    return None;
                }
                Some(self.parse_var_decl())
            }
            Kind::Ident if self.peek().kind == Kind::ColonAssign => {
                if !allow_decl {
                    self.error_current("for post clause cannot declare a variable");
                    return None;
                }
                Some(self.parse_short_decl())
            }
            _ => {
                let expr = self.parse_expression();
                if self.is_assign_op() {
                    Some(self.parse_assign(expr))
                } else {
                    Some(Statement::Expr(Box::new(ExprStmt { expr })))
                }
            }
        }
    }

    fn parse_break(&mut self) -> Statement {
        let break_tok = self.expect(Kind::Break, "expected break");
        Statement::Break(BreakStmt {
            break_pos: break_tok.pos,
        })
    }

    fn parse_continue(&mut self) -> Statement {
        let continue_tok = self.expect(Kind::Continue, "expected continue");
        Statement::Continue(ContinueStmt {
            continue_pos: continue_tok.pos,
        })
    }

    fn parse_return(&mut self) -> Statement {
        let return_tok = self.expect(Kind::Return, "expected return");
        let value = if self.at(Kind::RBrace) || self.at(Kind::Eof) {
            None
        } else {
            Some(self.parse_expression())
        };
        Statement::Return(Box::new(ReturnStmt {
            return_pos: return_tok.pos,
            value,
        }))
    }

    fn parse_spawn(&mut self) -> Statement {
        let spawn_tok = self.expect(Kind::Spawn, "expected spawn");
        let call = self.parse_expression();
        Statement::Spawn(Box::new(SpawnStmt {
            spawn_pos: spawn_tok.pos,
            call,
        }))
    }

    fn parse_match(&mut self) -> Statement {
        let match_tok = self.expect(Kind::Match, "expected match");
        let value = self.parse_expression();
        self.expect(Kind::LBrace, "expected '{' after match value");
        let mut stmt = MatchStmt {
            match_pos: match_tok.pos,
            value,
            arms: Vec::new(),
            else_body: None,
        };
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) && !self.at(Kind::Else) {
            stmt.arms.push(self.parse_match_arm());
        }
        if self.at(Kind::Else) {
            self.advance();
            stmt.else_body = Some(self.parse_block());
        }
        self.expect(Kind::RBrace, "expected '}' after match");
        Statement::Match(Box::new(stmt))
    }

    fn parse_match_arm(&mut self) -> MatchArm {
        let case_tok = self.expect(Kind::Case, "expected case");
        let (enum_type, case_name, case_pos) = self.parse_match_case_pattern();
        let mut arm = MatchArm {
            case_pos: case_tok.pos,
            enum_type,
            case_name,
            case_name_pos: case_pos,
            bind_name: String::new(),
            bind_name_pos: Position::default(),
            bind_ignore: false,
            body: BlockStmt {
                lbrace: Position::default(),
                stmts: Vec::new(),
            },
        };
        if self.at(Kind::LParen) {
            self.advance();
            let bind_tok = self.expect(Kind::Ident, "expected payload binding");
            arm.bind_ignore = bind_tok.text == "_";
            arm.bind_name = bind_tok.text;
            arm.bind_name_pos = bind_tok.pos;
            self.expect(Kind::RParen, "expected ')' after payload binding");
        }
        arm.body = self.parse_block();
        arm
    }

    fn parse_match_case_pattern(&mut self) -> (TypeRef, String, Position) {
        let name_tok = self.expect(Kind::Ident, "expected enum case");
        let mut parts = vec![name_tok.text.clone()];
        while self.at(Kind::Dot) {
            self.advance();
            let part_tok = self.expect(Kind::Ident, "expected identifier after '.'");
            parts.push(part_tok.text);
            if parts.len() == 3 {
                break;
            }
        }
        if parts.len() < 2 {
            self.diag
                .add(name_tok.pos.clone(), "match case must name Enum.Case");
            return (
                TypeRef::named(name_tok.text, name_tok.pos.clone()),
                String::new(),
                name_tok.pos,
            );
        }
        let case_name = parts.pop().unwrap_or_default();
        let enum_type = parts.join(".");
        (
            TypeRef::named(enum_type, name_tok.pos.clone()),
            case_name,
            name_tok.pos,
        )
    }

    fn parse_expression(&mut self) -> Expression {
        self.parse_handle()
    }

    fn parse_handle(&mut self) -> Expression {
        let expr = self.parse_logical_or();
        if !self.at(Kind::Or) {
            return expr;
        }
        let or_tok = self.expect(Kind::Or, "expected or");
        self.expect(Kind::Pipe, "expected '|' after or");
        let err_tok = self.expect(Kind::Ident, "expected handler error name");
        self.expect(Kind::Pipe, "expected '|' after handler error name");
        let handler = self.parse_block();
        Expression::Handle(Box::new(HandleExpr {
            inner: expr,
            or_pos: or_tok.pos,
            err_name: err_tok.text,
            err_pos: err_tok.pos,
            handler,
        }))
    }

    fn parse_logical_or(&mut self) -> Expression {
        let mut expr = self.parse_logical_and();
        while self.at(Kind::PipePipe) {
            let op = self.current();
            self.advance();
            let right = self.parse_logical_and();
            expr = Expression::Binary(Box::new(BinaryExpr {
                left: expr,
                operator: op.kind,
                op_pos: op.pos,
                right,
            }));
        }
        expr
    }

    fn parse_logical_and(&mut self) -> Expression {
        let mut expr = self.parse_equality();
        while self.at(Kind::AmpAmp) {
            let op = self.current();
            self.advance();
            let right = self.parse_equality();
            expr = Expression::Binary(Box::new(BinaryExpr {
                left: expr,
                operator: op.kind,
                op_pos: op.pos,
                right,
            }));
        }
        expr
    }

    fn parse_equality(&mut self) -> Expression {
        let mut expr = self.parse_comparison();
        while self.at(Kind::EqualEqual) || self.at(Kind::BangEqual) {
            let op = self.current();
            self.advance();
            let right = self.parse_comparison();
            expr = Expression::Binary(Box::new(BinaryExpr {
                left: expr,
                operator: op.kind,
                op_pos: op.pos,
                right,
            }));
        }
        expr
    }

    fn parse_comparison(&mut self) -> Expression {
        let mut expr = self.parse_additive();
        while self.at(Kind::Less)
            || self.at(Kind::LessEqual)
            || self.at(Kind::Greater)
            || self.at(Kind::GreaterEqual)
        {
            let op = self.current();
            self.advance();
            let right = self.parse_additive();
            expr = Expression::Binary(Box::new(BinaryExpr {
                left: expr,
                operator: op.kind,
                op_pos: op.pos,
                right,
            }));
        }
        expr
    }

    fn parse_additive(&mut self) -> Expression {
        let mut expr = self.parse_multiplicative();
        while self.at(Kind::Plus) || self.at(Kind::Minus) {
            let op = self.current();
            self.advance();
            let right = self.parse_multiplicative();
            expr = Expression::Binary(Box::new(BinaryExpr {
                left: expr,
                operator: op.kind,
                op_pos: op.pos,
                right,
            }));
        }
        expr
    }

    fn parse_multiplicative(&mut self) -> Expression {
        let mut expr = self.parse_unary();
        while self.at(Kind::Star) || self.at(Kind::Slash) || self.at(Kind::Percent) {
            let op = self.current();
            self.advance();
            let right = self.parse_unary();
            expr = Expression::Binary(Box::new(BinaryExpr {
                left: expr,
                operator: op.kind,
                op_pos: op.pos,
                right,
            }));
        }
        expr
    }

    fn parse_unary(&mut self) -> Expression {
        if self.at(Kind::Bang) || self.at(Kind::Minus) || self.at(Kind::Star) || self.at(Kind::Amp)
        {
            let op = self.current();
            self.advance();
            return Expression::Unary(Box::new(UnaryExpr {
                operator: op.kind,
                op_pos: op.pos,
                inner: self.parse_unary(),
            }));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Expression {
        let mut expr = self.parse_primary();
        loop {
            if self.at(Kind::LParen) {
                expr = self.finish_call(expr);
                continue;
            }
            if self.at(Kind::LBrace) {
                let Some(type_ref) = type_ref_from_expression(&expr) else {
                    return expr;
                };
                if !self.looks_like_struct_literal() {
                    return expr;
                }
                expr = self.finish_struct_literal(type_ref);
                continue;
            }
            if self.at(Kind::Dot) {
                let dot_tok = self.expect(Kind::Dot, "expected '.'");
                let field_tok = self.expect(Kind::Ident, "expected field name");
                expr = Expression::Selector(Box::new(SelectorExpr {
                    inner: expr,
                    dot_pos: dot_tok.pos,
                    name: field_tok.text,
                    name_pos: field_tok.pos,
                }));
                continue;
            }
            if self.at(Kind::LBracket) {
                let lbracket_pos = self.current().pos;
                if let Some(type_args) = self.try_parse_type_application() {
                    expr = Expression::TypeApplication(Box::new(TypeApplicationExpr {
                        inner: expr,
                        lbracket_pos,
                        type_args,
                    }));
                    continue;
                }
                expr = self.finish_index_or_slice(expr);
                continue;
            }
            if self.at(Kind::Question) {
                let question_tok = self.expect(Kind::Question, "expected '?'");
                expr = Expression::Propagate(Box::new(PropagateExpr {
                    inner: expr,
                    question_pos: question_tok.pos,
                }));
                continue;
            }
            return expr;
        }
    }

    fn parse_primary(&mut self) -> Expression {
        let tok = self.current();
        match tok.kind {
            Kind::Ident => {
                self.advance();
                if self.at(Kind::LBrace) && self.looks_like_struct_literal() {
                    return self.finish_struct_literal(TypeRef::named(tok.text, tok.pos));
                }
                Expression::Ident(Box::new(IdentExpr {
                    name: tok.text,
                    name_pos: tok.pos,
                }))
            }
            Kind::Int => {
                self.advance();
                let value = match parse_int_literal(&tok) {
                    Ok(value) => value,
                    Err(_) => {
                        self.diag.add(
                            tok.pos.clone(),
                            format!("invalid integer literal {:?}", tok.text),
                        );
                        0
                    }
                };
                Expression::Int(IntLiteral {
                    value,
                    lit_pos: tok.pos,
                })
            }
            Kind::Char => {
                self.advance();
                Expression::Char(CharLiteral {
                    value: parse_char_literal(&tok),
                    lit_pos: tok.pos,
                })
            }
            Kind::String => {
                self.advance();
                Expression::String(StringLiteral {
                    value: tok.text,
                    lit_pos: tok.pos,
                })
            }
            Kind::True => {
                self.advance();
                Expression::Bool(BoolLiteral {
                    value: true,
                    lit_pos: tok.pos,
                })
            }
            Kind::False => {
                self.advance();
                Expression::Bool(BoolLiteral {
                    value: false,
                    lit_pos: tok.pos,
                })
            }
            Kind::Nil => {
                self.advance();
                Expression::Nil(NilLiteral { lit_pos: tok.pos })
            }
            Kind::Error => {
                self.advance();
                self.expect(Kind::Dot, "expected '.' after error");
                let name_tok = self.expect(Kind::Ident, "expected error name");
                Expression::Error(ErrorLiteral {
                    name: name_tok.text,
                    err_pos: tok.pos,
                })
            }
            Kind::LParen => {
                self.advance();
                let inner = self.parse_expression();
                self.expect(Kind::RParen, "expected ')'");
                Expression::Group(Box::new(GroupExpr { inner }))
            }
            Kind::Fn => self.parse_function_literal(),
            Kind::Taskgroup => self.parse_taskgroup_expr(),
            Kind::LBracket => self.parse_sequence_literal(),
            Kind::Map => self.parse_map_literal(),
            _ => {
                let pos = tok.pos;
                self.error_current("expected expression");
                self.advance();
                Expression::Missing(pos)
            }
        }
    }

    fn parse_taskgroup_expr(&mut self) -> Expression {
        let taskgroup_tok = self.expect(Kind::Taskgroup, "expected taskgroup");
        let result_type = self.parse_type_ref();
        let body = self.parse_block();
        Expression::Taskgroup(Box::new(TaskgroupExpr {
            taskgroup_pos: taskgroup_tok.pos,
            result_type,
            body,
        }))
    }

    fn parse_function_literal(&mut self) -> Expression {
        let fn_tok = self.expect(Kind::Fn, "expected fn");
        self.expect(Kind::LParen, "expected '(' after fn");
        let params = self.parse_param_list();
        let (return_is_bang, return_type) = self.parse_return_type();
        let body = self.parse_block();
        Expression::FunctionLiteral(Box::new(FunctionLiteralExpr {
            fn_pos: fn_tok.pos,
            enclosing_function: String::new(),
            params,
            return_type,
            return_is_bang,
            body,
        }))
    }

    fn finish_struct_literal(&mut self, type_ref: TypeRef) -> Expression {
        let lbrace = self.expect(Kind::LBrace, "expected '{'");
        let mut expr = StructLiteralExpr {
            type_ref,
            lbrace: lbrace.pos,
            fields: Vec::new(),
        };
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            let field_tok = self.expect(Kind::Ident, "expected field name");
            self.expect(Kind::Colon, "expected ':' after field name");
            let value = self.parse_expression();
            expr.fields.push(StructLiteralField {
                name: field_tok.text,
                name_pos: field_tok.pos,
                value,
            });
            if !self.at(Kind::Comma) {
                break;
            }
            self.advance();
            if self.at(Kind::RBrace) {
                break;
            }
        }
        self.expect(Kind::RBrace, "expected '}' after struct literal");
        Expression::StructLiteral(Box::new(expr))
    }

    fn parse_sequence_literal(&mut self) -> Expression {
        let type_ref = self.parse_type_ref();
        let lbrace = self.expect(Kind::LBrace, "expected '{' after array type");
        let mut elements = Vec::new();
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            elements.push(self.parse_expression());
            if !self.at(Kind::Comma) {
                break;
            }
            self.advance();
            if self.at(Kind::RBrace) {
                break;
            }
        }
        self.expect(Kind::RBrace, "expected '}' after array literal");
        if type_ref.kind == TypeRefKind::Slice {
            Expression::SliceLiteral(Box::new(SliceLiteralExpr {
                type_ref,
                lbrace: lbrace.pos,
                elements,
            }))
        } else {
            Expression::ArrayLiteral(Box::new(ArrayLiteralExpr {
                type_ref,
                lbrace: lbrace.pos,
                elements,
            }))
        }
    }

    fn parse_map_literal(&mut self) -> Expression {
        let type_ref = self.parse_type_ref();
        let lbrace = self.expect(Kind::LBrace, "expected '{' after map type");
        let mut expr = MapLiteralExpr {
            type_ref,
            lbrace: lbrace.pos,
            pairs: Vec::new(),
        };
        while !self.at(Kind::RBrace) && !self.at(Kind::Eof) {
            let key_pos = self.current().pos;
            let key = self.parse_expression();
            self.expect(Kind::Colon, "expected ':' after map key");
            let value_pos = self.current().pos;
            let value = self.parse_expression();
            expr.pairs.push(MapLiteralPair {
                key,
                key_pos,
                value,
                value_pos,
            });
            if !self.at(Kind::Comma) {
                break;
            }
            self.advance();
            if self.at(Kind::RBrace) {
                break;
            }
        }
        self.expect(Kind::RBrace, "expected '}' after map literal");
        Expression::MapLiteral(Box::new(expr))
    }

    fn finish_index_or_slice(&mut self, expr: Expression) -> Expression {
        let lbracket = self.expect(Kind::LBracket, "expected '['");
        if self.at(Kind::Colon) {
            let colon = self.expect(Kind::Colon, "expected ':'");
            let end = if self.at(Kind::RBracket) {
                None
            } else {
                Some(self.parse_expression())
            };
            self.expect(Kind::RBracket, "expected ']'");
            return Expression::Slice(Box::new(SliceExpr {
                inner: expr,
                lbracket_pos: lbracket.pos,
                start: None,
                colon_pos: colon.pos,
                end,
            }));
        }
        let start = self.parse_expression();
        if self.at(Kind::Colon) {
            let colon = self.expect(Kind::Colon, "expected ':'");
            let end = if self.at(Kind::RBracket) {
                None
            } else {
                Some(self.parse_expression())
            };
            self.expect(Kind::RBracket, "expected ']'");
            return Expression::Slice(Box::new(SliceExpr {
                inner: expr,
                lbracket_pos: lbracket.pos,
                start: Some(start),
                colon_pos: colon.pos,
                end,
            }));
        }
        self.expect(Kind::RBracket, "expected ']'");
        Expression::Index(Box::new(IndexExpr {
            inner: expr,
            lbracket_pos: lbracket.pos,
            index: start,
        }))
    }

    fn finish_call(&mut self, callee: Expression) -> Expression {
        self.expect(Kind::LParen, "expected '('");
        let mut args = Vec::new();
        while !self.at(Kind::RParen) && !self.at(Kind::Eof) {
            args.push(self.parse_expression());
            if !self.at(Kind::Comma) {
                break;
            }
            self.advance();
        }
        self.expect(Kind::RParen, "expected ')'");
        Expression::Call(Box::new(CallExpr { callee, args }))
    }

    fn parse_param_list(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        while !self.at(Kind::RParen) && !self.at(Kind::Eof) {
            let param_name = self.expect(Kind::Ident, "expected parameter name");
            let param_type = self.parse_type_ref();
            params.push(Param {
                name: param_name.text,
                name_pos: param_name.pos,
                type_ref: param_type,
            });
            if !self.at(Kind::Comma) {
                break;
            }
            self.advance();
        }
        self.expect(Kind::RParen, "expected ')' after parameters");
        params
    }

    fn parse_return_type(&mut self) -> (bool, TypeRef) {
        let return_is_bang = if self.at(Kind::Bang) {
            self.advance();
            true
        } else {
            false
        };
        (return_is_bang, self.parse_type_ref())
    }

    fn parse_optional_type_param_list(&mut self) -> Vec<TypeParam> {
        if !self.at(Kind::LBracket) {
            return Vec::new();
        }
        let lbracket = self.expect(Kind::LBracket, "expected '['");
        let mut params = Vec::new();
        while !self.at(Kind::RBracket) && !self.at(Kind::Eof) {
            let name_tok = self.expect(Kind::Ident, "expected type parameter name");
            params.push(TypeParam {
                name: name_tok.text,
                pos: name_tok.pos,
            });
            if !self.at(Kind::Comma) {
                break;
            }
            self.advance();
        }
        self.expect(Kind::RBracket, "expected ']' after type parameter list");
        if params.is_empty() {
            self.diag
                .add(lbracket.pos, "type parameter list cannot be empty");
        }
        params
    }

    fn parse_optional_type_arg_list(&mut self) -> Vec<TypeRef> {
        if !self.at(Kind::LBracket) {
            return Vec::new();
        }
        self.expect(Kind::LBracket, "expected '['");
        let mut args = Vec::new();
        while !self.at(Kind::RBracket) && !self.at(Kind::Eof) {
            args.push(self.parse_type_ref());
            if !self.at(Kind::Comma) {
                break;
            }
            self.advance();
        }
        self.expect(Kind::RBracket, "expected ']' after type argument list");
        args
    }

    fn try_parse_type_application(&mut self) -> Option<Vec<TypeRef>> {
        if !self.at(Kind::LBracket) {
            return None;
        }
        let mut temp = self.clone();
        temp.diag = List::default();
        temp.expect(Kind::LBracket, "expected '['");
        let mut args = Vec::new();
        while !temp.at(Kind::RBracket) && !temp.at(Kind::Eof) {
            args.push(temp.parse_type_ref());
            if !temp.at(Kind::Comma) {
                break;
            }
            temp.advance();
        }
        temp.expect(Kind::RBracket, "expected ']' after type argument list");
        if !temp.diag.is_empty() {
            return None;
        }
        if !temp.at(Kind::LParen) && (!temp.at(Kind::LBrace) || !temp.looks_like_struct_literal()) {
            return None;
        }
        self.index = temp.index;
        Some(args)
    }

    fn looks_like_struct_literal(&self) -> bool {
        if !self.at(Kind::LBrace) {
            return false;
        }
        let Some(next) = self.tokens.get(self.index + 1) else {
            return false;
        };
        if next.kind == Kind::RBrace {
            return true;
        }
        if next.kind != Kind::Ident {
            return false;
        }
        self.tokens
            .get(self.index + 2)
            .is_some_and(|token| token.kind == Kind::Colon)
    }

    fn current(&self) -> Token {
        self.tokens.get(self.index).cloned().unwrap_or(Token {
            kind: Kind::Eof,
            text: String::new(),
            pos: Position::default(),
        })
    }

    fn peek(&self) -> Token {
        self.tokens.get(self.index + 1).cloned().unwrap_or(Token {
            kind: Kind::Eof,
            text: String::new(),
            pos: Position::default(),
        })
    }

    fn at(&self, kind: Kind) -> bool {
        self.current().kind == kind
    }

    fn advance(&mut self) {
        if self.index < self.tokens.len() {
            self.index += 1;
        }
    }

    fn expect(&mut self, kind: Kind, message: &str) -> Token {
        let tok = self.current();
        if tok.kind != kind {
            self.diag.add(
                tok.pos.clone(),
                format!("{message}, got {}", describe_token(&tok)),
            );
            return Token {
                kind,
                text: String::new(),
                pos: tok.pos,
            };
        }
        self.advance();
        tok
    }

    fn error_current(&mut self, message: &str) {
        self.diag.add(self.current().pos, message);
    }
}

fn type_ref_from_expression(expr: &Expression) -> Option<TypeRef> {
    match expr {
        Expression::Ident(expr) => Some(TypeRef::named(expr.name.clone(), expr.name_pos.clone())),
        Expression::Selector(expr) => {
            let mut inner = type_ref_from_expression(&expr.inner)?;
            inner.name.push('.');
            inner.name.push_str(&expr.name);
            Some(inner)
        }
        Expression::TypeApplication(expr) => {
            let mut inner = type_ref_from_expression(&expr.inner)?;
            inner.type_args = expr.type_args.clone();
            Some(inner)
        }
        _ => None,
    }
}

fn describe_token(tok: &Token) -> String {
    if tok.kind == Kind::Ident || tok.kind == Kind::Int || tok.kind == Kind::String {
        return format!("{} {:?}", tok.kind, tok.text);
    }
    tok.kind.to_string()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use super::*;

    #[test]
    fn parses_generics_closures_enums_and_error_sugar() {
        let (program, diagnostics) = parse(
            r#"
package main

struct Box[T] { pub value T private i32 }
enum Expr { Int { value i64 } Name { text str } }

fn make_adder(x i32) fn(i32) !i32 {
    return fn(y i32) !i32 {
        return x + y
    }
}

fn main() !i32 {
    box := Box[i32]{value: 1}
    expr := Expr.Name{text: "main"}
    match expr {
    case Expr.Int(_) { return 1 }
    case Expr.Name(v) { print(v.text) }
    }
    return make_adder(box.value)(2)?
}
"#,
        );

        assert_eq!(diagnostics, Vec::new());
        assert_eq!(program.package_name, "main");
        assert_eq!(program.structs[0].type_params[0].name, "T");
        assert!(program.structs[0].fields[0].exported);
        assert!(!program.structs[0].fields[1].exported);
        assert_eq!(program.enums[0].cases.len(), 2);
        assert_eq!(program.functions.len(), 2);
        assert_eq!(program.functions[0].return_type.to_string(), "fn(i32) !i32");
    }

    #[test]
    fn rejects_pub_on_enum_payload_fields_without_stalling() {
        let (program, diagnostics) = parse(
            r#"package main

enum Value {
    Number { pub value i32 }
}
"#,
        );

        assert_eq!(program.enums[0].cases[0].fields[0].name, "value");
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.as_str())
                .collect::<Vec<_>>(),
            vec!["enum payload fields are inherently public and do not accept 'pub'"]
        );
    }

    #[test]
    fn parses_pointer_field_assignment_targets() {
        let (program, diagnostics) = parse(
            r#"
package main

struct Node {
    value i32
    next *Node
}

fn set_value(node *Node, value i32) void {
    (*node).value = value
}

fn main() i32 {
    tail := &Node{value: 2, next: nil}
    return 0
}
"#,
        );

        assert_eq!(diagnostics, Vec::new());

        let Statement::Assign(assign) = &program.functions[0].body.stmts[0] else {
            panic!("expected assignment statement");
        };
        let Expression::Selector(target) = &assign.target else {
            panic!("expected selector assignment target");
        };
        let Expression::Group(group) = &target.inner else {
            panic!("expected grouped dereference base");
        };
        let Expression::Unary(deref) = &group.inner else {
            panic!("expected unary dereference");
        };
        assert_eq!(deref.operator, Kind::Star);

        let Statement::Let(stmt) = &program.functions[1].body.stmts[0] else {
            panic!("expected local declaration");
        };
        let Expression::Unary(addr) = &stmt.value else {
            panic!("expected unary address-of expression");
        };
        assert_eq!(addr.operator, Kind::Amp);
        assert!(matches!(addr.inner, Expression::StructLiteral(_)));
    }

    #[test]
    fn parses_pointer_field_assignments_in_for_clauses() {
        let (program, diagnostics) = parse(
            r#"
package main

struct Node {
    value i32
}

fn main() i32 {
    node := &Node{value: 0}
    for (*node).value = 0; (*node).value < 1; (*node).value = (*node).value + 1 {
    }
    return 0
}
"#,
        );

        assert_eq!(diagnostics, Vec::new());

        let Statement::For(loop_) = &program.functions[0].body.stmts[1] else {
            panic!("expected for statement");
        };
        assert!(matches!(loop_.init, Some(Statement::Assign(_))));
        assert!(matches!(loop_.post, Some(Statement::Assign(_))));
    }

    #[test]
    fn preserves_compound_assignment_as_its_own_statement() {
        let (program, diagnostics) = parse(
            r#"
package main

fn main() i32 {
    values[index] += amount
    return 0
}
"#,
        );

        assert_eq!(diagnostics, Vec::new());
        let Statement::CompoundAssign(statement) = &program.functions[0].body.stmts[0] else {
            panic!("expected compound assignment statement");
        };
        assert_eq!(statement.operator, Kind::Plus);
        assert!(matches!(statement.target, Expression::Index(_)));
        assert!(matches!(statement.value, Expression::Ident(_)));
    }

    #[test]
    fn parses_current_yar_corpus_without_diagnostics() {
        let root = repo_root();
        let mut files = Vec::new();
        collect_yar_files(&root.join("stdlib/packages"), &mut files);
        collect_yar_files(&root.join("testdata"), &mut files);
        files.sort();

        assert!(!files.is_empty(), "expected checked-in Yar fixtures");

        let mut failures = Vec::new();
        for path in files {
            let src = fs::read_to_string(&path).unwrap_or_else(|err| {
                panic!("read {}: {err}", path.display());
            });
            let rel = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy();
            let (program, diagnostics) = parse_file(rel.as_ref(), &src);
            if !diagnostics.is_empty() {
                failures.push(format!("{}: {:?}", path.display(), diagnostics));
                continue;
            }
            if program.package_name.is_empty() {
                failures.push(format!("{}: empty package name", path.display()));
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate is nested under crates/yar-compiler")
            .to_path_buf()
    }

    fn collect_yar_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        {
            let path = entry
                .unwrap_or_else(|err| panic!("read entry in {}: {err}", dir.display()))
                .path();
            if path.is_dir() {
                collect_yar_files(&path, out);
                continue;
            }
            if path.extension().is_some_and(|ext| ext == "yar") {
                out.push(path);
            }
        }
    }
}
