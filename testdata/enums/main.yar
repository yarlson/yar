package main

enum TokenKind {
	Ident
	Int
}

enum Expr {
	Int { value i32 }
	Name { text str }
}

fn print_kind(kind TokenKind) void {
	match kind {
	case TokenKind.Ident {
		print("ident")
		print("\n")
	}
	case TokenKind.Int {
		print("int")
		print("\n")
	}
	}
}

fn print_expr(expr Expr) void {
	match expr {
	case Expr.Int(v) {
		print("int")
		print("\n")
		print(to_str(v.value))
		print("\n")
	}
	case Expr.Name(v) {
		print("name")
		print("\n")
		print(v.text)
		print("\n")
	}
	}
}

fn main() i32 {
	expr := Expr.Name{text: "main"}
	print_expr(expr)
	print_kind(TokenKind.Ident)
	return 0
}
