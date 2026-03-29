package lib

pub interface Labeler {
	label(prefix str) str
}

pub interface Counter {
	inc(delta i32) i32
}

pub struct User {
	name str
}

struct counterBox {
	value i32
}

fn (u User) label(prefix str) str {
	return prefix + u.name
}

fn (c *counterBox) inc(delta i32) i32 {
	(*c).value = (*c).value + delta
	return (*c).value
}

pub fn greet(v Labeler) str {
	return v.label("hi ")
}

pub fn make_counter(start i32) Counter {
	return &counterBox{value: start}
}

pub fn add_twice(c Counter, delta i32) i32 {
	c.inc(delta)
	return c.inc(delta)
}
