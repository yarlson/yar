package main

struct Node {
	value i32
	next *Node
}

fn set_value(node *Node, value i32) void {
	(*node).value = value
}

fn main() i32 {
	tail := &Node{value: 2, next: nil}
	head := &Node{value: 1, next: tail}

	set_value(head, 3)

	if (*head).next == nil {
		return 1
	}

	next := (*head).next
	print_int((*head).value)
	print("\n")
	print_int((*next).value)
	print("\n")
	return 0
}
