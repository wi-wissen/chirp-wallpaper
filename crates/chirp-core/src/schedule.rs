//! When a running app should look for a new picture.
//!
//! The apps tick once a minute and ask [`Schedule::tick`]. A check is due
//! when a new day has begun (local time), when the clock jumped because the
//! computer was asleep or hibernating, or when the last check is an hour old.
//! Ticking is cheap: it only compares times.

use std::time::{Duration, SystemTime};
use time::{Date, OffsetDateTime, UtcOffset};

/// A gap between ticks longer than this means the computer was asleep.
const WAKE_GAP: Duration = Duration::from_secs(150);
/// Check at least this often while running.
const REGULAR: Duration = Duration::from_secs(3600);

pub struct Schedule {
    offset: UtcOffset,
    last_tick: SystemTime,
    last_check: SystemTime,
    last_day: Date,
}

impl Schedule {
    /// Start counting from `now`; call this right after the first check was started.
    pub fn new(now: SystemTime) -> Self {
        // Reading the local offset can fail on some systems; UTC is then good enough.
        let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
        Self::with_offset(now, offset)
    }

    pub fn with_offset(now: SystemTime, offset: UtcOffset) -> Self {
        Self {
            offset,
            last_tick: now,
            last_check: now,
            last_day: local_day(now, offset),
        }
    }

    /// Called once a minute: true when a check should run now.
    pub fn tick(&mut self, now: SystemTime) -> bool {
        let since_tick = now.duration_since(self.last_tick).unwrap_or_default();
        let since_check = now.duration_since(self.last_check).unwrap_or_default();
        let day = local_day(now, self.offset);
        self.last_tick = now;
        let due = day != self.last_day || since_tick > WAKE_GAP || since_check >= REGULAR;
        if due {
            self.checked(now);
        }
        due
    }

    /// Note a check that ran for another reason (e.g. "Update now").
    pub fn checked(&mut self, now: SystemTime) {
        self.last_check = now;
        self.last_day = local_day(now, self.offset);
    }
}

fn local_day(t: SystemTime, offset: UtcOffset) -> Date {
    OffsetDateTime::from(t).to_offset(offset).date()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    // 2026-11-22 22:00:00 UTC = 23:00 in Germany (UTC+1)
    const BASE: u64 = 1_795_384_800;

    fn germany(now: SystemTime) -> Schedule {
        Schedule::with_offset(now, UtcOffset::from_hms(1, 0, 0).unwrap())
    }

    #[test]
    fn quiet_minutes_do_nothing() {
        let mut s = germany(at(BASE));
        for m in 1..30 {
            assert!(!s.tick(at(BASE + m * 60)));
        }
    }

    /// Tick every minute from `from` to `to` (exclusive), counting due checks.
    fn ticks(s: &mut Schedule, from: u64, to: u64) -> usize {
        (from..to).filter(|m| s.tick(at(BASE + m * 60))).count()
    }

    #[test]
    fn new_local_day_triggers_once() {
        let mut s = germany(at(BASE));
        // 23:01 to 23:59 local: nothing; 00:00 local: check; 00:01: nothing again.
        assert_eq!(ticks(&mut s, 1, 60), 0);
        assert!(s.tick(at(BASE + 60 * 60)));
        assert!(!s.tick(at(BASE + 61 * 60)));
    }

    #[test]
    fn waking_up_triggers() {
        let mut s = germany(at(BASE));
        assert!(!s.tick(at(BASE + 60)));
        // Slept for 20 minutes: the next tick comes late.
        assert!(s.tick(at(BASE + 60 + 20 * 60)));
        assert!(!s.tick(at(BASE + 60 + 21 * 60)));
    }

    #[test]
    fn hourly_check() {
        let start = BASE - 6 * 3600; // early evening, no day change in between
        let mut s = germany(at(start));
        let mut due = 0;
        for m in 1..=120 {
            if s.tick(at(start + m * 60)) {
                due += 1;
            }
        }
        assert_eq!(due, 2);
    }

    #[test]
    fn manual_check_resets_the_hour() {
        let start = BASE - 6 * 3600;
        let mut s = germany(at(start));
        for m in 1..50 {
            s.tick(at(start + m * 60));
        }
        s.checked(at(start + 50 * 60));
        // No check at minute 60 any more, the next one is an hour after the manual one.
        let due: Vec<u64> = (51..=120).filter(|m| s.tick(at(start + m * 60))).collect();
        assert_eq!(due, vec![110]);
    }
}
