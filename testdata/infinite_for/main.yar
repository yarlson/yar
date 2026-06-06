package main

fn main() i32 {
	var i i32 = 0
	for {
		i += 1
		if i == 3 {
			break
		}
	}
	print(to_str(i) + "\n")
	return 0
}
