use std::collections::HashSet;

/// Content deduplicator using Blake3 hashing.
///
/// Normalizes content before hashing to detect near-identical pages
/// that differ only in whitespace or blank lines.
pub struct ContentDeduplicator {
    seen: HashSet<[u8; 32]>,
}

impl ContentDeduplicator {
    pub fn new() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }

    /// Returns `true` if this content has been seen before.
    pub fn is_duplicate(&mut self, content: &str) -> bool {
        let normalized = normalize_content(content);
        let hash = blake3::hash(normalized.as_bytes());
        !self.seen.insert(*hash.as_bytes())
    }

    /// Number of unique content hashes seen so far.
    pub fn seen_count(&self) -> usize {
        self.seen.len()
    }
}

impl Default for ContentDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize content for dedup comparison:
/// - Trim each line
/// - Collapse 3+ consecutive newlines into 2
/// - Trim the overall string
fn normalize_content(content: &str) -> String {
    let trimmed_lines: String = content
        .lines()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join("\n");

    // Collapse 3+ newlines into 2
    let mut result = String::with_capacity(trimmed_lines.len());
    let mut consecutive_newlines = 0u32;

    for ch in trimmed_lines.chars() {
        if ch == '\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                result.push(ch);
            }
        } else {
            consecutive_newlines = 0;
            result.push(ch);
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_content_is_duplicate() {
        let mut dedup = ContentDeduplicator::new();
        assert!(!dedup.is_duplicate("Hello, world!"));
        assert!(dedup.is_duplicate("Hello, world!"));
    }

    #[test]
    fn test_different_content_is_not_duplicate() {
        let mut dedup = ContentDeduplicator::new();
        assert!(!dedup.is_duplicate("Hello, world!"));
        assert!(!dedup.is_duplicate("Goodbye, world!"));
    }

    #[test]
    fn test_whitespace_normalization() {
        let mut dedup = ContentDeduplicator::new();
        assert!(!dedup.is_duplicate("  Hello  \n  World  "));
        // Same content with different whitespace should be duplicate
        assert!(dedup.is_duplicate("Hello\nWorld"));
    }

    #[test]
    fn test_blank_lines_collapsed() {
        let mut dedup = ContentDeduplicator::new();
        assert!(!dedup.is_duplicate("Hello\n\nWorld"));
        // 3+ blank lines collapsed to 2 should match
        assert!(dedup.is_duplicate("Hello\n\n\n\n\nWorld"));
    }

    #[test]
    fn test_empty_content() {
        let mut dedup = ContentDeduplicator::new();
        assert!(!dedup.is_duplicate(""));
        assert!(dedup.is_duplicate(""));
        assert!(dedup.is_duplicate("   \n  \n  "));
    }

    #[test]
    fn test_seen_count() {
        let mut dedup = ContentDeduplicator::new();
        assert_eq!(dedup.seen_count(), 0);
        dedup.is_duplicate("Page 1");
        assert_eq!(dedup.seen_count(), 1);
        dedup.is_duplicate("Page 2");
        assert_eq!(dedup.seen_count(), 2);
        dedup.is_duplicate("Page 1"); // duplicate
        assert_eq!(dedup.seen_count(), 2);
    }
}
