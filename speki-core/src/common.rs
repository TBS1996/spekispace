use std::io::{self, ErrorKind};
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;
use std::time::{Duration, UNIX_EPOCH};

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

pub fn filename_sanitizer(s: &str) -> String {
    let s = s.replace(" ", "_").replace("'", "");
    sanitize_filename::sanitize(s)
}

pub fn open_file_with_vim(path: &Path) -> io::Result<()> {
    let status = Command::new("nvim").arg(path).status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            ErrorKind::Other,
            "Failed to open file with vim",
        ))
    }
}

pub fn get_last_modified(path: &Path) -> Duration {
    let metadata = std::fs::metadata(path).unwrap();
    let modified_time = metadata.modified().unwrap();
    let secs = modified_time
        .duration_since(UNIX_EPOCH)
        .map(|s| s.as_secs())
        .unwrap();
    Duration::from_secs(secs)
}
