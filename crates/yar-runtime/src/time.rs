use std::{
    sync::OnceLock,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(crate) const STATUS_OK: i32 = 0;
pub(crate) const STATUS_OVERFLOW: i32 = 1;
pub(crate) const STATUS_INVALID_ARGUMENT: i32 = 2;

static MONOTONIC_ORIGIN: OnceLock<Instant> = OnceLock::new();

pub(crate) fn now_unix_nanoseconds() -> Option<i64> {
    unix_nanoseconds(SystemTime::now())
}

fn unix_nanoseconds(time: SystemTime) -> Option<i64> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_nanos()).ok(),
        Err(before) => i64::try_from(before.duration().as_nanos())
            .ok()
            .and_then(i64::checked_neg),
    }
}

pub(crate) fn instant_nanoseconds() -> Option<i64> {
    let origin = *MONOTONIC_ORIGIN.get_or_init(Instant::now);
    i64::try_from(Instant::now().duration_since(origin).as_nanos()).ok()
}

pub(crate) fn sleep_nanoseconds(nanoseconds: i64) -> i32 {
    let Ok(nanoseconds) = u64::try_from(nanoseconds) else {
        return STATUS_INVALID_ARGUMENT;
    };
    if nanoseconds > 0 {
        super::memory::blocking(|| std::thread::sleep(Duration::from_nanos(nanoseconds)));
    }
    STATUS_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_wall_clock_times_around_the_unix_epoch() {
        assert_eq!(unix_nanoseconds(UNIX_EPOCH), Some(0));
        assert_eq!(
            unix_nanoseconds(UNIX_EPOCH + Duration::new(1, 5)),
            Some(1_000_000_005)
        );
        assert_eq!(
            unix_nanoseconds(UNIX_EPOCH - Duration::new(2, 0)),
            Some(-2_000_000_000)
        );
        assert_eq!(
            unix_nanoseconds(UNIX_EPOCH + Duration::from_nanos(i64::MAX as u64 + 1)),
            None
        );
    }

    #[test]
    fn monotonic_readings_never_decrease_across_threads() {
        let readings = (0..4)
            .map(|_| std::thread::spawn(|| (instant_nanoseconds(), instant_nanoseconds())))
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>();
        for (first, second) in readings {
            assert!(first.unwrap() <= second.unwrap());
        }
    }

    #[test]
    fn sleep_rejects_negative_spans_and_waits_at_least_the_requested_span() {
        assert_eq!(sleep_nanoseconds(-1), STATUS_INVALID_ARGUMENT);
        assert_eq!(sleep_nanoseconds(0), STATUS_OK);
        let start = Instant::now();
        assert_eq!(sleep_nanoseconds(5_000_000), STATUS_OK);
        assert!(start.elapsed() >= Duration::from_millis(5));
    }
}
