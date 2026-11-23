//! Small shared helpers: Laravel-style timestamps and dates, random tokens, UUIDs.

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// Timestamp format used in every `*_at` column (Laravel's default), always UTC.
pub const TS_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
/// Date format used in every `*_on` column.
pub const DATE_FORMAT: &str = "%Y-%m-%d";

/// Now as a UTC timestamp string: `2026-11-22 18:04:05`.
pub fn now_ts() -> String {
    Utc::now().format(TS_FORMAT).to_string()
}

/// A UTC timestamp string `minutes` ago (negative = in the future).
pub fn ts_minutes_ago(minutes: i64) -> String {
    (Utc::now() - Duration::minutes(minutes)).format(TS_FORMAT).to_string()
}

/// Today's date on this computer (local time), the equivalent of Laravel's `today()`.
pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// Today as `YYYY-MM-DD`.
pub fn today_str() -> String {
    today().format(DATE_FORMAT).to_string()
}

/// Parse a `YYYY-MM-DD` (or a timestamp starting with one) into a date.
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    NaiveDate::parse_from_str(s.get(..10).unwrap_or(s), DATE_FORMAT).ok()
}

/// Parse a stored UTC timestamp.
pub fn parse_ts(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s.trim(), TS_FORMAT).ok()
}

/// Monday 00:00 of this week (local), as a UTC timestamp string, like `now()->startOfWeek()`.
pub fn start_of_week_ts() -> String {
    let today = today();
    let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    let local = Local.from_local_datetime(&monday.and_hms_opt(0, 0, 0).unwrap_or_default()).earliest().unwrap_or_else(Local::now);
    local.with_timezone(&Utc).format(TS_FORMAT).to_string()
}

/// Fill a buffer with secure random bytes.
pub fn random_bytes(buf: &mut [u8]) {
    if getrandom::fill(buf).is_err() {
        // The OS RNG never fails on supported platforms; fall back to a time-seeded mix just in case.
        let mut x = Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64 | 1;
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = x as u8;
        }
    }
}

/// A random alphanumeric string, like Laravel's `Str::random($len)`.
pub fn random_token(len: usize) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut out = String::with_capacity(len);
    let mut buf = [0u8; 64];
    while out.len() < len {
        random_bytes(&mut buf);
        for &b in buf.iter() {
            // Rejection sampling keeps the distribution uniform (248 = 4 * 62).
            if b < 248 && out.len() < len {
                out.push(CHARS[(b % 62) as usize] as char);
            }
        }
    }
    out
}

/// A random version 4 UUID (used for `notifications.id`).
pub fn uuid_v4() -> String {
    let mut b = [0u8; 16];
    random_bytes(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// `true` for null, an empty/whitespace string, an empty array or object (Laravel's `blank()`).
pub fn blank(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => true,
        serde_json::Value::String(s) => s.trim().is_empty(),
        serde_json::Value::Array(a) => a.is_empty(),
        serde_json::Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

/// The current platform name sent to the renderer: `macos`, `windows` or `linux`.
pub fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_and_uuids() {
        let t = random_token(40);
        assert_eq!(t.len(), 40);
        assert!(t.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(t, random_token(40));
        let u = uuid_v4();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
    }

    #[test]
    fn dates() {
        assert_eq!(parse_date("2026-11-22 10:00:00"), NaiveDate::from_ymd_opt(2026, 11, 22));
        assert!(parse_date("nope").is_none());
        assert_eq!(now_ts().len(), 19);
    }
}
