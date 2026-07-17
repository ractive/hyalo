/// Compute the Levenshtein edit distance between two strings.
///
/// Uses the standard iterative two-row DP algorithm.
/// Runs in O(|a| * |b|) time and O(|a| + |b|) additional space.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    // Ensure the shorter string is in the column dimension for minimal allocation.
    let (a, b) = if a.len() < b.len() { (b, a) } else { (a, b) };

    let m = a.len();
    let n = b.len();

    // prev[j] = edit distance between a[..i-1] and b[..j]
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            curr[j] = (curr[j - 1] + 1) // insertion
                .min(prev[j] + 1) // deletion
                .min(prev[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Returns `true` for strings that are both YYYY-MM-DD formatted **and**
/// represent a valid calendar date (month 1–12, day within the month's
/// actual range, including leap-year February).
///
/// Purely shape-matching strings like `"2026-13-50"` or `"2026-02-30"`
/// return `false`.
pub fn is_iso8601_date(s: &str) -> bool {
    if s.len() != 10 {
        return false;
    }
    let b = s.as_bytes();
    if b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    if !b[..4].iter().all(u8::is_ascii_digit)
        || !b[5..7].iter().all(u8::is_ascii_digit)
        || !b[8..10].iter().all(u8::is_ascii_digit)
    {
        return false;
    }

    // Parse numeric components. SAFETY: we verified all bytes are ASCII digits
    // above, so parse cannot fail.
    let year: u32 = s[..4].parse().unwrap_or(0);
    let month: u32 = s[5..7].parse().unwrap_or(0);
    let day: u32 = s[8..10].parse().unwrap_or(0);

    if month == 0 || month > 12 || day == 0 {
        return false;
    }

    let days_in_month = days_in_month(year, month);
    day <= days_in_month
}

/// Returns `true` for strings that are both `YYYY-MM-DDThh:mm:ss` formatted
/// **and** represent a valid calendar date plus a real wall-clock time
/// (hour 0–23, minute and second 0–59). The accepted grammar is intentionally
/// strict: no `Z` suffix, no timezone offset, no fractional seconds.
///
/// Purely shape-matching strings like `"2026-13-50T25:99:99"` return `false`.
pub fn is_iso8601_datetime(s: &str) -> bool {
    if s.len() != 19 {
        return false;
    }
    let b = s.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return false;
    }
    if !b[..4].iter().all(u8::is_ascii_digit)
        || !b[5..7].iter().all(u8::is_ascii_digit)
        || !b[8..10].iter().all(u8::is_ascii_digit)
        || !b[11..13].iter().all(u8::is_ascii_digit)
        || !b[14..16].iter().all(u8::is_ascii_digit)
        || !b[17..19].iter().all(u8::is_ascii_digit)
    {
        return false;
    }

    // Re-use the calendar-date validator for the date portion.
    if !is_iso8601_date(&s[..10]) {
        return false;
    }

    // The digit-check above means these parses succeed in practice; on any
    // unexpected failure, the sentinel 99 falls through to the range check
    // below and is rejected as out-of-range.
    let hour: u32 = s[11..13].parse().unwrap_or(99);
    let minute: u32 = s[14..16].parse().unwrap_or(99);
    let second: u32 = s[17..19].parse().unwrap_or(99);
    hour < 24 && minute < 60 && second < 60
}

/// Returns `true` for RFC 3339 / ISO 8601 timezone-aware datetimes, i.e. a
/// `YYYY-MM-DDThh:mm:ss` wall-clock time **followed by an explicit offset**:
/// either the literal `Z` (UTC / Zulu) or a numeric offset `±hh:mm`.
///
/// Examples that return `true`:
/// - `2026-05-28T14:30:00Z`
/// - `2026-05-28T22:44:47+00:00`
/// - `2026-05-28T22:44:47-05:30`
///
/// A naive datetime with no offset (`2026-05-28T14:30:00`) returns `false` —
/// a tz-aware property must carry a zone, and a naive property must not.
/// Fractional seconds and other RFC 3339 extensions are intentionally not
/// accepted (mirrors the strict grammar of [`is_iso8601_datetime`]).
pub fn is_iso8601_datetime_tz(s: &str) -> bool {
    // Split off the offset suffix, then validate the leading naive datetime.
    let Some(base) = s.get(..19) else {
        return false;
    };
    if !is_iso8601_datetime(base) {
        return false;
    }
    let offset = &s[19..];
    is_valid_tz_offset(offset)
}

/// Validate a timezone offset suffix: either `Z` or `±hh:mm` with hh in 0–23
/// and mm in 0–59.
fn is_valid_tz_offset(offset: &str) -> bool {
    if offset == "Z" {
        return true;
    }
    // Numeric offset: sign, two-digit hour, colon, two-digit minute.
    if offset.len() != 6 {
        return false;
    }
    let b = offset.as_bytes();
    if b[0] != b'+' && b[0] != b'-' {
        return false;
    }
    if b[3] != b':' {
        return false;
    }
    if !b[1..3].iter().all(u8::is_ascii_digit) || !b[4..6].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let hour: u32 = offset[1..3].parse().unwrap_or(99);
    let minute: u32 = offset[4..6].parse().unwrap_or(99);
    hour < 24 && minute < 60
}

/// Returns the number of days in the given month of the given year.
/// Months outside 1–12 return 0.
fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Returns `true` when `year` is a Gregorian leap year.
fn is_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings() {
        assert_eq!(levenshtein("hello", "hello"), 0);
    }

    #[test]
    fn single_insertion() {
        assert_eq!(levenshtein("cat", "cats"), 1);
    }

    #[test]
    fn single_deletion() {
        assert_eq!(levenshtein("cats", "cat"), 1);
    }

    #[test]
    fn single_substitution() {
        assert_eq!(levenshtein("cat", "bat"), 1);
    }

    #[test]
    fn empty_strings() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", "abc"), 3);
    }

    #[test]
    fn iso8601_date_valid() {
        assert!(is_iso8601_date("2024-01-15"));
        assert!(is_iso8601_date("1999-12-31"));
        assert!(is_iso8601_date("2024-02-29")); // leap year
        assert!(is_iso8601_date("2000-02-29")); // 400-year leap
        assert!(is_iso8601_date("2026-12-31")); // last day of year
    }

    #[test]
    fn iso8601_datetime_valid() {
        assert!(is_iso8601_datetime("2026-06-04T14:30:00"));
        assert!(is_iso8601_datetime("2024-02-29T23:59:59")); // leap year + end of day
        assert!(is_iso8601_datetime("1970-01-01T00:00:00")); // epoch
    }

    #[test]
    fn iso8601_datetime_invalid() {
        assert!(!is_iso8601_datetime(""));
        assert!(!is_iso8601_datetime("2026-06-04")); // no time part
        assert!(!is_iso8601_datetime("2026-06-04T14:30")); // missing seconds
        assert!(!is_iso8601_datetime("2026-06-04 14:30:00")); // space, not T
        assert!(!is_iso8601_datetime("2026-06-04T14:30:00Z")); // Z suffix rejected
        assert!(!is_iso8601_datetime("2026-06-04T14:30:00+02:00")); // offset rejected
        assert!(!is_iso8601_datetime("2026-13-04T14:30:00")); // bad month
        assert!(!is_iso8601_datetime("2026-02-30T14:30:00")); // bad day
        assert!(!is_iso8601_datetime("2026-06-04T24:00:00")); // hour 24
        assert!(!is_iso8601_datetime("2026-06-04T14:60:00")); // minute 60
        assert!(!is_iso8601_datetime("2026-06-04T14:30:60")); // second 60
        assert!(!is_iso8601_datetime("2023-02-29T00:00:00")); // 2023 not a leap year
    }

    #[test]
    fn iso8601_datetime_tz_valid() {
        // Z / Zulu suffix (blog-example spelling, unquoted YAML)
        assert!(is_iso8601_datetime_tz("2026-05-28T14:30:00Z"));
        // Numeric offsets (sample-bundle spelling)
        assert!(is_iso8601_datetime_tz("2026-05-28T22:44:47+00:00"));
        assert!(is_iso8601_datetime_tz("2026-05-28T22:44:47-05:30"));
        assert!(is_iso8601_datetime_tz("2026-05-28T22:44:47+14:00"));
        assert!(is_iso8601_datetime_tz("2024-02-29T23:59:59Z")); // leap year
    }

    #[test]
    fn iso8601_datetime_tz_invalid() {
        assert!(!is_iso8601_datetime_tz(""));
        assert!(!is_iso8601_datetime_tz("2026-05-28T14:30:00")); // naive: no offset
        assert!(!is_iso8601_datetime_tz("2026-05-28")); // date only
        assert!(!is_iso8601_datetime_tz("2026-05-28T14:30:00z")); // lowercase z
        assert!(!is_iso8601_datetime_tz("2026-05-28T14:30:00+2:00")); // hour not padded
        assert!(!is_iso8601_datetime_tz("2026-05-28T14:30:00+02")); // missing minutes
        assert!(!is_iso8601_datetime_tz("2026-05-28T14:30:00+0200")); // missing colon
        assert!(!is_iso8601_datetime_tz("2026-05-28T14:30:00+25:00")); // offset hour 25
        assert!(!is_iso8601_datetime_tz("2026-05-28T14:30:00+02:60")); // offset minute 60
        assert!(!is_iso8601_datetime_tz("2026-13-28T14:30:00Z")); // bad month
        assert!(!is_iso8601_datetime_tz("2026-05-28T24:00:00Z")); // hour 24
        assert!(!is_iso8601_datetime_tz("garbage"));
    }

    #[test]
    fn naive_and_tz_datetime_are_disjoint() {
        // A naive datetime is never tz-aware, and vice-versa.
        assert!(is_iso8601_datetime("2026-05-28T14:30:00"));
        assert!(!is_iso8601_datetime_tz("2026-05-28T14:30:00"));
        assert!(is_iso8601_datetime_tz("2026-05-28T14:30:00Z"));
        assert!(!is_iso8601_datetime("2026-05-28T14:30:00Z"));
    }

    #[test]
    fn iso8601_date_invalid() {
        assert!(!is_iso8601_date("2024-1-15")); // month not zero-padded
        assert!(!is_iso8601_date("20240115")); // no separators
        assert!(!is_iso8601_date("2024/01/15")); // wrong separator
        assert!(!is_iso8601_date(""));
        assert!(!is_iso8601_date("not-a-date"));
        // Calendar validation
        assert!(!is_iso8601_date("2026-13-50")); // month 13 is invalid
        assert!(!is_iso8601_date("2026-02-30")); // Feb 30 never exists
        assert!(!is_iso8601_date("2023-02-29")); // 2023 is not a leap year
        assert!(!is_iso8601_date("1900-02-29")); // 1900 is not a leap year (div 100 rule)
        assert!(!is_iso8601_date("2026-00-01")); // month 0
        assert!(!is_iso8601_date("2026-01-00")); // day 0
        assert!(!is_iso8601_date("2026-04-31")); // April has 30 days
        assert!(!is_iso8601_date("0000-00-00")); // all zeros
    }
}
