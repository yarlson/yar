package main

fn main() i32 {
	values := []i32{}
	values = append(values, 1)
	values = append(values, 2)
	values = append(values, 3)

	middle := values[1:2]
	middle[0] = 9

	print(to_str(len(values)))
	print("\n")
	print(to_str(values[1]))
	print("\n")
	print(to_str(len(middle)))
	print("\n")

	grown := append(middle, 4)
	print(to_str(len(grown)))
	print("\n")
	print(to_str(values[2]))
	print("\n")
	print(to_str(grown[1]))
	print("\n")
	return 0
}
