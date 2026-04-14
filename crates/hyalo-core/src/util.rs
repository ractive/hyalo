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

/// Returns `true` for YYYY-MM-DD formatted dates.
pub fn is_iso8601_date(s: &str) -> bool {
    if s.len() != 10 {
        return false;
    }
    let b = s.as_bytes();
    b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit)
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
    }

    #[test]
    fn iso8601_date_invalid() {
        assert!(!is_iso8601_date("2024-1-15")); // month not zero-padded
        assert!(!is_iso8601_date("20240115")); // no separators
        assert!(!is_iso8601_date("2024/01/15")); // wrong separator
        assert!(!is_iso8601_date(""));
        assert!(!is_iso8601_date("not-a-date"));
    }
}
