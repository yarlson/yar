package main

import "std/process"
import "std/strings"

fn churn(id i32) str {
    kept := []str{}
    for i := 0; i < 50; i += 1 {
        kept = append(kept, "task" + to_str(id) + ":" + to_str(i))
    }
    for i := 0; i < 20000; i += 1 {
        garbage := strings.repeat(to_str(i), 3)
        if len(garbage) == 0 {
            return "empty"
        }
    }
    return strings.join(kept, ",")
}

fn produce(values chan[str], count i32) i32 {
    for i := 0; i < count; i += 1 {
        chan_send(values, "item-" + to_str(i)) or |err| {
            return -1
        }
        _ := strings.repeat("x", 64)
    }
    chan_close(values)
    return count
}

fn consume(values chan[str]) i32 {
    total := 0
    for true {
        value := chan_recv(values) or |err| {
            return total
        }
        if !strings.has_prefix(value, "item-") {
            return -1
        }
        total += 1
    }
    return total
}

fn share_builder(builder i64, rounds i32) i32 {
    total := 0
    for i := 0; i < rounds; i += 1 {
        sb_write(builder, "ab")
        total += len(sb_string(builder))
        total += len(process.args())
    }
    return total
}

fn main() i32 {
    lists := taskgroup []str {
        spawn churn(0)
        spawn churn(1)
        spawn churn(2)
        spawn churn(3)
    }
    for id := 0; id < 4; id += 1 {
        parts := strings.split(lists[id], ",")
        if len(parts) != 50 || parts[49] != "task" + to_str(id) + ":49" {
            return 1
        }
    }

    values := chan_new[str](4)
    counts := taskgroup []i32 {
        spawn produce(values, 5000)
        spawn consume(values)
    }
    if counts[0] != 5000 || counts[1] != 5000 {
        return 2
    }
    builder := sb_new()
    shared := taskgroup []i32 {
        spawn share_builder(builder, 2000)
        spawn share_builder(builder, 2000)
    }
    if shared[0] < 2000 || shared[1] < 2000 {
        return 3
    }

    print("collection ok\n")
    return 0
}
