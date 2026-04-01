package main

fn maybe(v i32) !i32 {
	if v == 0 {
		return error.Zero
	}
	return v
}

fn main() i32 {
	values := taskgroup []!i32 {
		spawn maybe(1)
		spawn maybe(0)
	}

	first := values[0] or |err| {
		return 1
	}
	print(to_str(first) + "\n")

	second := values[1] or |err| {
		print(to_str(err) + "\n")
		return 0
	}
	print(to_str(second) + "\n")
	return 0
}
