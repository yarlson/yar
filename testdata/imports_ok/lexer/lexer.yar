package lexer

import "token"

pub fn classify(kind token.Kind) bool {
	return kind.ident
}
