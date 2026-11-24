//! Seeds as the game's world-creation screen reads them.

/// A number is that seed; any other text is its Java `String.hashCode`.
pub fn parse(text: &str) -> Option<i64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(text.parse().unwrap_or_else(|_| java_hash(text)))
}

fn java_hash(text: &str) -> i64 {
    let hash = text
        .encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)));
    i64::from(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_text() {
        assert_eq!(parse("-4172144997902289642"), Some(-4172144997902289642));
        assert_eq!(parse("+12"), Some(12));
        // "hello".hashCode() in Java.
        assert_eq!(parse("hello"), Some(99162322));
        assert_eq!(parse("   "), None);
    }
}
