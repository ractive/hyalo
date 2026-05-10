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
