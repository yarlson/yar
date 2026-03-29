package main

fn churn_once(seed i32) i32 {
	values := []i32{}
	for i := 0; i < 512; i = i + 1 {
		values = append(values, seed + i)
	}
	return values[0] + values[511]
}

fn main() i32 {
	total := 0
	for round := 0; round < 200; round = round + 1 {
		total = total + churn_once(round)
	}
	print(to_str(total))
	print("\n")
	return 0
}
