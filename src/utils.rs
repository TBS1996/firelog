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
