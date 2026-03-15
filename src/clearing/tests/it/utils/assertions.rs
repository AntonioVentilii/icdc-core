use core::fmt::{Debug, Display};

use shared::types::DecimalValue;

/// Asserts that a Result is an Err containing a specific message substring.
#[expect(dead_code)]
pub fn assert_err_msg<T, E: Display>(result: &Result<T, E>, msg: &str) {
    match result {
        Ok(_) => panic!("Expected error containing '{msg}', but got Ok"),
        Err(e) => {
            let error_str = e.to_string();
            assert!(
                error_str.contains(msg),
                "Error message '{error_str}' does not contain '{msg}'"
            );
        }
    }
}

/// Asserts that a Result is an Err specifically indicating a guard rejection (unauthorized).
pub fn assert_unauthorized<T, E: Display>(result: &Result<T, E>) {
    match result {
        Ok(_) => panic!("Expected unauthorized error, but got Ok"),
        Err(e) => {
            let error_str = e.to_string().to_lowercase();
            let patterns = [
                "is not authorized",
                "is not a controller",
                "unauthorized",
                "caller is not",
                "not authorised",
                "not authorized",
                "unauthorised",
            ];
            let matches = patterns.iter().any(|p| error_str.contains(p));
            assert!(
                matches,
                "Error message '{error_str}' does not look like an authorization error"
            );
        }
    }
}

/// Extracts the value from a Result<T, String> or panics with the error message.
pub fn assert_ok_value<T: Debug>(result: Result<T, String>) -> T {
    match result {
        Ok(val) => val,
        Err(e) => panic!("Expected Ok, but got Err: {e}"),
    }
}

/// Asserts that two `DecimalValues` are equal with a clear error message.
pub fn assert_decimal_eq(actual: &DecimalValue, expected_val: u128, expected_dec: u8) {
    assert_eq!(
        actual.value, expected_val,
        "Decimal value mismatch: expected {}, got {}",
        expected_val, actual.value
    );
    assert_eq!(
        actual.decimals, expected_dec,
        "Decimal precision mismatch: expected {}, got {}",
        expected_dec, actual.decimals
    );
}

#[macro_export]
macro_rules! assert_ok {
    ($res:expr) => {
        match $res {
            Ok(val) => val,
            Err(e) => panic!("Assertion failed: expected Ok, got Err: {:?}", e),
        }
    };
}

#[macro_export]
macro_rules! assert_matches {
    ($res:expr, $p:pat $(if $guard:expr)? $(,)?) => {
        match $res {
            $p $(if $guard)? => (),
            _ => panic!("Assertion failed: value does not match pattern"),
        }
    };
}
