package main

fn make_adder(base i32) fn(i32) i32 {
	return fn(delta i32) i32 {
		return base + delta
	}
}

fn apply_twice(f fn(i32) i32, value i32) i32 {
	return f(f(value))
}

fn main() i32 {
	base := 4
	add := make_adder(base)
	print(to_str(apply_twice(add, 5)))
	print("\n")

	offset := 7
	inc := fn(value i32) i32 {
		return value + 1
	}
	plus_offset := fn(value i32) i32 {
		return value + offset
	}
	print(to_str(plus_offset(inc(1))))
	print("\n")
	return 0
}
