package main

struct User {
	id i32
	name str
}

fn main() i32 {
	var winner User
	users := [3]User{
		User{id: 1, name: "alice"},
		User{id: 2, name: "bob"},
		User{id: 3, name: "eve"},
	}

	for i := 0; i < len(users); i = i + 1 {
		user := users[i]
		if !(user.id % 2 == 0) {
			continue
		} else {
			winner = user
			break
		}
	}

	if winner.id == 0 {
		return 1
	} else {
		winner.name = "B"
		print(winner.name)
		print("\n")
		return 0
	}
}
