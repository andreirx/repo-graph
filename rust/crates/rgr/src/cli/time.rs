//! Time utilities for CLI output.

use std::time::{SystemTime, UNIX_EPOCH};

/// Generate ISO 8601 timestamp for created_at fields.
///
/// Uses a rough calculation without external dependency.
/// Suitable for display timestamps, not calendar-critical operations.
pub fn chrono_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // Simple ISO 8601 format without external dependency
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        1970 + now / 31556952,           // Rough year
        (now % 31556952) / 2629746 + 1,  // Rough month
        (now % 2629746) / 86400 + 1,     // Rough day
        (now % 86400) / 3600,            // Hour
        (now % 3600) / 60,               // Minute
        now % 60                         // Second
    )
}

/// Return the current UTC time as an ISO 8601 string.
///
/// Format: `YYYY-MM-DDTHH:MM:SS.mmmZ` - compatible with the
/// lexicographic comparison used by `find_active_waivers` for
/// expiry checks. No external crate dependency (no chrono).
pub fn utc_now_iso8601() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let millis = dur.subsec_millis();

    // Break epoch seconds into date/time components.
    // Algorithm: civil_from_days (Howard Hinnant, public domain).
    let days = (secs / 86400) as i64;
    let day_secs = (secs % 86400) as u32;
    let hours = day_secs / 3600;
    let minutes = (day_secs % 3600) / 60;
    let seconds = day_secs % 60;

    // Days since 0000-03-01 (shifted epoch for leap year calc).
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, m, d, hours, minutes, seconds, millis,
    )
}
