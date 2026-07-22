//! Reading Claude Code's OAuth credentials from the macOS Keychain.
//!
//! The `security-framework` crate is the hidden adapter behind `read`'s one-fn
//! interface; a CLI shell-out could replace it without callers noticing.

pub mod read;
