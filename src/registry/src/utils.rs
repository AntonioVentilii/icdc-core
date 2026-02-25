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
pub fn canonical_id_part(s: &str) -> String {
    let trimmed = s.trim().to_ascii_uppercase();

    if trimmed.chars().any(|c| c.is_whitespace()) {
        ic_cdk::trap("Identifiers must not contain whitespace");
    }

    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        ic_cdk::trap("Identifier contains invalid characters");
    }

    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_id_part_valid() {
        assert_eq!(canonical_id_part("btc"), "BTC");
        assert_eq!(canonical_id_part("  eth  "), "ETH");
        assert_eq!(canonical_id_part("icp-123"), "ICP-123");
        assert_eq!(canonical_id_part("SOL_USD"), "SOL_USD");
    }

    #[test]
    #[should_panic(expected = "trap should only be called inside canisters.")]
    fn test_canonical_id_part_whitespace() {
        canonical_id_part("btc usd");
    }

    #[test]
    #[should_panic(expected = "trap should only be called inside canisters.")]
    fn test_canonical_id_part_invalid_chars() {
        canonical_id_part("btc@usd");
    }
}
