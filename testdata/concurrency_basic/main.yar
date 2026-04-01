package main

fn square(v i32) i32 {
	return v * v
}

fn main() i32 {
	values := taskgroup []i32 {
		spawn square(2)
		spawn square(3)
	}
	print(to_str(values[0]) + "\n")
	print(to_str(values[1]) + "\n")
	return 0
}
