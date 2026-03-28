package main

fn divide(a i32, b i32) !i32 {
	if b == 0 {
		return error.DivideByZero
	}
	return a / b
}

fn main() !i32 {
	x := divide(10, 2)?
	print_int(x)
	print("\n")
	return 0
}
