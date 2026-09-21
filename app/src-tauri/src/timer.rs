//! Timers, typed on any line of a note:
//!
//! | command               | effect                                         |
//! |-----------------------|------------------------------------------------|
//! | `timer`               | stopwatch                                      |
//! | `timer 3.5`, `3:30`   | countdown (minutes, or m:ss)                   |
//! | `timer 9AM`, `21:15`  | countdown to that time of day                  |
//! | `timer 5: Laundry`    | named countdown                                |
//! | `timer 25 5`          | pomodoro: 25 min work, 5 min rest              |
//! | `timer pomo`          | standard 25/5 pomodoro                         |
//! | `timer p` / `r` / `s` | pause-resume / restart / stop (`timer 0` too)  |
//!
//! The state lives in Rust so it keeps running while the window is hidden.

use chrono::{Local, NaiveTime, TimeZone, Timelike};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Stopwatch { title: String },
    Countdown { ms: i64, title: String },
    Pomodoro { work_ms: i64, rest_ms: i64, title: String },
    Pause,
    Restart,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Stopwatch,
    Countdown,
    Pomodoro,
}

#[derive(Debug, Clone, Serialize)]
pub struct Timer {
    pub kind: Kind,
    pub title: String,
    /// Countdown length / current pomodoro phase length.
    pub duration_ms: i64,
    pub work_ms: i64,
    pub rest_ms: i64,
    /// "work" or "rest" for pomodoros.
    pub phase: &'static str,
    /// Time accumulated before the current run.
    pub accumulated_ms: i64,
    /// Start of the current run (0 when paused).
    pub started_ms: i64,
    pub running: bool,
}

/// What the UI shows every second.
#[derive(Debug, Clone, Serialize)]
pub struct Tick {
    pub kind: Kind,
    pub title: String,
    pub phase: &'static str,
    pub running: bool,
    /// Elapsed for stopwatches, remaining for countdowns and pomodoros.
    pub ms: i64,
    pub label: String,
}

/// Parses a line. `None` if it is not a timer command.
pub fn parse(line: &str) -> Option<Command> {
    let line = line.trim();
    let lower = line.to_lowercase();
    let rest = lower.strip_prefix("timer")?;
    if !(rest.is_empty() || rest.starts_with(' ') || rest.starts_with(':')) {
        return None;
    }
    // Title: text after the first colon that is not followed by a digit.
    let orig_rest = &line[5..];
    let (args, title) = split_title(orig_rest);
    let args = args.trim().to_lowercase();
    let parts: Vec<&str> = args.split_whitespace().collect();

    match parts.as_slice() {
        [] => Some(Command::Stopwatch { title }),
        ["p"] | ["pause"] => Some(Command::Pause),
        ["r"] | ["restart"] => Some(Command::Restart),
        ["s"] | ["stop"] | ["0"] => Some(Command::Stop),
        ["pomo"] | ["pomodoro"] => Some(Command::Pomodoro { work_ms: 25 * 60_000, rest_ms: 5 * 60_000, title }),
        [a] => parse_target(a).map(|ms| Command::Countdown { ms, title }),
        [a, b] => {
            let (w, r) = (minutes(a)?, minutes(b)?);
            Some(Command::Pomodoro { work_ms: w, rest_ms: r, title })
        }
        _ => None,
    }
}

fn split_title(s: &str) -> (&str, String) {
    let b = s.as_bytes();
    for (i, c) in s.char_indices() {
        if c == ':' && !b.get(i + 1).is_some_and(|n| n.is_ascii_digit()) {
            return (&s[..i], s[i + 1..].trim().to_string());
        }
    }
    (s, String::new())
}

/// "3.5" → 3.5 min, "3,5" too; "3:30" → 3 min 30 s.
fn minutes(s: &str) -> Option<i64> {
    if let Some((m, sec)) = s.split_once(':') {
        let m: i64 = m.parse().ok()?;
        let sec: i64 = sec.parse().ok()?;
        return (sec < 60).then_some((m * 60 + sec) * 1000);
    }
    let v: f64 = s.replace(',', ".").parse().ok()?;
    (v > 0.0 && v < 100_000.0).then_some((v * 60_000.0).round() as i64)
}

/// Countdown length for a duration or a time of day. "9am" and "21:15"
/// (two-digit hour) are times of day; "3:30" and "3.5" are durations.
fn parse_target(s: &str) -> Option<i64> {
    if let Some(x) = s.strip_suffix("am") {
        return until(x, Some(false));
    }
    if let Some(x) = s.strip_suffix("pm") {
        return until(x, Some(true));
    }
    match s.split_once(':') {
        Some((h, _)) if h.len() == 2 => until(s, None),
        _ => minutes(s),
    }
}

fn until(clock: &str, pm: Option<bool>) -> Option<i64> {
    let (h, m) = match clock.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None => (clock.parse::<u32>().ok()?, 0),
    };
    let h = match pm {
        Some(true) if h < 12 => h + 12,
        Some(false) if h == 12 => 0,
        _ => h,
    };
    let target = NaiveTime::from_hms_opt(h, m, 0)?;
    let now = Local::now();
    let mut day = now.date_naive();
    if target <= now.time().with_nanosecond(0)? {
        day = day.succ_opt()?;
    }
    let at = Local.from_local_datetime(&day.and_time(target)).single()?;
    Some((at - now).num_milliseconds())
}

impl Timer {
    pub fn start(cmd: &Command, now: i64) -> Option<Timer> {
        let base = |kind, title: &String, d, w, r| Timer {
            kind,
            title: title.clone(),
            duration_ms: d,
            work_ms: w,
            rest_ms: r,
            phase: "work",
            accumulated_ms: 0,
            started_ms: now,
            running: true,
        };
        match cmd {
            Command::Stopwatch { title } => Some(base(Kind::Stopwatch, title, 0, 0, 0)),
            Command::Countdown { ms, title } => Some(base(Kind::Countdown, title, *ms, 0, 0)),
            Command::Pomodoro { work_ms, rest_ms, title } => {
                Some(base(Kind::Pomodoro, title, *work_ms, *work_ms, *rest_ms))
            }
            _ => None,
        }
    }

    pub fn elapsed(&self, now: i64) -> i64 {
        self.accumulated_ms + if self.running { now - self.started_ms } else { 0 }
    }

    pub fn toggle_pause(&mut self, now: i64) {
        if self.running {
            self.accumulated_ms = self.elapsed(now);
            self.running = false;
        } else {
            self.started_ms = now;
            self.running = true;
        }
    }

    pub fn restart(&mut self, now: i64) {
        self.accumulated_ms = 0;
        self.started_ms = now;
        self.running = true;
        if self.kind == Kind::Pomodoro {
            self.phase = "work";
            self.duration_ms = self.work_ms;
        }
    }

    /// Advances the timer; returns `Some(message)` when a countdown or a
    /// pomodoro phase has just finished.
    pub fn advance(&mut self, now: i64) -> Option<&'static str> {
        if !self.running || self.kind == Kind::Stopwatch || self.elapsed(now) < self.duration_ms {
            return None;
        }
        match self.kind {
            Kind::Countdown => {
                self.accumulated_ms = self.duration_ms;
                self.running = false;
                Some("done")
            }
            Kind::Pomodoro => {
                let finished = self.phase;
                self.phase = if finished == "work" { "rest" } else { "work" };
                self.duration_ms = if self.phase == "work" { self.work_ms } else { self.rest_ms };
                self.accumulated_ms = 0;
                self.started_ms = now;
                Some(if finished == "work" { "work_done" } else { "rest_done" })
            }
            Kind::Stopwatch => None,
        }
    }

    pub fn tick(&self, now: i64) -> Tick {
        let ms = match self.kind {
            Kind::Stopwatch => self.elapsed(now),
            _ => (self.duration_ms - self.elapsed(now)).max(0),
        };
        Tick {
            kind: self.kind,
            title: self.title.clone(),
            phase: self.phase,
            running: self.running,
            ms,
            label: format_ms(ms),
        }
    }
}

pub fn format_ms(ms: i64) -> String {
    let s = (ms + 999) / 1000;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        assert_eq!(parse("timer"), Some(Command::Stopwatch { title: String::new() }));
        assert_eq!(parse("timer 3.5"), Some(Command::Countdown { ms: 210_000, title: String::new() }));
        assert_eq!(parse("timer 3,5"), Some(Command::Countdown { ms: 210_000, title: String::new() }));
        assert_eq!(parse("timer 3:30"), Some(Command::Countdown { ms: 210_000, title: String::new() }));
        assert_eq!(
            parse("timer 5: Errands: bank, post"),
            Some(Command::Countdown { ms: 300_000, title: "Errands: bank, post".into() })
        );
        assert_eq!(
            parse("Timer 25 5"),
            Some(Command::Pomodoro { work_ms: 1_500_000, rest_ms: 300_000, title: String::new() })
        );
        assert!(matches!(parse("timer pomo"), Some(Command::Pomodoro { work_ms: 1_500_000, .. })));
        assert_eq!(parse("timer p"), Some(Command::Pause));
        assert_eq!(parse("timer r"), Some(Command::Restart));
        assert_eq!(parse("timer s"), Some(Command::Stop));
        assert_eq!(parse("timer 0"), Some(Command::Stop));
        assert_eq!(parse("timers are great"), None);
        assert_eq!(parse("the timer 5"), None);
    }

    #[test]
    fn time_of_day_is_in_the_future() {
        for s in ["timer 9am", "timer 21:15", "timer 12pm"] {
            match parse(s) {
                Some(Command::Countdown { ms, .. }) => assert!(ms > 0 && ms <= 86_400_000, "{s}: {ms}"),
                other => panic!("{s}: {other:?}"),
            }
        }
    }

    #[test]
    fn countdown_and_pomodoro_flow() {
        let mut t = Timer::start(&Command::Countdown { ms: 1000, title: "x".into() }, 0).unwrap();
        assert_eq!(t.tick(400).ms, 600);
        t.toggle_pause(400);
        assert_eq!(t.tick(5000).ms, 600);
        t.toggle_pause(5000);
        assert_eq!(t.advance(5700), Some("done"));
        assert!(!t.running);

        let mut p = Timer::start(&Command::Pomodoro { work_ms: 1000, rest_ms: 500, title: String::new() }, 0).unwrap();
        assert_eq!(p.advance(1000), Some("work_done"));
        assert_eq!(p.phase, "rest");
        assert_eq!(p.advance(1500), Some("rest_done"));
        assert_eq!(format_ms(61_000), "1:01");
        assert_eq!(format_ms(3_600_000), "1:00:00");
    }
}
