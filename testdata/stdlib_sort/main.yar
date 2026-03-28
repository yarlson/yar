package main

import "conv"
import "sort"
import "strings"

fn main() i32 {
    words := []str{"beta", "alpha", "alphabet", "alp", "", "alpha"}
    sort.strings(words)
    if strings.join(words, ",") != ",alp,alpha,alpha,alphabet,beta" {
        return 1
    }

    empty_words := []str{}
    sort.strings(empty_words)
    if len(empty_words) != 0 {
        return 2
    }

    single_word := []str{"solo"}
    sort.strings(single_word)
    if single_word[0] != "solo" {
        return 3
    }

    ints := []i32{7, 0 - 3, 7, 2, 0, 10}
    sort.i32s(ints)
    if strings.join([]str{
        conv.itoa(ints[0]),
        conv.itoa(ints[1]),
        conv.itoa(ints[2]),
        conv.itoa(ints[3]),
        conv.itoa(ints[4]),
        conv.itoa(ints[5]),
    }, ",") != "-3,0,2,7,7,10" {
        return 4
    }

    empty_ints := []i32{}
    sort.i32s(empty_ints)
    if len(empty_ints) != 0 {
        return 5
    }

    single_int := []i32{42}
    sort.i32s(single_int)
    if single_int[0] != 42 {
        return 6
    }

    bigs := []i64{conv.to_i64(5), conv.to_i64(0 - 8), conv.to_i64(0), conv.to_i64(5), conv.to_i64(12)}
    sort.i64s(bigs)
    if strings.join([]str{
        conv.itoa64(bigs[0]),
        conv.itoa64(bigs[1]),
        conv.itoa64(bigs[2]),
        conv.itoa64(bigs[3]),
        conv.itoa64(bigs[4]),
    }, ",") != "-8,0,5,5,12" {
        return 7
    }

    empty_bigs := []i64{}
    sort.i64s(empty_bigs)
    if len(empty_bigs) != 0 {
        return 8
    }

    single_big := []i64{conv.to_i64(99)}
    sort.i64s(single_big)
    if single_big[0] != conv.to_i64(99) {
        return 9
    }

    print("sort ok\n")
    return 0
}
