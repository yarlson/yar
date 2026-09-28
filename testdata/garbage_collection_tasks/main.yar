package main

struct Node {
	value i32
	label str
	next *Node
}

fn build(seed i32, n i32) *Node {
	var head *Node = nil
	for i := 0; i < n; i = i + 1 {
		head = &Node{value: i + seed, label: to_str(i) + "-" + to_str(seed), next: head}
	}
	return head
}

fn checksum(list *Node) i32 {
	total := 0
	node := list
	for node != nil {
		total = total + node.value + len(node.label)
		node = node.next
	}
	return total
}

fn produce(id i32, out chan[str], rounds i32) i32 {
	total := 0
	for round := 0; round < rounds; round = round + 1 {
		list := build(id, 200)
		index := map[str]i32{}
		node := list
		for node != nil {
			index[node.label] = node.value
			node = node.next
		}
		parts := []str{}
		for k := 0; k < 20; k = k + 1 {
			parts = append(parts, to_str(k * id))
		}
		shift := fn(value i32) i32 {
			return value + id
		}
		total = total + shift(checksum(list)) + len(index) + len(parts[19])
		chan_send(out, "p" + to_str(id) + ":" + to_str(round)) or |err| {
			return total
		}
	}
	return total
}

fn consume(input chan[str], expected i32) i32 {
	received := 0
	bytes := 0
	for received < expected {
		message := chan_recv(input) or |err| {
			break
		}
		bytes = bytes + len(message)
		received = received + 1
	}
	return bytes
}

fn long_loop(iterations i32) i32 {
	total := 0
	for i := 0; i < iterations; i = i + 1 {
		step := i % 7
		total = total + step
	}
	return total
}

fn main() i32 {
	messages := chan_new[str](8)
	results := taskgroup []i32 {
		spawn produce(1, messages, 40)
		spawn produce(2, messages, 40)
		spawn produce(3, messages, 40)
		spawn produce(4, messages, 40)
		spawn consume(messages, 160)
	}
	produced := results[0] + results[1] + results[2] + results[3]
	if produced != 3439120 || results[4] != 760 {
		print("unexpected task results\n")
		return 1
	}
	if long_loop(3000000) != 8999994 {
		print("unexpected loop total\n")
		return 1
	}
	print("tasks ok\n")
	return 0
}
