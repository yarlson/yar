package main

struct Leaf {
	name str
	code i32
}

struct Safe {
	leaves [2]Leaf
}

struct Node {
	value i32
	next chan[Node]
}

enum Message {
	Ready { value Safe }
	Failed { reason error }
}

struct Box[T] {
	value T
}

fn inspect(box Box[Message], ready chan[Safe]) i32 {
	return 3
}

fn inspect_node(node Node) i32 {
	return node.value
}

fn build_items() []i32 {
	return []i32{4, 5}
}

fn generic_code[T](value T, code i32) i32 {
	results := taskgroup []i32 {
		spawn fn() i32 {
			value
			return code
		}()
	}
	return results[0]
}

fn main() i32 {
	safe := Safe{
		leaves: [2]Leaf{
			Leaf{name: "a", code: 1},
			Leaf{name: "b", code: 2},
		},
	}
	box := Box[Message]{value: Message.Ready{value: safe}}
	ready := chan_new[Safe](1)
	next := chan_new[Node](1)
	node := Node{value: 7, next: next}
	offset := 4
	first_code := generic_code[i32](1, 11)
	second_code := generic_code[str]("safe", 12)

	numbers := taskgroup []i32 {
		spawn inspect(box, ready)
		spawn inspect_node(node)
		spawn fn(value i32) i32 {
			add_offset := fn() i32 {
				return value + offset
			}
			return add_offset()
		}(5)
	}

	items := taskgroup [][]i32 {
		spawn build_items()
	}

	chan_close(ready)
	chan_close(next)
	if numbers[0] + numbers[1] + numbers[2] != 19 ||
		items[0][1] != 5 ||
		first_code + second_code != 23 {
		return 1
	}
	return 0
}
