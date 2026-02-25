/// Represents an amount of an asset to be transferred or processed.
pub enum AssetAmount {
    /// A specific fixed number of tokens.
    Fixed(u128),
    /// A placeholder for deducting the entire available balance (usage depends on context).
    DeductAll,
}
