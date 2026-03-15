use candid::Principal;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
use ic_cdk::{api::time, id};

/// Returns the current time in nanoseconds.
///
/// On the IC, this uses `ic_cdk::api::time()`.
/// Off-chain (for tests / host builds), it returns 0.
pub fn now_ns() -> u64 {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        time()
    }

    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        0
    }
}

/// Returns the current canister ID.
///
/// On the IC, this uses `ic_cdk::id()`.
/// Off-chain (for tests / host builds), it returns `Principal::anonymous()`.
pub fn canister_id() -> Principal {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        id()
    }

    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        Principal::anonymous()
    }
}
