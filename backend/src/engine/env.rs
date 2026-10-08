//! The `FELLA_*` runtime knobs are power-user / test escape hatches, and every
//! one of them is the same rule: parse a positive integer from the environment
//! or fall back to the built-in default.

/// `$key` parsed as `T`, ignoring a missing, non-numeric, or non-positive value.
pub fn positive<T>(key: &str, default: T) -> T
where
    T: std::str::FromStr + Default + PartialOrd,
{
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<T>().ok())
        .filter(|n| *n > T::default())
        .unwrap_or(default)
}
