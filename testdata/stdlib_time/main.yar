package main

import "std/conv"
import "std/time"

fn main() !i32 {
    epoch := time.from_utc(1970, 1, 1, 0, 0, 0, 0)?
    if time.timestamp_unix_nanoseconds(epoch) != 0 || time.format_rfc3339(epoch) != "1970-01-01T00:00:00Z" {
        return 1
    }

    leap := time.from_utc(2000, 2, 29, 12, 34, 56, 789000000)?
    if time.timestamp_unix_nanoseconds(leap) != 951827696789000000 {
        return 2
    }
    date := time.utc_date(leap)
    if date.year != 2000 || date.month != 2 || date.day != 29 || date.hour != 12 ||
        date.minute != 34 || date.second != 56 || date.nanosecond != 789000000 ||
        date.weekday != 2 {
        return 3
    }
    if time.format_rfc3339(leap) != "2000-02-29T12:34:56.789Z" {
        return 4
    }

    before_epoch := time.timestamp_from_unix_nanoseconds(conv.to_i64(-1))
    earlier := time.utc_date(before_epoch)
    if earlier.year != 1969 || earlier.month != 12 || earlier.day != 31 || earlier.weekday != 3 {
        return 5
    }
    if time.format_rfc3339(before_epoch) != "1969-12-31T23:59:59.999999999Z" {
        return 6
    }

    if !time.timestamp_equal(time.parse_rfc3339("2000-02-29T14:34:56.789+02:00")?, leap) ||
        !time.timestamp_equal(time.parse_rfc3339("2000-02-29T12:34:56.789000Z")?, leap) ||
        !time.timestamp_equal(time.parse_rfc3339("1970-01-01T00:00:00+00:00")?, epoch) {
        return 7
    }
    invalid := []str{
        "1970-01-01T00:00:00-00:00",
        "2001-02-29T00:00:00Z",
        "2000-01-01T24:00:00Z",
        "2000-01-01T00:00:60Z",
        "2000-01-01t00:00:00Z",
        "2000-01-01T00:00:00",
        "2000-01-01T00:00:00.Z",
        "2000-01-01T00:00:00.1234567890Z",
        "2000-01-01T00:00:00+24:00",
        "2000-1-01T00:00:00Z",
        "20x0-01-01T00:00:00Z",
    }
    for i := 0; i < len(invalid); i += 1 {
        if !parse_fails(invalid[i], time.InvalidFormat) {
            print("accepted " + invalid[i] + "\n")
            return 8
        }
    }

    latest := time.from_utc(2262, 4, 11, 23, 47, 16, 854775807)?
    if time.format_rfc3339(latest) != "2262-04-11T23:47:16.854775807Z" {
        return 9
    }
    earliest := time.parse_rfc3339("1677-09-21T00:12:43.145224192Z")?
    if time.format_rfc3339(earliest) != "1677-09-21T00:12:43.145224192Z" {
        return 10
    }
    if !from_utc_fails(2262, 4, 11, 23, 47, 16, 854775808, time.Overflow) ||
        !from_utc_fails(2023, 2, 29, 0, 0, 0, 0, time.InvalidArgument) ||
        !parse_fails("2262-04-11T23:47:17Z", time.Overflow) {
        return 11
    }

    if time.duration_nanoseconds(time.seconds(conv.to_i64(2))?) != 2000000000 ||
        time.duration_nanoseconds(time.milliseconds(conv.to_i64(-5))?) != -5000000 ||
        time.duration_nanoseconds(time.hours(conv.to_i64(2562047))?) != 9223369200000000000 {
        return 12
    }
    if !hours_overflow(conv.to_i64(2562048)) {
        return 13
    }

    later := time.timestamp_add(epoch, time.seconds(conv.to_i64(90))?)?
    if time.duration_nanoseconds(time.timestamp_difference(epoch, later)?) != -90000000000 ||
        !time.timestamp_before(epoch, later) {
        return 14
    }
    if !add_overflows(latest) {
        return 16
    }

    start := time.instant()
    time.sleep(time.milliseconds(conv.to_i64(2))?)?
    finish := time.instant()
    if time.duration_nanoseconds(time.instant_difference(finish, start)?) < 2000000 {
        return 17
    }
    if !reversed_difference_fails(start, finish) {
        return 19
    }
    rejected := false
    time.sleep(time.nanoseconds(conv.to_i64(-1))) or |err| {
        rejected = err == time.InvalidArgument
    }
    if !rejected {
        return 20
    }

    origin := time.instant()
    elapsed := taskgroup []i64 {
        spawn sleep_briefly()
        spawn nanoseconds_since(origin)
    }
    if elapsed[1] >= 200000000 {
        return 21
    }

    if time.timestamp_before(time.now()?, time.from_utc(2020, 1, 1, 0, 0, 0, 0)?) {
        return 22
    }

    print("time ok\n")
    return 0
}

fn parse_fails(text str, expected error) bool {
    value := time.parse_rfc3339(text) or |err| {
        return err == expected
    }
    _ := value
    return false
}

fn from_utc_fails(
    year i32,
    month i32,
    day i32,
    hour i32,
    minute i32,
    second i32,
    nanosecond i32,
    expected error,
) bool {
    value := time.from_utc(year, month, day, hour, minute, second, nanosecond) or |err| {
        return err == expected
    }
    _ := value
    return false
}

fn hours_overflow(value i64) bool {
    duration := time.hours(value) or |err| {
        return err == time.Overflow
    }
    _ := duration
    return false
}

fn add_overflows(value time.Timestamp) bool {
    later := time.timestamp_add(value, time.nanoseconds(conv.to_i64(1))) or |err| {
        return err == time.Overflow
    }
    _ := later
    return false
}

fn reversed_difference_fails(earlier time.Instant, later time.Instant) bool {
    duration := time.instant_difference(earlier, later) or |err| {
        return err == time.InvalidArgument
    }
    _ := duration
    return false
}

fn sleep_briefly() i64 {
    duration := time.milliseconds(conv.to_i64(300)) or |err| {
        return -1
    }
    time.sleep(duration) or |err| {
        return -1
    }
    return 0
}

fn nanoseconds_since(origin time.Instant) i64 {
    elapsed := time.instant_difference(time.instant(), origin) or |err| {
        return -1
    }
    return time.duration_nanoseconds(elapsed)
}
