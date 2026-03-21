/// 1 KiB in bytes (1024)
pub const KIB: u64 = 1024;
/// 1 MiB in bytes (1024 * 1024)
pub const MIB: u64 = 1024 * KIB;
/// 1 GiB in bytes (1024 * 1024 * 1024)
pub const GIB: u64 = 1024 * MIB;

/// Number of nanoseconds in one second
pub const SECOND_NS: u64 = 1_000_000_000;
/// Number of nanoseconds in one minute
pub const MINUTE_NS: u64 = 60 * SECOND_NS;
/// Number of nanoseconds in one hour
pub const HOUR_NS: u64 = 60 * MINUTE_NS;
/// Number of nanoseconds in one day
pub const DAY_NS: u64 = 24 * HOUR_NS;
/// Number of nanoseconds in 30 days
pub const MONTH_NS: u64 = 30 * DAY_NS;

/// The canonical number of decimals for internal USD accounting.
pub const USD_DECIMALS: u8 = 4;

/// Maximum length of a series title in characters.
pub const MAX_SERIES_TITLE_LEN: usize = 128;
/// Maximum length of a series description in characters.
pub const MAX_SERIES_DESCRIPTION_LEN: usize = 1024;

/// Default insurance fund fee ratio in basis points (10 bps = 0.1%).
pub const DEFAULT_INSURANCE_FEE_RATIO: u16 = 10;
/// Default protocol fee ratio in basis points (5 bps = 0.05%).
pub const DEFAULT_PROTOCOL_FEE_RATIO: u16 = 5;

/// The base for basis points (100% = 10,000 bps).
pub const BPS_BASE: u16 = 10_000;
