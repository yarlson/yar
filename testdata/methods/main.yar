package main

import "people"

struct Counter {
	value i32
}

fn (c Counter) current() i32 {
	return c.value
}

fn (c *Counter) inc(delta i32) void {
	(*c).value = (*c).value + delta
}

fn main() i32 {
	counter := &Counter{value: 2}
	counter.inc(3)
	print_int((*counter).current())
	print("\n")

	user := people.User{name: "ada"}
	print(user.label())
	print("\n")

	user_ptr := &user
	user_ptr.rename("eve")
	print((*user_ptr).label())
	print("\n")
	return 0
}
