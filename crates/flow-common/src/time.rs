//! Small civil-time helpers (no chrono): proleptic Gregorian, UTC plus a
//! fixed offset. DST-aware zones are the caller's job (recompute offsets).

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 0 = Monday .. 6 = Sunday for days since epoch (1970-01-01 was a Thursday).
pub fn weekday(days: i64) -> u32 {
    ((days + 3).rem_euclid(7)) as u32
}

pub fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Civil {
    pub y: i64,
    pub mo: u32,
    pub d: u32,
    pub h: u32,
    pub mi: u32,
    pub s: u32,
}

/// Unix seconds (UTC) -> civil at fixed `offset_s`.
pub fn to_civil(ts: i64, offset_s: i64) -> Civil {
    let t = ts + offset_s;
    let days = t.div_euclid(86_400);
    let rem = t.rem_euclid(86_400) as u32;
    let (y, mo, d) = civil_from_days(days);
    Civil {
        y,
        mo,
        d,
        h: rem / 3600,
        mi: rem % 3600 / 60,
        s: rem % 60,
    }
}

/// Civil at fixed `offset_s` -> unix seconds.
pub fn from_civil(c: Civil, offset_s: i64) -> i64 {
    days_from_civil(c.y, c.mo, c.d) * 86_400
        + i64::from(c.h) * 3600
        + i64::from(c.mi) * 60
        + i64::from(c.s)
        - offset_s
}

/// Parse an iCalendar-style `YYYYMMDD` / `YYYYMMDDTHHMMSS[Z]` value.
/// Returns (unix seconds treating floating times as UTC, is_date_only, is_utc).
pub fn parse_ical(v: &str) -> Option<(i64, bool, bool)> {
    let v = v.trim();
    let n = |a: usize, b: usize| v.get(a..b)?.parse::<u32>().ok();
    let y = v.get(0..4)?.parse::<i64>().ok()?;
    let (mo, d) = (n(4, 6)?, n(6, 8)?);
    if !(1..=12).contains(&mo) || d == 0 || d > days_in_month(y, mo) {
        return None;
    }
    if v.len() == 8 {
        return Some((
            from_civil(
                Civil {
                    y,
                    mo,
                    d,
                    h: 0,
                    mi: 0,
                    s: 0,
                },
                0,
            ),
            true,
            false,
        ));
    }
    if v.get(8..9)? != "T" {
        return None;
    }
    let (h, mi, s) = (n(9, 11)?, n(11, 13)?, n(13, 15)?);
    if h > 23 || mi > 59 || s > 60 {
        return None;
    }
    let utc = v.ends_with('Z');
    if v.len() != if utc { 16 } else { 15 } {
        return None;
    }
    Some((from_civil(Civil { y, mo, d, h, mi, s }, 0), false, utc))
}

/// Parse `+hhmm`/`-hhmm` (as printed by `date +%z`) into seconds.
pub fn parse_offset(s: &str) -> Option<i64> {
    let s = s.trim();
    let sign = match s.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let h = s.get(1..3)?.parse::<i64>().ok()?;
    let m = s.get(3..5)?.parse::<i64>().ok()?;
    Some(sign * (h * 3600 + m * 60))
}

/// Current local UTC offset via `date +%z` (so the system tz database decides,
/// including DST). Falls back to 0.
pub fn local_offset_s() -> i64 {
    std::process::Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .and_then(|o| parse_offset(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_and_known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(weekday(0), 3); // Thursday
        assert_eq!(weekday(days_from_civil(2024, 2, 29)), 3);
    }

    #[test]
    fn roundtrip_many() {
        for d in (-800_000..800_000).step_by(997) {
            let (y, m, dd) = civil_from_days(d);
            assert_eq!(days_from_civil(y, m, dd), d);
        }
    }

    #[test]
    fn civil_with_offset() {
        let ts = 1_700_000_000; // 2023-11-14T22:13:20Z
        let c = to_civil(ts, 0);
        assert_eq!((c.y, c.mo, c.d, c.h, c.mi, c.s), (2023, 11, 14, 22, 13, 20));
        let c2 = to_civil(ts, 7200);
        assert_eq!((c2.d, c2.h), (15, 0));
        assert_eq!(from_civil(c2, 7200), ts);
    }

    #[test]
    fn ical_parse() {
        let (t, date, utc) = parse_ical("20231114T221320Z").unwrap();
        assert_eq!((t, date, utc), (1_700_000_000, false, true));
        assert!(parse_ical("20231114").unwrap().1);
        assert!(parse_ical("20230230").is_none());
        assert!(parse_ical("20231114T256000Z").is_none());
        assert!(parse_ical("garbage").is_none());
        assert!(!parse_ical("20231114T221320").unwrap().2);
    }

    #[test]
    fn offsets() {
        assert_eq!(parse_offset("+0530"), Some(19_800));
        assert_eq!(parse_offset("-0800\n"), Some(-28_800));
        assert_eq!(parse_offset("x"), None);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
    }
}
