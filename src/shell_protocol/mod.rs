//! Shell protocol — out-of-band communication between `mate` and its shell wrappers.
//!
//! # Overview
//!
//! When `mate` is invoked through a shell wrapper (e.g. the function installed by
//! `mate init zsh`), it needs to instruct the *shell* to perform actions that a
//! child process cannot do by itself — most notably changing the shell's working
//! directory.  A plain exit code is not enough, and writing instructions to stderr
//! would pollute error output visible to the user.
//!
//! The shell protocol solves this by writing structured messages directly to a
//! **protocol file** whose path is passed via the `GIT_MATE_PROTO` environment
//! variable.  The shell wrapper creates a temporary file, sets `GIT_MATE_PROTO` to
//! its path, then invokes `mate` normally — stdout and stdin remain connected to
//! the terminal.  After `mate` exits, the wrapper runs `mate _protocol interpret`
//! to turn the collected messages into shell statements and `eval`s them.
//!
//! # Wire format
//!
//! Every line in the protocol file is a bare message, e.g.:
//!
//! ```text
//! CD:<path>
//! ```
//!
//! | Message     | Meaning                           |
//! |-------------|-----------------------------------|
//! | `CD:<path>` | Ask the shell to `cd` to `<path>` |
//!
//! # Activation
//!
//! Protocol messages are only written when the `GIT_MATE_PROTO` environment variable
//! is set to a writable file path.  Direct invocations (without the wrapper) never
//! produce protocol output.

pub mod emit;
pub mod interpreter;
pub mod message;

pub use emit::emit_cd;
pub use message::Message;
