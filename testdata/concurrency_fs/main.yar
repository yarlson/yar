package main

import "fs"
import "path"

fn main() !i32 {
	root := fs.temp_dir("yar-concurrency-fs")?
	file := path.join([]str{root, "data.txt"})
	fs.write_file(file, "hello")?

	values := taskgroup []!str {
		spawn fs.read_file(file)
		spawn fs.read_file(file)
	}

	first := values[0]?
	second := values[1]?
	print(first + "\n")
	print(second + "\n")
	fs.remove_all(root)?
	return 0
}
