package people

pub struct User {
	name str
}

pub fn (u User) label() str {
	return u.name
}

pub fn (u *User) rename(name str) void {
	(*u).name = name
}
