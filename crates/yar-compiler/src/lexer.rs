use crate::{
    diag::{Diagnostic, List},
    token::{Kind, Position, Token},
};

pub struct Lexer<'a> {
    src: &'a str,
    file: String,
    offset: usize,
    line: usize,
    column: usize,
    diag: List,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self::new_file(src, "")
    }

    pub fn new_file(src: &'a str, file: impl Into<String>) -> Self {
        Self {
            src,
            file: file.into(),
            offset: 0,
            line: 1,
            column: 1,
            diag: List::default(),
        }
    }

    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diag.items()
    }

    pub fn lex(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            self.skip_trivia();
            let pos = self.position();
            let Some(r) = self.peek_char() else {
                tokens.push(Token {
                    kind: Kind::Eof,
                    text: String::new(),
                    pos,
                });
                return tokens;
            };

            if is_ident_start(r) {
                let text = self.lex_ident();
                tokens.push(Token {
                    kind: lookup_keyword(&text),
                    text,
                    pos,
                });
                continue;
            }
            if r.is_ascii_digit() {
                tokens.push(Token {
                    kind: Kind::Int,
                    text: self.lex_digits(),
                    pos,
                });
                continue;
            }

            self.advance_char(r);
            match r {
                ':' => {
                    if self.match_char('=') {
                        tokens.push(self.fixed(Kind::ColonAssign, ":=", pos));
                    } else {
                        tokens.push(self.fixed(Kind::Colon, ":", pos));
                    }
                }
                ';' => tokens.push(self.fixed(Kind::Semicolon, ";", pos)),
                '=' => {
                    if self.match_equals() {
                        tokens.push(self.fixed(Kind::EqualEqual, "==", pos));
                    } else {
                        tokens.push(self.fixed(Kind::Assign, "=", pos));
                    }
                }
                '!' => {
                    if self.match_equals() {
                        tokens.push(self.fixed(Kind::BangEqual, "!=", pos));
                    } else {
                        tokens.push(self.fixed(Kind::Bang, "!", pos));
                    }
                }
                '?' => tokens.push(self.fixed(Kind::Question, "?", pos)),
                ',' => tokens.push(self.fixed(Kind::Comma, ",", pos)),
                '.' => tokens.push(self.fixed(Kind::Dot, ".", pos)),
                '&' => {
                    if self.match_char('&') {
                        tokens.push(self.fixed(Kind::AmpAmp, "&&", pos));
                    } else {
                        tokens.push(self.fixed(Kind::Amp, "&", pos));
                    }
                }
                '(' => tokens.push(self.fixed(Kind::LParen, "(", pos)),
                ')' => tokens.push(self.fixed(Kind::RParen, ")", pos)),
                '{' => tokens.push(self.fixed(Kind::LBrace, "{", pos)),
                '}' => tokens.push(self.fixed(Kind::RBrace, "}", pos)),
                '[' => tokens.push(self.fixed(Kind::LBracket, "[", pos)),
                ']' => tokens.push(self.fixed(Kind::RBracket, "]", pos)),
                '|' => {
                    if self.match_char('|') {
                        tokens.push(self.fixed(Kind::PipePipe, "||", pos));
                    } else {
                        tokens.push(self.fixed(Kind::Pipe, "|", pos));
                    }
                }
                '+' => {
                    if self.match_equals() {
                        tokens.push(self.fixed(Kind::PlusAssign, "+=", pos));
                    } else {
                        tokens.push(self.fixed(Kind::Plus, "+", pos));
                    }
                }
                '-' => {
                    if self.match_equals() {
                        tokens.push(self.fixed(Kind::MinusAssign, "-=", pos));
                    } else {
                        tokens.push(self.fixed(Kind::Minus, "-", pos));
                    }
                }
                '*' => {
                    if self.match_equals() {
                        tokens.push(self.fixed(Kind::StarAssign, "*=", pos));
                    } else {
                        tokens.push(self.fixed(Kind::Star, "*", pos));
                    }
                }
                '/' => {
                    if self.match_equals() {
                        tokens.push(self.fixed(Kind::SlashAssign, "/=", pos));
                    } else {
                        tokens.push(self.fixed(Kind::Slash, "/", pos));
                    }
                }
                '%' => {
                    if self.match_equals() {
                        tokens.push(self.fixed(Kind::PercentAssign, "%=", pos));
                    } else {
                        tokens.push(self.fixed(Kind::Percent, "%", pos));
                    }
                }
                '<' => {
                    if self.match_equals() {
                        tokens.push(self.fixed(Kind::LessEqual, "<=", pos));
                    } else {
                        tokens.push(self.fixed(Kind::Less, "<", pos));
                    }
                }
                '>' => {
                    if self.match_equals() {
                        tokens.push(self.fixed(Kind::GreaterEqual, ">=", pos));
                    } else {
                        tokens.push(self.fixed(Kind::Greater, ">", pos));
                    }
                }
                '"' => {
                    let value = self.lex_string(&pos);
                    tokens.push(Token {
                        kind: Kind::String,
                        text: value,
                        pos,
                    });
                }
                '\'' => {
                    let value = self.lex_char(&pos);
                    tokens.push(Token {
                        kind: Kind::Char,
                        text: value.to_string(),
                        pos,
                    });
                }
                _ => {
                    self.diag
                        .add(pos.clone(), format!("unexpected character {:?}", r));
                    tokens.push(Token {
                        kind: Kind::Illegal,
                        text: r.to_string(),
                        pos,
                    });
                }
            }
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            let Some(r) = self.peek_char() else {
                return;
            };
            if r == '/' && self.peek_next_char() == Some('/') {
                self.advance_char('/');
                self.advance_char('/');
                while let Some(comment_char) = self.peek_char() {
                    self.advance_char(comment_char);
                    if comment_char == '\n' {
                        break;
                    }
                }
                continue;
            }
            if !r.is_whitespace() {
                return;
            }
            self.advance_char(r);
        }
    }

    fn lex_ident(&mut self) -> String {
        let start = self.offset;
        while let Some(r) = self.peek_char() {
            if !is_ident_part(r) {
                break;
            }
            self.advance_char(r);
        }
        self.src[start..self.offset].to_owned()
    }

    fn lex_digits(&mut self) -> String {
        let start = self.offset;
        while let Some(r) = self.peek_char() {
            if !r.is_ascii_digit() {
                break;
            }
            self.advance_char(r);
        }
        self.src[start..self.offset].to_owned()
    }

    fn lex_string(&mut self, pos: &Position) -> String {
        let mut out = String::new();
        while let Some(r) = self.peek_char() {
            self.advance_char(r);
            match r {
                '"' => return out,
                '\\' => {
                    let Some(esc) = self.peek_char() else {
                        self.diag.add(pos.clone(), "unterminated string literal");
                        return out;
                    };
                    self.advance_char(esc);
                    match esc {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        '\\' => out.push('\\'),
                        '"' => out.push('"'),
                        'r' => out.push('\r'),
                        '0' => out.push('\0'),
                        _ => self.diag.add(
                            pos.clone(),
                            format!("unsupported string escape {:?}", format!("\\{esc}")),
                        ),
                    }
                }
                '\n' => {
                    self.diag.add(pos.clone(), "unterminated string literal");
                    return out;
                }
                _ => out.push(r),
            }
        }
        self.diag.add(pos.clone(), "unterminated string literal");
        out
    }

    fn lex_char(&mut self, pos: &Position) -> char {
        let Some(r) = self.peek_char() else {
            self.diag.add(pos.clone(), "unterminated character literal");
            return '\0';
        };
        self.advance_char(r);

        let value = match r {
            '\'' => {
                self.diag.add(pos.clone(), "empty character literal");
                return '\0';
            }
            '\n' => {
                self.diag.add(pos.clone(), "unterminated character literal");
                return '\0';
            }
            '\\' => {
                let Some(esc) = self.peek_char() else {
                    self.diag.add(pos.clone(), "unterminated character literal");
                    return '\0';
                };
                self.advance_char(esc);
                match esc {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '\\' => '\\',
                    '\'' => '\'',
                    '0' => '\0',
                    _ => {
                        self.diag.add(
                            pos.clone(),
                            format!("unsupported character escape {:?}", format!("\\{esc}")),
                        );
                        esc
                    }
                }
            }
            _ => r,
        };

        if self.peek_char() != Some('\'') {
            self.diag.add(pos.clone(), "unterminated character literal");
            return value;
        }
        self.advance_char('\'');
        value
    }

    fn match_equals(&mut self) -> bool {
        self.match_char('=')
    }

    fn match_char(&mut self, want: char) -> bool {
        if self.peek_char() != Some(want) {
            return false;
        }
        self.advance_char(want);
        true
    }

    fn peek_char(&self) -> Option<char> {
        self.src[self.offset..].chars().next()
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.src[self.offset..].chars();
        chars.next()?;
        chars.next()
    }

    fn advance_char(&mut self, r: char) {
        self.offset += r.len_utf8();
        if r == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
    }

    fn position(&self) -> Position {
        Position {
            file: self.file.clone(),
            line: self.line,
            column: self.column,
        }
    }

    fn fixed(&self, kind: Kind, text: &str, pos: Position) -> Token {
        Token {
            kind,
            text: text.to_owned(),
            pos,
        }
    }
}

pub fn parse_char_literal(tok: &Token) -> char {
    tok.text.chars().next().unwrap_or('\0')
}

pub fn parse_int_literal(tok: &Token) -> Result<i64, std::num::ParseIntError> {
    tok.text.parse()
}

fn is_ident_start(r: char) -> bool {
    r == '_' || r.is_alphabetic()
}

fn is_ident_part(r: char) -> bool {
    is_ident_start(r) || r.is_ascii_digit()
}

fn lookup_keyword(text: &str) -> Kind {
    match text {
        "package" => Kind::Package,
        "import" => Kind::Import,
        "fn" => Kind::Fn,
        "pub" => Kind::Pub,
        "let" => Kind::Let,
        "var" => Kind::Var,
        "struct" => Kind::Struct,
        "interface" => Kind::Interface,
        "enum" => Kind::Enum,
        "or" => Kind::Or,
        "if" => Kind::If,
        "else" => Kind::Else,
        "for" => Kind::For,
        "break" => Kind::Break,
        "continue" => Kind::Continue,
        "return" => Kind::Return,
        "match" => Kind::Match,
        "case" => Kind::Case,
        "taskgroup" => Kind::Taskgroup,
        "spawn" => Kind::Spawn,
        "true" => Kind::True,
        "false" => Kind::False,
        "nil" => Kind::Nil,
        "error" => Kind::Error,
        "map" => Kind::Map,
        "chan" => Kind::Chan,
        _ => Kind::Ident,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use super::*;

    #[test]
    fn lexes_basic_tokens() {
        let mut lexer = Lexer::new("package main\nfn main() i32 { return 42 }\n");
        let tokens = lexer.lex();

        assert_eq!(lexer.diagnostics(), Vec::new());
        assert_eq!(tokens[0].kind, Kind::Package);
        assert_eq!(tokens[1].kind, Kind::Ident);
        assert_eq!(tokens[1].text, "main");
        assert_eq!(tokens[3].kind, Kind::Ident);
        assert_eq!(tokens[9].kind, Kind::Int);
        assert_eq!(tokens.last().map(|token| token.kind), Some(Kind::Eof));
    }

    #[test]
    fn decodes_string_and_char_escapes() {
        let mut lexer = Lexer::new("\"a\\n\\t\\r\\0\\\\\\\"\" '\\n' '\\0'");
        let tokens = lexer.lex();

        assert_eq!(lexer.diagnostics(), Vec::new());
        assert_eq!(tokens[0].kind, Kind::String);
        assert_eq!(tokens[0].text, "a\n\t\r\0\\\"");
        assert_eq!(tokens[1].kind, Kind::Char);
        assert_eq!(parse_char_literal(&tokens[1]), '\n');
        assert_eq!(tokens[2].kind, Kind::Char);
        assert_eq!(parse_char_literal(&tokens[2]), '\0');
    }

    #[test]
    fn lexes_current_yar_corpus_without_diagnostics() {
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
            let mut lexer = Lexer::new_file(&src, rel.as_ref());
            let tokens = lexer.lex();
            let diagnostics = lexer.diagnostics();
            if !diagnostics.is_empty() {
                failures.push(format!("{}: {:?}", path.display(), diagnostics));
                continue;
            }
            if tokens.iter().any(|token| token.kind == Kind::Illegal) {
                failures.push(format!("{}: illegal token produced", path.display()));
                continue;
            }
            if tokens.last().map(|token| token.kind) != Some(Kind::Eof) {
                failures.push(format!("{}: missing EOF token", path.display()));
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
