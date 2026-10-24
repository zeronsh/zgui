//! Delegation to the user's registered URL handler without blocking the UI.
use std::{fmt, io};
/// Why a URL could not be handed to its handler.
#[derive(Debug)]
pub enum OpenUrlError {
    /// The URL has no valid scheme, or holds control characters.
    InvalidUrl,
    /// The system's URL launcher failed.
    Launcher(io::Error),
    /// The background worker that launches URLs is gone.
    WorkerUnavailable,
}
impl fmt::Display for OpenUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl => {
                f.write_str("URL must have a valid scheme and contain no control characters")
            }
            Self::Launcher(error) => write!(f, "could not launch URL handler: {error}"),
            Self::WorkerUnavailable => f.write_str("URL launcher worker unavailable"),
        }
    }
}
impl std::error::Error for OpenUrlError {}
pub(crate) fn valid_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once(':') else {
        return false;
    };
    !rest.is_empty()
        && scheme
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
        && scheme
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
        && !url.chars().any(char::is_control)
}
/// Ask the OS to open a URL using the user's registered handler. Success means
/// the launcher accepted the request, not that a remote page loaded successfully.
/// No shell command is formed from the URL. Invoke in response to a user action.
pub async fn open_url(url: impl Into<String>) -> Result<(), OpenUrlError> {
    let url = url.into();
    if !valid_url(&url) {
        return Err(OpenUrlError::InvalidUrl);
    }
    crate::file_dialog::background(move || open::that(url))
        .map_err(|_| OpenUrlError::WorkerUnavailable)?
        .await
        .map_err(|_| OpenUrlError::WorkerUnavailable)?
        .map_err(OpenUrlError::Launcher)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls_require_schemes_and_reject_command_options_and_controls() {
        for url in [
            "https://example.org/path?q=a",
            "mailto:person@example.org",
            "zgx-demo:document",
        ] {
            assert!(valid_url(url));
        }
        for url in [
            "--help",
            "/tmp/file",
            "9invalid:thing",
            "https:",
            "https://example.org\nextra",
        ] {
            assert!(!valid_url(url));
        }
    }
}
