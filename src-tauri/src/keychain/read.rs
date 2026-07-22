//! Read the `Claude Code-credentials` Keychain item and defensively parse the
//! `claudeAiOauth` blob.
//!
//! We only ever READ (Decision #4 — never refresh, never write). The parsed
//! `Credentials` carries the access token to the fetch layer; it is never logged.

use serde::Deserialize;

const SERVICE: &str = "Claude Code-credentials";

/// errSecItemNotFound — the Keychain item does not exist.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

/// The credentials the app needs. The token is used only as a request header.
#[derive(Debug, Clone)]
pub struct Credentials {
    pub access_token: String,
    /// Epoch millis the token expires. Part of the credentials contract; not yet
    /// consumed (reserved for the later liveness/delegated-refresh slice).
    #[allow(dead_code)]
    pub expires_at: Option<i64>,
    pub subscription_type: Option<String>,
}

/// Why a Keychain read failed. Every variant maps to a Blind Snapshot upstream.
#[derive(Debug)]
pub enum KeychainError {
    /// The item (or the `$USER` account) was not found.
    NotFound,
    /// The user denied the confidential-info prompt, or access was refused.
    Denied,
    /// The blob was present but not the JSON shape we expected. The inner detail
    /// is retained for future surfacing; the poll loop currently maps via `Err(_)`.
    #[allow(dead_code)]
    Parse(String),
}

/// The Keychain blob's outer shape. The credential is nested under
/// `claudeAiOauth`; the outer object may carry other keys we ignore.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Blob {
    claude_ai_oauth: OauthBlob,
}

/// The nested OAuth object. Only `accessToken` is required; the rest are
/// best-effort (the shape varies by Claude Code version — parse defensively).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OauthBlob {
    access_token: String,
    #[serde(default)]
    expires_at: Option<i64>,
    #[serde(default)]
    subscription_type: Option<String>,
}

/// Read + parse the Claude Code credentials from the login Keychain.
///
/// The account is `$USER` (per the on-machine item). Any Keychain-level failure
/// maps to `NotFound`/`Denied`; a present-but-unexpected blob maps to `Parse`.
pub fn read_claude_credentials() -> Result<Credentials, KeychainError> {
    let account = std::env::var("USER").map_err(|_| KeychainError::NotFound)?;

    let bytes = security_framework::passwords::get_generic_password(SERVICE, &account)
        .map_err(map_keychain_error)?;

    parse_credentials(&bytes)
}

/// Defensively parse the raw Keychain bytes into `Credentials`.
fn parse_credentials(bytes: &[u8]) -> Result<Credentials, KeychainError> {
    let blob: Blob =
        serde_json::from_slice(bytes).map_err(|e| KeychainError::Parse(e.to_string()))?;

    Ok(Credentials {
        access_token: blob.claude_ai_oauth.access_token,
        expires_at: blob.claude_ai_oauth.expires_at,
        subscription_type: blob.claude_ai_oauth.subscription_type,
    })
}

/// Map a Security.framework OSStatus to our coarse error. Not-found is distinct;
/// everything else (user-cancel -128, authFailed -25293, …) is treated as denied.
fn map_keychain_error(error: security_framework::base::Error) -> KeychainError {
    match error.code() {
        ERR_SEC_ITEM_NOT_FOUND => KeychainError::NotFound,
        _ => KeychainError::Denied,
    }
}
