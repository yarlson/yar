package main

fn main() i32 {
	values := []i32{}
	values = append(values, 1)
	values = append(values, 2)
	values = append(values, 3)

	middle := values[1:2]
	middle[0] = 9

	print_int(len(values))
	print("\n")
	print_int(values[1])
	print("\n")
	print_int(len(middle))
	print("\n")

	grown := append(middle, 4)
	print_int(len(grown))
	print("\n")
	print_int(values[2])
	print("\n")
	print_int(grown[1])
	print("\n")
	return 0
}
