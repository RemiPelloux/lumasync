use std::time::{Duration, Instant};

pub(super) fn wait(deadline: Instant, period: Duration, dropped: &mut u64) -> Instant {
    let now = Instant::now();
    let (next, skipped) = schedule(now, deadline, period);
    *dropped += skipped;
    std::thread::sleep(next.saturating_duration_since(Instant::now()));
    next
}

fn schedule(now: Instant, deadline: Instant, period: Duration) -> (Instant, u64) {
    let next = deadline + period;
    if next >= now {
        return (next, 0);
    }
    let skipped = (now.duration_since(next).as_nanos() / period.as_nanos()) as u64 + 1;
    (next + period.mul_f64(skipped as f64), skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_work_keeps_original_cadence() {
        let start = Instant::now();
        let period = Duration::from_millis(20);
        assert_eq!(
            schedule(start + Duration::from_millis(3), start, period),
            (start + period, 0)
        );
    }
    #[test]
    fn overrun_skips_slots_without_immediate_catchup() {
        let start = Instant::now();
        let period = Duration::from_millis(20);
        let now = start + Duration::from_millis(61);
        assert_eq!(
            schedule(now, start, period),
            (start + Duration::from_millis(80), 3)
        );
    }
}
