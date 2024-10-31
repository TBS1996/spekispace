use std::time::Duration;
use std::time::SystemTime;

pub fn duration_to_days(dur: &Duration) -> f32 {
    dur.as_secs_f32() / 86400.
}

pub fn days_to_duration(days: f32) -> Duration {
    Duration::from_secs_f32(days * 86400.)
}

pub fn current_time() -> Duration {
    system_time_as_unix_time(SystemTime::now())
}

pub fn system_time_as_unix_time(time: SystemTime) -> Duration {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .expect("Time went backwards")
}

/// Safe way to truncate string.
pub fn truncate_string(input: String, max_len: usize) -> String {
    let mut graphemes = input.chars();
    let mut result = String::new();

    for _ in 0..max_len {
        if let Some(c) = graphemes.next() {
            result.push(c);
        } else {
            break;
        }
    }

    result
}
