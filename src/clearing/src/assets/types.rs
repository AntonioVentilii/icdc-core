/// Represents an amount of an asset to be transferred or processed.
pub(crate) enum AssetAmount {
    /// A specific fixed number of tokens.
    Fixed(u128),
}
