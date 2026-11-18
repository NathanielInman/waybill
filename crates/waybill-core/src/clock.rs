//! Taskwarrior's timestamps and the local calendar. Times are Unix seconds
//! everywhere; a `Zone` turns one into the day and hour the user lived it in.

pub const HOUR: i64 = 3_600;
pub const DAY: i64 = 86_400;

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date (year, month, day) of a day number.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Parses Taskwarrior's `20261118T151505Z` into Unix seconds.
pub fn parse_stamp(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 16 || b[8] != b'T' || b[15] != b'Z' {
        return None;
    }
    let n = |a: usize, z: usize| s.get(a..z)?.parse::<i64>().ok();
    let (y, mo, d) = (n(0, 4)?, n(4, 6)?, n(6, 8)?);
    let (h, mi, se) = (n(9, 11)?, n(11, 13)?, n(13, 15)?);
    Some(days_from_civil(y, mo, d) * DAY + h * HOUR + mi * 60 + se)
}

/// Whose calendar to count days in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zone {
    /// The machine's time zone, daylight saving included.
    Local,
    /// A fixed number of seconds east of UTC (tests, demos, screenshots).
    Fixed(i64),
}

impl Zone {
    /// Seconds east of UTC at `ts`.
    pub fn offset(self, ts: i64) -> i64 {
        match self {
            Zone::Fixed(o) => o,
            Zone::Local => local_offset(ts),
        }
    }

    /// The local day number `ts` falls on.
    pub fn day(self, ts: i64) -> i64 {
        (ts + self.offset(ts)).div_euclid(DAY)
    }

    /// The hour of the local day, 0 to 23.
    pub fn hour(self, ts: i64) -> usize {
        ((ts + self.offset(ts)).rem_euclid(DAY) / HOUR) as usize
    }

    /// The first second of local day `day`.
    pub fn day_start(self, day: i64) -> i64 {
        let guess = day * DAY;
        guess - self.offset(guess)
    }
}

#[cfg(unix)]
fn local_offset(ts: i64) -> i64 {
    // SAFETY: localtime_r writes only into `tm`, which outlives the call.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let t = ts as libc::time_t;
        if libc::localtime_r(&t, &mut tm).is_null() {
            return 0;
        }
        tm.tm_gmtoff as i64
    }
}

#[cfg(not(unix))]
fn local_offset(_ts: i64) -> i64 {
    0
}

/// 0 is Monday.
pub fn weekday(day: i64) -> usize {
    (day + 3).rem_euclid(7) as usize
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
pub const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// "Nov 18".
pub fn day_label(day: i64) -> String {
    let (_, m, d) = civil_from_days(day);
    format!("{} {}", MONTHS[(m - 1) as usize], d)
}

/// "09:22" for a time already shifted into local seconds.
pub fn hm(local: i64) -> String {
    let s = local.rem_euclid(DAY);
    format!("{:02}:{:02}", s / HOUR, s % HOUR / 60)
}

/// How long ago, in the largest unit that fits: "now", "7 min", "3 h", "2 d".
pub fn span(seconds: i64) -> String {
    let s = seconds.max(0);
    match s {
        0..60 => "now".into(),
        60..3_600 => format!("{} min", s / 60),
        3_600..86_400 => format!("{} h", s / HOUR),
        _ => format!("{} d", s / DAY),
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps() {
        assert_eq!(parse_stamp("19700101T000000Z"), Some(0));
        assert_eq!(parse_stamp("20261118T151505Z"), Some(1_795_014_905));
        assert_eq!(parse_stamp("2026-11-18"), None);
    }

    #[test]
    fn civil_round_trip() {
        for day in [-1, 0, 59, 60, 20_775, 20_776] {
            let (y, m, d) = civil_from_days(day);
            assert_eq!(days_from_civil(y, m, d), day);
        }
        assert_eq!(
            civil_from_days(parse_stamp("20261118T151505Z").unwrap() / DAY),
            (2026, 11, 18)
        );
    }

    #[test]
    fn a_zone_moves_the_day() {
        // 03:00 UTC on the 18th is still the 17th in Chicago.
        let ts = parse_stamp("20261118T030000Z").unwrap();
        let chicago = Zone::Fixed(-6 * HOUR);
        assert_eq!(day_label(chicago.day(ts)), "Nov 17");
        assert_eq!(chicago.hour(ts), 21);
        assert_eq!(chicago.day_start(chicago.day(ts)), ts - 21 * HOUR);
        // 2026-11-18 is a Wednesday.
        assert_eq!(WEEKDAYS[weekday(Zone::Fixed(0).day(ts))], "Wed");
    }

    #[test]
    fn spans_read_plainly() {
        assert_eq!(span(20), "now");
        assert_eq!(span(7 * 60 + 5), "7 min");
        assert_eq!(span(3 * HOUR + 59), "3 h");
        assert_eq!(span(2 * DAY + 5), "2 d");
        assert_eq!(hm(9 * HOUR + 22 * 60), "09:22");
    }
}
