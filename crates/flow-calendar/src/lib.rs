//! Calendar store. Local-first; events carry `href`/`etag`/`dirty` so a
//! CalDAV sync layer can be added later (TODO(feature): CalDAV client).
//! Times are UTC or floating (treated as UTC); TZID support is a TODO.

use flow_common::time::{
    civil_from_days, days_from_civil, days_in_month, from_civil, parse_ical, to_civil, weekday,
    Civil,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub freq: Freq,
    pub interval: u32,
    pub count: Option<u32>,
    pub until: Option<i64>,
    /// Weekly: bit 0 = Monday.
    pub by_day: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub uid: String,
    pub summary: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub rule: Option<Rule>,
    /// Minutes before start for reminders (VALARM TRIGGER:-PTnM).
    pub alarms_min: Vec<u32>,
    pub href: Option<String>,
    pub etag: Option<String>,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Occurrence {
    pub uid: String,
    pub summary: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
}

fn unescape(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for l in text.lines() {
        if (l.starts_with(' ') || l.starts_with('\t')) && !out.is_empty() {
            out.last_mut().unwrap().push_str(&l[1..]);
        } else {
            out.push(l.to_string());
        }
    }
    out
}

fn parse_rrule(v: &str) -> Option<Rule> {
    let mut r = Rule {
        freq: Freq::Daily,
        interval: 1,
        count: None,
        until: None,
        by_day: 0,
    };
    let mut have = false;
    for part in v.split(';') {
        let (k, val) = part.split_once('=')?;
        match k {
            "FREQ" => {
                r.freq = match val {
                    "DAILY" => Freq::Daily,
                    "WEEKLY" => Freq::Weekly,
                    "MONTHLY" => Freq::Monthly,
                    "YEARLY" => Freq::Yearly,
                    _ => return None,
                };
                have = true;
            }
            "INTERVAL" => r.interval = val.parse().ok().filter(|i| (1..=1000).contains(i))?,
            "COUNT" => r.count = Some(val.parse().ok()?),
            "UNTIL" => r.until = Some(parse_ical(val)?.0),
            "BYDAY" => {
                for d in val.split(',') {
                    let i = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"]
                        .iter()
                        .position(|x| d.ends_with(x))?;
                    r.by_day |= 1 << i;
                }
            }
            _ => {} // unsupported parts are ignored (documented limitation)
        }
    }
    have.then_some(r)
}

fn parse_trigger(v: &str) -> Option<u32> {
    let v = v.strip_prefix("-PT")?;
    if let Some(m) = v.strip_suffix('M') {
        m.parse().ok()
    } else if let Some(h) = v.strip_suffix('H') {
        h.parse::<u32>().ok().map(|h| h * 60)
    } else {
        None
    }
}

/// Parse all VEVENTs of an iCalendar document. Malformed events are skipped.
pub fn parse_ics(text: &str) -> Vec<Event> {
    let mut out = vec![];
    let mut cur: Option<Event> = None;
    for line in unfold(text) {
        let Some((name_params, value)) = line.split_once(':') else {
            continue;
        };
        let name = name_params
            .split(';')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        match (name.as_str(), cur.as_mut()) {
            ("BEGIN", _) if value == "VEVENT" => {
                cur = Some(Event {
                    uid: String::new(),
                    summary: String::new(),
                    start: 0,
                    end: 0,
                    all_day: false,
                    rule: None,
                    alarms_min: vec![],
                    href: None,
                    etag: None,
                    dirty: false,
                })
            }
            ("END", Some(_)) if value == "VEVENT" => {
                let mut e = cur.take().unwrap();
                if e.start != 0 && !e.uid.is_empty() {
                    if e.end == 0 {
                        e.end = e.start + if e.all_day { 86_400 } else { 3600 };
                    }
                    out.push(e);
                }
            }
            ("UID", Some(e)) => e.uid = value.to_string(),
            ("SUMMARY", Some(e)) => e.summary = unescape(value),
            ("DTSTART", Some(e)) => {
                if let Some((t, d, _)) = parse_ical(value) {
                    e.start = t;
                    e.all_day = d;
                }
            }
            ("DTEND", Some(e)) => {
                if let Some((t, _, _)) = parse_ical(value) {
                    e.end = t;
                }
            }
            ("RRULE", Some(e)) => e.rule = parse_rrule(value),
            ("TRIGGER", Some(e)) => {
                if let Some(m) = parse_trigger(value) {
                    e.alarms_min.push(m);
                }
            }
            _ => {}
        }
    }
    out
}

const MAX_OCCURRENCES: usize = 2000;

impl Event {
    /// Occurrences overlapping [from, to). Bounded to avoid runaway rules.
    pub fn occurrences(&self, from: i64, to: i64) -> Vec<Occurrence> {
        let dur = self.end - self.start;
        let mk = |s: i64| Occurrence {
            uid: self.uid.clone(),
            summary: self.summary.clone(),
            start: s,
            end: s + dur,
            all_day: self.all_day,
        };
        let Some(r) = &self.rule else {
            return if self.start < to && self.end > from {
                vec![mk(self.start)]
            } else {
                vec![]
            };
        };
        let c0 = to_civil(self.start, 0);
        let day0 = days_from_civil(c0.y, c0.mo, c0.d);
        let tod = self.start - day0 * 86_400;
        let mut out = vec![];
        let mut n = 0u32;
        let mut k = 0i64;
        loop {
            if out.len() >= MAX_OCCURRENCES || k > 100_000 {
                break;
            }
            // candidate start times for period k
            let cands: Vec<i64> = match r.freq {
                Freq::Daily => vec![self.start + k * i64::from(r.interval) * 86_400],
                Freq::Weekly => {
                    let week_start =
                        day0 - i64::from(weekday(day0)) + k * i64::from(r.interval) * 7;
                    let mask = if r.by_day == 0 {
                        1 << weekday(day0)
                    } else {
                        r.by_day
                    };
                    (0..7)
                        .filter(|d| mask & (1 << d) != 0)
                        .map(|d| (week_start + d) * 86_400 + tod)
                        .filter(|t| *t >= self.start)
                        .collect()
                }
                Freq::Monthly => {
                    let m0 = i64::from(c0.mo) - 1 + k * i64::from(r.interval);
                    let (y, m) = (c0.y + m0.div_euclid(12), (m0.rem_euclid(12) + 1) as u32);
                    if c0.d <= days_in_month(y, m) {
                        vec![
                            from_civil(
                                Civil {
                                    y,
                                    mo: m,
                                    d: c0.d,
                                    h: 0,
                                    mi: 0,
                                    s: 0,
                                },
                                0,
                            ) + tod,
                        ]
                    } else {
                        vec![]
                    }
                }
                Freq::Yearly => {
                    let y = c0.y + k * i64::from(r.interval);
                    if c0.d <= days_in_month(y, c0.mo) {
                        vec![
                            from_civil(
                                Civil {
                                    y,
                                    mo: c0.mo,
                                    d: c0.d,
                                    h: 0,
                                    mi: 0,
                                    s: 0,
                                },
                                0,
                            ) + tod,
                        ]
                    } else {
                        vec![]
                    }
                }
            };
            let first = cands.first().copied().unwrap_or(self.start + k * 86_400);
            if first >= to && r.until.is_none_or(|u| first > u) && k > 0 {
                break;
            }
            for s in cands {
                if r.until.is_some_and(|u| s > u) {
                    return out;
                }
                n += 1;
                if r.count.is_some_and(|c| n > c) {
                    return out;
                }
                if s >= to {
                    return out;
                }
                if s + dur > from {
                    out.push(mk(s));
                }
            }
            k += 1;
        }
        out
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Calendar {
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reminder {
    pub uid: String,
    pub summary: String,
    pub fire_at: i64,
    pub start: i64,
}

impl Calendar {
    pub fn upsert(&mut self, mut e: Event) {
        e.dirty = true;
        match self.events.iter_mut().find(|x| x.uid == e.uid) {
            Some(slot) => *slot = e,
            None => self.events.push(e),
        }
    }
    pub fn remove(&mut self, uid: &str) -> bool {
        let n = self.events.len();
        self.events.retain(|e| e.uid != uid);
        self.events.len() != n
    }
    pub fn import_ics(&mut self, text: &str) -> usize {
        let evs = parse_ics(text);
        let n = evs.len();
        for e in evs {
            self.upsert(e);
        }
        n
    }
    pub fn range(&self, from: i64, to: i64) -> Vec<Occurrence> {
        let mut v: Vec<_> = self
            .events
            .iter()
            .flat_map(|e| e.occurrences(from, to))
            .collect();
        v.sort_by_key(|o| (o.start, o.uid.clone()));
        v
    }
    /// Reminders whose fire time lies in (after, until].
    pub fn reminders(&self, after: i64, until: i64) -> Vec<Reminder> {
        let mut out = vec![];
        for e in &self.events {
            for m in &e.alarms_min {
                let lead = i64::from(*m) * 60;
                // look for occurrences starting in (after+lead, until+lead]
                for o in e.occurrences(after + lead, until + lead + 1) {
                    let fire = o.start - lead;
                    if fire > after && fire <= until {
                        out.push(Reminder {
                            uid: e.uid.clone(),
                            summary: e.summary.clone(),
                            fire_at: fire,
                            start: o.start,
                        });
                    }
                }
            }
        }
        out.sort_by_key(|r| r.fire_at);
        out
    }
    /// Next reminder strictly after `after` within `horizon_s`.
    pub fn next_reminder(&self, after: i64, horizon_s: i64) -> Option<Reminder> {
        self.reminders(after, after + horizon_s).into_iter().next()
    }
}

/// Civil date of a unix time (UTC), for day-grouping UIs.
pub fn day_of(ts: i64) -> (i64, u32, u32) {
    civil_from_days(ts.div_euclid(86_400))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICS: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a@x\r\nSUMMARY:Stand\\, up\r\nDTSTART:20231114T090000Z\r\nDTEND:20231114T091500Z\r\nRRULE:FREQ=DAILY;COUNT=3\r\nBEGIN:VALARM\r\nTRIGGER:-PT10M\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b@x\r\nSUMMARY:Holiday\r\nDTSTART;VALUE=DATE:20231225\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:no uid\r\nDTSTART:20231114T090000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    const D14: i64 = 1_699_920_000; // 2023-11-14T00:00:00Z

    #[test]
    fn parse_events() {
        let e = parse_ics(ICS);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].summary, "Stand, up");
        assert_eq!(e[0].start, D14 + 9 * 3600);
        assert_eq!(e[0].alarms_min, vec![10]);
        assert_eq!(e[0].rule.as_ref().unwrap().count, Some(3));
        assert!(e[1].all_day);
        assert_eq!(e[1].end - e[1].start, 86_400);
    }

    #[test]
    fn daily_count_bounded() {
        let e = &parse_ics(ICS)[0];
        let o = e.occurrences(D14, D14 + 30 * 86_400);
        assert_eq!(o.len(), 3);
        assert_eq!(o[2].start, D14 + 2 * 86_400 + 9 * 3600);
        assert_eq!(e.occurrences(D14 + 3 * 86_400, D14 + 10 * 86_400).len(), 0);
    }

    #[test]
    fn weekly_byday_and_until() {
        let mut e = parse_ics(ICS).remove(0);
        e.rule = parse_rrule("FREQ=WEEKLY;BYDAY=MO,FR;UNTIL=20231201T000000Z");
        let o = e.occurrences(D14, D14 + 60 * 86_400);
        let days: Vec<_> = o.iter().map(|x| day_of(x.start)).collect();
        assert_eq!(
            days,
            vec![
                (2023, 11, 17),
                (2023, 11, 20),
                (2023, 11, 24),
                (2023, 11, 27)
            ]
        );
    }

    #[test]
    fn monthly_skips_short_months() {
        let mut e = parse_ics(ICS).remove(0);
        e.start = flow_common::time::from_civil(
            Civil {
                y: 2024,
                mo: 1,
                d: 31,
                h: 8,
                mi: 0,
                s: 0,
            },
            0,
        );
        e.end = e.start + 3600;
        e.rule = parse_rrule("FREQ=MONTHLY;COUNT=4");
        let o = e.occurrences(e.start, e.start + 200 * 86_400);
        let months: Vec<_> = o.iter().map(|x| day_of(x.start).1).collect();
        assert_eq!(months, vec![1, 3, 5, 7]); // short months skipped, not counted (RFC 5545)
    }

    #[test]
    fn reminders_and_store() {
        let mut c = Calendar::default();
        assert_eq!(c.import_ics(ICS), 2);
        let r = c.reminders(D14, D14 + 86_400);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].fire_at, D14 + 9 * 3600 - 600);
        assert!(c.next_reminder(D14 + 9 * 3600 - 600, 3 * 86_400).is_some()); // next day's
        assert!(c.events.iter().all(|e| e.dirty));
        assert!(c.remove("b@x") && !c.remove("b@x"));
        let rng = c.range(D14, D14 + 2 * 86_400);
        assert_eq!(rng.len(), 2);
    }

    #[test]
    fn bad_rrules_ignored() {
        assert!(parse_rrule("FREQ=HOURLY").is_none());
        assert!(parse_rrule("INTERVAL=2").is_none());
        assert!(parse_rrule("FREQ=DAILY;INTERVAL=0").is_none());
    }
}
