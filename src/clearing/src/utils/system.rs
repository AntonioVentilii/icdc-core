use candid::Principal;

/// Returns the current time in nanoseconds.
///
/// In a canister environment, this uses `ic_cdk::api::time()`.
/// In a test environment, it returns 0 to avoid panics.
pub fn now_ns() -> u64 {
    #[cfg(not(test))]
    {
        ic_cdk::api::time()
    }
    #[cfg(test)]
    {
        0
    }
}

/// Returns the current canister ID.
///
/// In a canister environment, this uses `ic_cdk::id()`.
/// In a test environment, it returns `Principal::anonymous()` to avoid panics.
pub fn canister_id() -> Principal {
    #[cfg(not(test))]
    {
        ic_cdk::id()
    }
    #[cfg(test)]
    {
        Principal::anonymous()
    }
}
