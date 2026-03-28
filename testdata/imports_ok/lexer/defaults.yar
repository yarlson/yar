package lexer

import "token"

pub fn default_kind() token.Kind {
	return token.ident_kind()
}
