package main

import "lexer"

error NotIdent

fn main() !i32 {
	if check(lexer.classify(lexer.default_kind())) {
		print("ok\n")
		return 0
	}
	return error.NotIdent
}
