/// Implements `From<Result<T, E>>` for a Candid result enum whose variants are
/// `Ok(T)` and `Err(E)`, so an endpoint can build its typed result with
/// `.into()`. Pass `()` as `T` for an enum whose `Ok` variant carries nothing.
///
/// A result that has to transform the value (box it, format the error) keeps a
/// hand-written `impl`.
#[macro_export]
macro_rules! impl_from_result {
    ($result:ty, (), $err:ty) => {
        impl From<Result<(), $err>> for $result {
            fn from(value: Result<(), $err>) -> Self {
                match value {
                    Ok(()) => Self::Ok,
                    Err(e) => Self::Err(e),
                }
            }
        }
    };
    ($result:ty, $ok:ty, $err:ty) => {
        impl From<Result<$ok, $err>> for $result {
            fn from(value: Result<$ok, $err>) -> Self {
                match value {
                    Ok(v) => Self::Ok(v),
                    Err(e) => Self::Err(e),
                }
            }
        }
    };
}
