package main

fn add(value i32, delta i32) i32 {
	return value + delta
}

fn exercise(seed i32) i32 {
	values := taskgroup []i32 {
		spawn add(seed, 1)
		spawn add(seed, 2)
	}

	ch := chan_new[str](2)
	first := "first-" + to_str(seed)
	second := "second-" + to_str(seed)
	chan_send(ch, first) or |_| {
		return -1
	}
	chan_send(ch, second) or |_| {
		return -1
	}
	received := chan_recv(ch) or |_| {
		return -1
	}
	if received != first {
		return -1
	}
	return values[0] + values[1]
}

fn main() i32 {
	total := 0
	for seed := 0; seed < 500; seed = seed + 1 {
		value := exercise(seed)
		if value < 0 {
			return 1
		}
		total = total + value
	}
	print(to_str(total))
	print("\n")
	return 0
}
