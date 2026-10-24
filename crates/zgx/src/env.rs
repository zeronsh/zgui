//! The environment variables zgx reads (`ZGX_*`). Each is read under its
//! name from before the rename to zgx too, for one release: when the new
//! name is unset, the old one's value, with a warning (once a name).
use std::ffi::OsString;
use std::sync::{Mutex, PoisonError};

/// `std::env::var_os(key)`, or for a `ZGX_` key unset its deprecated name's.
pub fn var_os(key: &str) -> Option<OsString> {
    std::env::var_os(key).or_else(|| {
        let old = format!("ZGUI_{}", key.strip_prefix("ZGX_")?); // rename-to-zgx: keep
        let value = std::env::var_os(&old)?;
        static WARNED: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let mut warned = WARNED.lock().unwrap_or_else(PoisonError::into_inner);
        if !warned.contains(&old) {
            eprintln!("zgx: {old} is deprecated; set {key} instead");
            warned.push(old);
        }
        Some(value)
    })
}

/// `std::env::var(key)`, or for a `ZGX_` key unset its deprecated name's.
pub fn var(key: &str) -> Result<String, std::env::VarError> {
    var_os(key)
        .ok_or(std::env::VarError::NotPresent)?
        .into_string()
        .map_err(std::env::VarError::NotUnicode)
}
