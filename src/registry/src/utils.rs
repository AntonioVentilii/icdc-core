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
