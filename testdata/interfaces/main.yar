package main

import "lib"

fn main() !i32 {
	print(lib.greet(lib.User{name: "ada"}))
	print("\n")

	counter := lib.make_counter(1)
	print(to_str(lib.add_twice(counter, 2)))
	print("\n")
	print(to_str(counter.inc(3)))
	print("\n")
	return 0
}
