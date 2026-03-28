package token

pub struct Kind {
	ident bool
}

pub fn ident_kind() Kind {
	return Kind{ident: true}
}

fn hidden_kind() Kind {
	return Kind{ident: false}
}
