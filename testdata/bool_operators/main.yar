package main

fn trace_true(label str) bool {
	print(label)
	return true
}

fn trace_false(label str) bool {
	print(label)
	return false
}

fn main() i32 {
	if trace_false("and-left\n") && trace_true("and-right\n") {
		print("bad-and\n")
	}

	if trace_true("or-left\n") || trace_true("or-right\n") {
		print("done\n")
	}

	return 0
}
