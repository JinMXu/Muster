use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Parse the timestamp shapes the agent logs use: RFC3339 with `Z` or a
/// numeric offset, or plain epoch milliseconds.
pub fn parse_iso_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Ok(ms) = s.parse::<i64>() {
        return Some(ms);
    }
    let (date, rest) = s.split_once('T').or_else(|| s.split_once(' '))?;
    let mut dp = date.split('-');
    let y: i64 = dp.next()?.parse().ok()?;
    let m: i64 = dp.next()?.parse().ok()?;
    let d: i64 = dp.next()?.parse().ok()?;

    let mut time = rest;
    let mut offset_min = 0i64;
    if let Some(stripped) = time.strip_suffix('Z') {
        time = stripped;
    } else if let Some(pos) = time.rfind(['+', '-']) {
        let off = &time[pos..];
        let sign = if off.starts_with('-') { -1 } else { 1 };
        let mut op = off[1..].split(':');
        let oh: i64 = op.next()?.parse().ok()?;
        let om: i64 = op.next().unwrap_or("0").parse().ok()?;
        offset_min = sign * (oh * 60 + om);
        time = &time[..pos];
    }

    let (hms, frac) = time.split_once('.').unwrap_or((time, ""));
    let mut hms = hms.split(':');
    let h: i64 = hms.next()?.parse().ok()?;
    let mi: i64 = hms.next()?.parse().ok()?;
    let sec: i64 = hms.next().unwrap_or("0").parse().ok()?;
    let ms: i64 = if frac.is_empty() {
        0
    } else {
        let digits: String = frac.chars().take(3).collect();
        format!("{digits:0<3}").parse().ok()?
    };
    let days = days_from_civil(y, m, d);
    Some((days * 86_400 + h * 3_600 + mi * 60 + sec) * 1_000 + ms - offset_min * 60_000)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_z_and_offsets() {
        assert_eq!(parse_iso_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_iso_ms("2026-05-09T09:14:43.501Z"),
            Some(1778318083501)
        );
        assert_eq!(
            parse_iso_ms("2026-10-21T15:01:37.840+08:00"),
            Some(1792566097840)
        );
        assert_eq!(
            parse_iso_ms("2026-10-21T15:01:37.840+08:00"),
            parse_iso_ms("2026-10-21T07:01:37.840Z")
        );
    }

    #[test]
    fn parses_epoch_and_fraction_widths() {
        assert_eq!(parse_iso_ms("1785828912860"), Some(1785828912860));
        assert_eq!(parse_iso_ms("1970-01-01T00:00:00.5Z"), Some(500));
        assert_eq!(parse_iso_ms("1970-01-01T00:00:00.05Z"), Some(50));
    }
}
