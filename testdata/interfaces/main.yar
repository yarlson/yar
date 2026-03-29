package main

import "lib"

fn main() !i32 {
	print(lib.greet(lib.User{name: "ada"}))
	print("\n")

	counter := lib.make_counter(1)
	print_int(lib.add_twice(counter, 2))
	print("\n")
	print_int(counter.inc(3))
	print("\n")
	return 0
}
