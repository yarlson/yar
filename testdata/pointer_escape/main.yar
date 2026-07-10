package main

struct Record {
	value i32
}

fn local_pointer() *i32 {
	value := 41
	return &value
}

fn parameter_pointer(value i32) *i32 {
	return &value
}

fn field_pointer() *i32 {
	record := Record{value: 43}
	return &record.value
}

fn element_pointer() *i32 {
	values := [2]i32{44, 45}
	return &values[1]
}

fn closure_pointer() *i32 {
	make_pointer := fn() *i32 {
		value := 46
		return &value
	}
	return make_pointer()
}

fn fail() !i32 {
	return error.Boom
}

fn error_pointer() *error {
	value := fail() or |err| {
		return &err
	}
	fallback := error.Unexpected
	if value == 0 {
		return &fallback
	}
	return &fallback
}

fn clobber(seed i32) i32 {
	values := [8]i32{1, 2, 3, 4, 5, 6, 7, 8}
	return values[seed]
}

fn main() i32 {
	local := local_pointer()
	parameter := parameter_pointer(42)
	field := field_pointer()
	element := element_pointer()
	closure := closure_pointer()
	err := error_pointer()

	ignored := clobber(0) + clobber(1) + clobber(2)
	if ignored != 6 {
		return 1
	}
	if *local != 41 {
		return 2
	}
	if *parameter != 42 {
		return 3
	}
	if *field != 43 {
		return 4
	}
	if *element != 45 {
		return 5
	}
	if *closure != 46 {
		return 6
	}
	if *err != error.Boom {
		return 7
	}

	print("pointer escape ok\n")
	return 0
}
