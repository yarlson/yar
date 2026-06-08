package main

enum Color {
    Red
    Green
    Blue
}

enum Direction {
    Up
    Down
}

fn describe(color Color) i32 {
    match color {
    case Color.Red {
        return 1
    }
    else {
        return 0
    }
    }
}

fn classify(direction Direction) i32 {
    match direction {
    else {
        return 42
    }
    }
}

fn main() i32 {
    if describe(Color.Red) != 1 {
        return 1
    }
    if describe(Color.Green) != 0 {
        return 2
    }
    if describe(Color.Blue) != 0 {
        return 3
    }
    if classify(Direction.Up) != 42 {
        return 4
    }
    if classify(Direction.Down) != 42 {
        return 5
    }

    print("match_else ok\n")
    return 0
}
