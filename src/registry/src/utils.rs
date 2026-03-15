use ic_cdk::trap;

/// Normalises an identifier part by trimming, converting to uppercase, and validating characters.
///
/// # Arguments
/// * `s` - The string to normalise.
///
/// # Returns
/// * A normalised uppercase string.
///
/// # Panics
/// * Traps if the identifier contains whitespace or invalid characters.
#[must_use]
pub fn canonical_id_part(s: &str) -> String {
    let trimmed = s.trim().to_ascii_uppercase();

    if trimmed.chars().any(char::is_whitespace) {
        trap("Identifiers must not contain whitespace");
    }

    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        trap("Identifier contains invalid characters");
    }

    trimmed
}

#[cfg(test)]
mod tests {
    use std::panic::catch_unwind;

    use super::*;

    #[test]
    fn canonical_id_part_valid() {
        assert_eq!(canonical_id_part("btc"), "BTC");
        assert_eq!(canonical_id_part("  eth  "), "ETH");
        assert_eq!(canonical_id_part("icp-123"), "ICP-123");
        assert_eq!(canonical_id_part("SOL_USD"), "SOL_USD");
    }

    #[test]
    fn canonical_id_part_whitespace() {
        let result = catch_unwind(|| canonical_id_part("btc usd"));
        assert!(result.is_err());
    }

    #[test]
    fn canonical_id_part_invalid_chars() {
        let result = catch_unwind(|| canonical_id_part("btc@usd"));
        assert!(result.is_err());
    }
}
