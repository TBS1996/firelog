use crate::task::TaskLog;
use js_sys::Date;
use std::time::Duration;

type UnixTime = Duration;

pub fn current_time() -> UnixTime {
    let date = Date::new_0();
    let milliseconds_since_epoch = date.get_time() as u64;
    let seconds_since_epoch = milliseconds_since_epoch / 1000;
    UnixTime::from_secs(seconds_since_epoch)
}

pub fn dur_format(dur: Duration) -> String {
    if dur > Duration::from_secs(86400) {
        let days = dur.as_secs_f32() / 86400.;
        format!("{:.1}d", days)
    } else if dur > Duration::from_secs(3600) {
        let hrs = dur.as_secs_f32() / 3600.;
        format!("{:.1}h", hrs)
    } else {
        let mins = dur.as_secs_f32() / 60.;
        format!("{:.1}m", mins)
    }
}

pub fn str_as_mins(s: &str) -> Option<Duration> {
    let mins: f32 = s.parse().ok()?;
    Some(Duration::from_secs_f32(mins * 60.))
}

pub fn str_as_days(s: &str) -> Option<Duration> {
    let days: f32 = s.parse().ok()?;
    Some(Duration::from_secs_f32(days * 86400.))
}

pub fn value_since(s: &str) -> Duration {
    match s {
        "1" => Duration::from_secs(86400),
        "2" => Duration::from_secs(86400 * 7),
        "3" => Duration::from_secs(86400 * 30),
        "4" => Duration::from_secs(1000000000),
        _ => panic!(),
    }
}

pub fn format_float(f: f32) -> String {
    if f < 10. {
        format!("{:.3}", f)
    } else if f < 100. {
        format!("{:.2}", f)
    } else if f < 1000. {
        format!("{:.1}", f)
    } else {
        format!("{}", f as u32)
    }
}

pub fn logstr(log: &TaskLog) -> String {
    let logstr: Vec<String> = log
        .time_since(current_time())
        .into_iter()
        .map(|dur| dur_format(dur))
        .collect();
    let logstr = format!("{:?}", logstr);
    let mut logstr = logstr.replace("\"", "");
    logstr.pop();
    logstr.remove(0);
    logstr
}
