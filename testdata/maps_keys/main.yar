package main

fn main() i32 {
    counts := map[str]i32{"alpha": 1, "beta": 2, "gamma": 3}
    names := keys(counts)

    print_int(len(names))
    print("\n")

    delete(counts, "alpha")
    counts["delta"] = 4

    print_int(len(names))
    print("\n")
    print_int(len(counts))
    print("\n")

    alpha := 0
    beta := 0
    gamma := 0
    delta := 0
    for i := 0; i < len(names); i = i + 1 {
        name := names[i]
        if name == "alpha" {
            alpha = alpha + 1
        }
        if name == "beta" {
            beta = beta + 1
        }
        if name == "gamma" {
            gamma = gamma + 1
        }
        if name == "delta" {
            delta = delta + 1
        }
    }

    print_int(alpha)
    print("\n")
    print_int(beta)
    print("\n")
    print_int(gamma)
    print("\n")
    print_int(delta)
    print("\n")

    flags := map[bool]i32{true: 1, false: 0}
    print_int(len(keys(flags)))
    print("\n")

    return 0
}
