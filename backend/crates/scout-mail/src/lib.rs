//! Template renderer, linter and IMAP drafts.
//!
//! Nothing here can send mail: `lettre` is built without a transport and the
//! only IMAP write is an append to the Drafts folder.

mod compose;
mod imap;
mod lint;

pub use compose::{Email, Slots, build_message, render};
pub use imap::Gmail;
pub use lint::{lint, unsupported_facts};
