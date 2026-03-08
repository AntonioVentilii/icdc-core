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

/// Principal ID of the ICP Ledger canister
pub const ICP_LEDGER: &str = "ryjl3-tyaaa-aaaaa-aaaba-cai";
/// Principal ID of the ckUSDC Ledger canister
pub const CKUSDC_LEDGER: &str = "yfumr-cyaaa-aaaar-qaela-cai";
/// Principal ID of the ckUSDT Ledger canister
pub const CKUSDT_LEDGER: &str = "6rdls-viaaa-aaaar-qaelq-cai";
/// Principal ID of the virtual USD (vUSD) Ledger canister (Internal ghost token)
pub const VUSD_LEDGER: &str = "ap6gq-taaaa-aaaae-acsaq-cai";
/// Principal ID of the virtual USD (vUSD) Index canister
pub const VUSD_INDEX: &str = "x3qir-tyaaa-aaaae-acr6a-cai";

/// The canonical number of decimals for internal USD accounting.
pub const USD_DECIMALS: u8 = 6;

/// Maximum length of a series title in characters.
pub const MAX_SERIES_TITLE_LEN: usize = 128;
/// Maximum length of a series description in characters.
pub const MAX_SERIES_DESCRIPTION_LEN: usize = 1024;

/// Default insurance fund fee ratio in basis points (10 bps = 0.1%).
pub const DEFAULT_INSURANCE_FEE_RATIO: u16 = 10;
/// Default protocol fee ratio in basis points (5 bps = 0.05%).
pub const DEFAULT_PROTOCOL_FEE_RATIO: u16 = 5;
