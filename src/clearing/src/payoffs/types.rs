/// Defines how to handle fractional values during decimal scaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoundingMode {
    /// Round down (toward zero). Used for payouts.
    Floor,
    /// Round up (away from zero). Used for margin/collateral.
    Ceil,
}
