package time

import "std/conv"

pub error InvalidArgument
pub error InvalidFormat
pub error Overflow

pub struct Timestamp {
    unix_nanoseconds i64
}

pub struct Instant {
    monotonic_nanoseconds i64
}

pub struct Duration {
    nanoseconds i64
}

pub struct Date {
    pub year i32
    pub month i32
    pub day i32
    pub hour i32
    pub minute i32
    pub second i32
    pub nanosecond i32
    pub weekday i32
}

pub fn now() !Timestamp {
    return Timestamp{unix_nanoseconds: now_unix_nanoseconds()?}
}

pub fn instant() Instant {
    return Instant{monotonic_nanoseconds: instant_nanoseconds()}
}

pub fn sleep(duration Duration) !void {
    sleep_nanoseconds(duration.nanoseconds)?
}

pub fn nanoseconds(value i64) Duration {
    return Duration{nanoseconds: value}
}

pub fn microseconds(value i64) !Duration {
    return Duration{nanoseconds: checked_mul(value, 1000)?}
}

pub fn milliseconds(value i64) !Duration {
    return Duration{nanoseconds: checked_mul(value, 1000000)?}
}

pub fn seconds(value i64) !Duration {
    return Duration{nanoseconds: checked_mul(value, nanos_per_second())?}
}

pub fn minutes(value i64) !Duration {
    return Duration{nanoseconds: checked_mul(value, 60 * nanos_per_second())?}
}

pub fn hours(value i64) !Duration {
    return Duration{nanoseconds: checked_mul(value, 3600 * nanos_per_second())?}
}

pub fn duration_nanoseconds(value Duration) i64 {
    return value.nanoseconds
}

pub fn timestamp_from_unix_nanoseconds(value i64) Timestamp {
    return Timestamp{unix_nanoseconds: value}
}

pub fn timestamp_unix_nanoseconds(value Timestamp) i64 {
    return value.unix_nanoseconds
}

pub fn timestamp_add(value Timestamp, span Duration) !Timestamp {
    return Timestamp{unix_nanoseconds: checked_add(value.unix_nanoseconds, span.nanoseconds)?}
}

pub fn timestamp_subtract(value Timestamp, span Duration) !Timestamp {
    return Timestamp{unix_nanoseconds: checked_sub(value.unix_nanoseconds, span.nanoseconds)?}
}

pub fn timestamp_difference(left Timestamp, right Timestamp) !Duration {
    return Duration{nanoseconds: checked_sub(left.unix_nanoseconds, right.unix_nanoseconds)?}
}

pub fn timestamp_before(left Timestamp, right Timestamp) bool {
    return left.unix_nanoseconds < right.unix_nanoseconds
}

pub fn timestamp_equal(left Timestamp, right Timestamp) bool {
    return left.unix_nanoseconds == right.unix_nanoseconds
}

pub fn instant_difference(later Instant, earlier Instant) !Duration {
    if later.monotonic_nanoseconds < earlier.monotonic_nanoseconds {
        return error.InvalidArgument
    }
    return Duration{
        nanoseconds: checked_sub(later.monotonic_nanoseconds, earlier.monotonic_nanoseconds)?,
    }
}

pub fn instant_before(left Instant, right Instant) bool {
    return left.monotonic_nanoseconds < right.monotonic_nanoseconds
}

pub fn instant_equal(left Instant, right Instant) bool {
    return left.monotonic_nanoseconds == right.monotonic_nanoseconds
}

pub fn utc_date(value Timestamp) Date {
    seconds := floor_div(value.unix_nanoseconds, nanos_per_second())
    nanosecond := value.unix_nanoseconds - seconds * nanos_per_second()
    days := floor_div(seconds, 86400)
    second_of_day := seconds - days * 86400
    date := civil_from_days(days)
    date.hour = conv.to_i32(second_of_day / 3600)
    date.minute = conv.to_i32(second_of_day % 3600 / 60)
    date.second = conv.to_i32(second_of_day % 60)
    date.nanosecond = conv.to_i32(nanosecond)
    date.weekday = conv.to_i32(floor_mod(days + 4, 7))
    return date
}

pub fn from_utc(
    year i32,
    month i32,
    day i32,
    hour i32,
    minute i32,
    second i32,
    nanosecond i32,
) !Timestamp {
    if !valid_civil(year, month, day, hour, minute, second, nanosecond) {
        return error.InvalidArgument
    }
    return timestamp_from_civil(year, month, day, hour, minute, second, nanosecond, 0)
}

pub fn format_rfc3339(value Timestamp) str {
    date := utc_date(value)
    text := padded(date.year, 4) + "-" + padded(date.month, 2) + "-" + padded(date.day, 2) +
        "T" + padded(date.hour, 2) + ":" + padded(date.minute, 2) + ":" + padded(date.second, 2)
    if date.nanosecond != 0 {
        fraction := padded(date.nanosecond, 9)
        end := 9
        for fraction[end - 1] == '0' {
            end -= 1
        }
        text = text + "." + fraction[0:end]
    }
    return text + "Z"
}

pub fn parse_rfc3339(value str) !Timestamp {
    if len(value) < 20 ||
        value[4] != '-' || value[7] != '-' || value[10] != 'T' ||
        value[13] != ':' || value[16] != ':' {
        return error.InvalidFormat
    }
    year := digits(value, 0, 4)
    month := digits(value, 5, 2)
    day := digits(value, 8, 2)
    hour := digits(value, 11, 2)
    minute := digits(value, 14, 2)
    second := digits(value, 17, 2)
    if year < 0 || month < 0 || day < 0 || hour < 0 || minute < 0 || second < 0 {
        return error.InvalidFormat
    }

    pos := 19
    nanosecond := 0
    if value[pos] == '.' {
        start := pos + 1
        pos = start
        for pos < len(value) && pos - start < 9 && digit_value(value[pos]) >= 0 {
            nanosecond = nanosecond * 10 + digit_value(value[pos])
            pos += 1
        }
        if pos == start || (pos < len(value) && digit_value(value[pos]) >= 0) {
            return error.InvalidFormat
        }
        for width := pos - start; width < 9; width += 1 {
            nanosecond *= 10
        }
    }

    offset_seconds := parse_offset(value, pos)?
    if !valid_civil(year, month, day, hour, minute, second, nanosecond) {
        return error.InvalidFormat
    }
    return timestamp_from_civil(year, month, day, hour, minute, second, nanosecond, offset_seconds)
}

fn now_unix_nanoseconds() !i64 {
    panic("time.now_unix_nanoseconds intrinsic")
}

fn instant_nanoseconds() i64 {
    panic("time.instant_nanoseconds intrinsic")
}

fn sleep_nanoseconds(nanoseconds i64) !void {
    panic("time.sleep_nanoseconds intrinsic")
}

fn parse_offset(value str, pos i32) !i32 {
    if pos == len(value) - 1 && value[pos] == 'Z' {
        return 0
    }
    if pos + 6 != len(value) || (value[pos] != '+' && value[pos] != '-') || value[pos + 3] != ':' {
        return error.InvalidFormat
    }
    hours := digits(value, pos + 1, 2)
    minutes := digits(value, pos + 4, 2)
    if hours < 0 || hours > 23 || minutes < 0 || minutes > 59 {
        return error.InvalidFormat
    }
    offset := hours * 3600 + minutes * 60
    if value[pos] == '-' {
        if offset == 0 {
            return error.InvalidFormat
        }
        return 0 - offset
    }
    return offset
}

fn timestamp_from_civil(
    year i32,
    month i32,
    day i32,
    hour i32,
    minute i32,
    second i32,
    nanosecond i32,
    offset_seconds i32,
) !Timestamp {
    days := days_from_civil(conv.to_i64(year), conv.to_i64(month), conv.to_i64(day))
    seconds := days * 86400 +
        conv.to_i64(hour * 3600 + minute * 60 + second - offset_seconds)
    if seconds < 0 && nanosecond > 0 {
        whole := checked_mul(seconds + 1, nanos_per_second())?
        remainder := nanos_per_second() - conv.to_i64(nanosecond)
        return Timestamp{unix_nanoseconds: checked_sub(whole, remainder)?}
    }
    nanos := checked_mul(seconds, nanos_per_second())?
    return Timestamp{unix_nanoseconds: checked_add(nanos, conv.to_i64(nanosecond))?}
}

fn valid_civil(
    year i32,
    month i32,
    day i32,
    hour i32,
    minute i32,
    second i32,
    nanosecond i32,
) bool {
    return month >= 1 && month <= 12 &&
        day >= 1 && day <= days_in_month(year, month) &&
        hour >= 0 && hour <= 23 &&
        minute >= 0 && minute <= 59 &&
        second >= 0 && second <= 59 &&
        nanosecond >= 0 && nanosecond <= 999999999
}

fn days_in_month(year i32, month i32) i32 {
    if month == 2 {
        if leap_year(year) {
            return 29
        }
        return 28
    }
    if month == 4 || month == 6 || month == 9 || month == 11 {
        return 30
    }
    return 31
}

fn leap_year(year i32) bool {
    return (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year i64, month i64, day i64) i64 {
    y := year
    if month <= 2 {
        y -= 1
    }
    era := floor_div(y, 400)
    year_of_era := y - era * 400
    shifted_month := month + 9
    if month > 2 {
        shifted_month = month - 3
    }
    day_of_year := (153 * shifted_month + 2) / 5 + day - 1
    day_of_era := year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year
    return era * 146097 + day_of_era - 719468
}

fn civil_from_days(days i64) Date {
    z := days + 719468
    era := floor_div(z, 146097)
    day_of_era := z - era * 146097
    year_of_era := (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365
    day_of_year := day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100)
    shifted_month := (5 * day_of_year + 2) / 153
    day := day_of_year - (153 * shifted_month + 2) / 5 + 1
    month := shifted_month + 3
    if shifted_month >= 10 {
        month = shifted_month - 9
    }
    year := year_of_era + era * 400
    if month <= 2 {
        year += 1
    }
    return Date{
        year: conv.to_i32(year),
        month: conv.to_i32(month),
        day: conv.to_i32(day),
        hour: 0,
        minute: 0,
        second: 0,
        nanosecond: 0,
        weekday: 0,
    }
}

fn checked_add(left i64, right i64) !i64 {
    if (right > 0 && left > max_i64() - right) || (right < 0 && left < min_i64() - right) {
        return error.Overflow
    }
    return left + right
}

fn checked_sub(left i64, right i64) !i64 {
    if (right < 0 && left > max_i64() + right) || (right > 0 && left < min_i64() + right) {
        return error.Overflow
    }
    return left - right
}

fn checked_mul(value i64, factor i64) !i64 {
    if value > max_i64() / factor || value < min_i64() / factor {
        return error.Overflow
    }
    return value * factor
}

fn floor_div(value i64, divisor i64) i64 {
    quotient := value / divisor
    if value % divisor != 0 && value < 0 {
        quotient -= 1
    }
    return quotient
}

fn floor_mod(value i64, divisor i64) i64 {
    return value - floor_div(value, divisor) * divisor
}

fn digits(value str, start i32, count i32) i32 {
    result := 0
    for i := start; i < start + count; i += 1 {
        digit := digit_value(value[i])
        if digit < 0 {
            return -1
        }
        result = result * 10 + digit
    }
    return result
}

fn digit_value(byte i32) i32 {
    if byte >= '0' && byte <= '9' {
        return byte - '0'
    }
    return -1
}

fn padded(value i32, width i32) str {
    text := conv.itoa(value)
    for len(text) < width {
        text = "0" + text
    }
    return text
}

fn nanos_per_second() i64 {
    return 1000000000
}

fn max_i64() i64 {
    return 9223372036854775807
}

fn min_i64() i64 {
    return -9223372036854775807 - 1
}
