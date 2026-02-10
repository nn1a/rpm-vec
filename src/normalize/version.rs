/// RPM version comparison implementation
///
/// Implements the rpmvercmp algorithm used by RPM package manager.
/// Compares versions in epoch:version-release format.
///
/// # Algorithm
///
/// 1. Compare epochs (numeric, higher wins)
/// 2. Compare version strings using segment comparison
/// 3. Compare release strings using segment comparison
///
/// Segment comparison alternates between numeric and alphabetic parts:
/// - Numeric segments compared as integers
/// - Alphabetic segments compared lexicographically
/// - Non-alphanumeric characters act as separators (except tilde)
/// - Tilde (~) has special pre-release semantics:
///   - "1.0~rc1" < "1.0" (pre-release is less than release)
///   - "1.0~alpha" < "1.0~beta" < "1.0"
///   - Tilde sorts before any other character, including end-of-string
///
/// # Examples
///
/// ```
/// use rpm_repo_search::normalize::version::RpmVersion;
/// use std::cmp::Ordering;
///
/// let v1 = RpmVersion::new(None, "1.0".to_string(), "1".to_string());
/// let v2 = RpmVersion::new(None, "2.0".to_string(), "1".to_string());
/// assert_eq!(v1.cmp(&v2), Ordering::Less);
///
/// // Pre-release versions
/// let pre = RpmVersion::new(None, "1.0~rc1".to_string(), "1".to_string());
/// let rel = RpmVersion::new(None, "1.0".to_string(), "1".to_string());
/// assert_eq!(pre.cmp(&rel), Ordering::Less);
/// ```
use std::cmp::Ordering;

/// Represents a parsed RPM version for comparison
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpmVersion {
    pub epoch: i64,
    pub version: String,
    pub release: String,
}

impl RpmVersion {
    /// Create a new RPM version
    pub fn new(epoch: Option<i64>, version: String, release: String) -> Self {
        Self {
            epoch: epoch.unwrap_or(0),
            version,
            release,
        }
    }

    /// Compare two version/release strings using RPM algorithm
    fn compare_segments(a: &str, b: &str) -> Ordering {
        let mut a_chars = a.chars().peekable();
        let mut b_chars = b.chars().peekable();

        loop {
            // Skip non-alphanumeric characters (except tilde)
            while a_chars
                .peek()
                .is_some_and(|c| !c.is_alphanumeric() && *c != '~')
            {
                a_chars.next();
            }
            while b_chars
                .peek()
                .is_some_and(|c| !c.is_alphanumeric() && *c != '~')
            {
                b_chars.next();
            }

            // Handle tilde (~) special case for pre-release versions
            // In RPM, tilde sorts before everything, even end-of-string
            let a_has_tilde = a_chars.peek() == Some(&'~');
            let b_has_tilde = b_chars.peek() == Some(&'~');

            if a_has_tilde && b_has_tilde {
                // Both have tilde, skip it and continue comparison
                a_chars.next();
                b_chars.next();
                continue;
            }
            if a_has_tilde {
                // a has tilde but b doesn't: a < b (pre-release)
                return Ordering::Less;
            }
            if b_has_tilde {
                // b has tilde but a doesn't: a > b
                return Ordering::Greater;
            }

            // Check if we're at the end
            let a_empty = a_chars.peek().is_none();
            let b_empty = b_chars.peek().is_none();

            if a_empty && b_empty {
                return Ordering::Equal;
            }
            if a_empty {
                return Ordering::Less;
            }
            if b_empty {
                return Ordering::Greater;
            }

            // Determine segment types
            let a_is_digit = a_chars.peek().is_some_and(|c| c.is_ascii_digit());
            let b_is_digit = b_chars.peek().is_some_and(|c| c.is_ascii_digit());

            // If one is numeric and the other is not, numeric wins
            if a_is_digit && !b_is_digit {
                return Ordering::Greater;
            }
            if !a_is_digit && b_is_digit {
                return Ordering::Less;
            }

            // Both are same type, compare the segment
            if a_is_digit {
                // Compare numeric segments as integers
                let mut a_num = String::new();
                while let Some(&c) = a_chars.peek() {
                    if c.is_ascii_digit() {
                        a_num.push(c);
                        a_chars.next();
                    } else {
                        break;
                    }
                }

                let mut b_num = String::new();
                while let Some(&c) = b_chars.peek() {
                    if c.is_ascii_digit() {
                        b_num.push(c);
                        b_chars.next();
                    } else {
                        break;
                    }
                }

                // Parse as u64 to handle large numbers
                let a_val = a_num.parse::<u64>().unwrap_or(0);
                let b_val = b_num.parse::<u64>().unwrap_or(0);

                match a_val.cmp(&b_val) {
                    Ordering::Equal => continue,
                    other => return other,
                }
            } else {
                // Compare alphabetic segments lexicographically
                let mut a_str = String::new();
                while let Some(&c) = a_chars.peek() {
                    if c.is_alphanumeric() && !c.is_ascii_digit() {
                        a_str.push(c);
                        a_chars.next();
                    } else {
                        break;
                    }
                }

                let mut b_str = String::new();
                while let Some(&c) = b_chars.peek() {
                    if c.is_alphanumeric() && !c.is_ascii_digit() {
                        b_str.push(c);
                        b_chars.next();
                    } else {
                        break;
                    }
                }

                match a_str.cmp(&b_str) {
                    Ordering::Equal => continue,
                    other => return other,
                }
            }
        }
    }
}

impl PartialOrd for RpmVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RpmVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        // 1. Compare epochs
        match self.epoch.cmp(&other.epoch) {
            Ordering::Equal => {}
            other => return other,
        }

        // 2. Compare versions
        match Self::compare_segments(&self.version, &other.version) {
            Ordering::Equal => {}
            other => return other,
        }

        // 3. Compare releases
        Self::compare_segments(&self.release, &other.release)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_comparison() {
        let v1 = RpmVersion::new(Some(1), "1.0".to_string(), "1".to_string());
        let v2 = RpmVersion::new(Some(2), "1.0".to_string(), "1".to_string());
        assert!(v1 < v2);
    }

    #[test]
    fn test_version_numeric() {
        let v1 = RpmVersion::new(None, "1.0".to_string(), "1".to_string());
        let v2 = RpmVersion::new(None, "2.0".to_string(), "1".to_string());
        assert!(v1 < v2);

        let v3 = RpmVersion::new(None, "1.10".to_string(), "1".to_string());
        let v4 = RpmVersion::new(None, "1.2".to_string(), "1".to_string());
        assert!(v3 > v4); // 10 > 2
    }

    #[test]
    fn test_version_alpha() {
        // Note: In RPM, "1.0a" is parsed as: 1, 0, a
        // and "1.0b" is parsed as: 1, 0, b
        // Alphabetic 'a' < 'b'
        let v1 = RpmVersion::new(None, "1.0a".to_string(), "1".to_string());
        let v2 = RpmVersion::new(None, "1.0b".to_string(), "1".to_string());

        assert!(v1 < v2);
    }

    #[test]
    fn test_release_comparison() {
        let v1 = RpmVersion::new(None, "1.0".to_string(), "1.el9".to_string());
        let v2 = RpmVersion::new(None, "1.0".to_string(), "2.el9".to_string());
        assert!(v1 < v2);
    }

    #[test]
    fn test_numeric_vs_alpha() {
        // Numeric segments have priority over alphabetic
        let v1 = RpmVersion::new(None, "1.0.1".to_string(), "1".to_string());
        let v2 = RpmVersion::new(None, "1.0.a".to_string(), "1".to_string());
        assert!(v1 > v2);
    }

    #[test]
    fn test_real_world_versions() {
        // Common RPM version patterns
        let v1 = RpmVersion::new(None, "2.6.32".to_string(), "279.el6".to_string());
        let v2 = RpmVersion::new(None, "2.6.32".to_string(), "754.el6".to_string());
        assert!(v1 < v2);

        // With epoch
        let v3 = RpmVersion::new(Some(1), "2.6.32".to_string(), "100.el6".to_string());
        let v4 = RpmVersion::new(None, "3.0.0".to_string(), "1.el6".to_string());
        assert!(v3 > v4); // epoch takes precedence
    }

    #[test]
    fn test_tilde_versions() {
        // Versions with tilde (~) are pre-release versions in RPM
        // "1.0~rc1" should be less than "1.0"
        let v1 = RpmVersion::new(None, "1.0~rc1".to_string(), "1".to_string());
        let v2 = RpmVersion::new(None, "1.0".to_string(), "1".to_string());
        assert_eq!(v1.cmp(&v2), Ordering::Less);

        // Multiple pre-release versions
        let v3 = RpmVersion::new(None, "1.0~alpha".to_string(), "1".to_string());
        let v4 = RpmVersion::new(None, "1.0~beta".to_string(), "1".to_string());
        assert_eq!(v3.cmp(&v4), Ordering::Less); // alpha < beta

        // Pre-release vs pre-release
        let v5 = RpmVersion::new(None, "1.0~rc1".to_string(), "1".to_string());
        let v6 = RpmVersion::new(None, "1.0~rc2".to_string(), "1".to_string());
        assert_eq!(v5.cmp(&v6), Ordering::Less); // rc1 < rc2

        // Pre-release with numeric suffix
        let v7 = RpmVersion::new(None, "2.0~1".to_string(), "1".to_string());
        let v8 = RpmVersion::new(None, "2.0~2".to_string(), "1".to_string());
        assert_eq!(v7.cmp(&v8), Ordering::Less); // ~1 < ~2

        // Tilde in release
        let v9 = RpmVersion::new(None, "1.0".to_string(), "1~rc1".to_string());
        let v10 = RpmVersion::new(None, "1.0".to_string(), "1".to_string());
        assert_eq!(v9.cmp(&v10), Ordering::Less);
    }

    #[test]
    fn test_segment_comparison() {
        assert_eq!(RpmVersion::compare_segments("1.0", "1.0"), Ordering::Equal);
        assert_eq!(RpmVersion::compare_segments("1.0", "2.0"), Ordering::Less);
        assert_eq!(
            RpmVersion::compare_segments("2.0", "1.0"),
            Ordering::Greater
        );
        assert_eq!(
            RpmVersion::compare_segments("1.10", "1.2"),
            Ordering::Greater
        );
        assert_eq!(RpmVersion::compare_segments("1a", "1b"), Ordering::Less);
        // Tilde pre-release versions
        assert_eq!(
            RpmVersion::compare_segments("1.0~rc1", "1.0"),
            Ordering::Less
        );
        assert_eq!(
            RpmVersion::compare_segments("1.0~alpha", "1.0~beta"),
            Ordering::Less
        );
        assert_eq!(
            RpmVersion::compare_segments("2.0~1", "2.0~2"),
            Ordering::Less
        );
    }
}
