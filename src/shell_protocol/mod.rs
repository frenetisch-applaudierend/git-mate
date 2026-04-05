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
//! The shell protocol solves this by embedding structured messages in the
//! command's **stdout** stream.  The shell wrapper pipes that stream through
//! `mate _protocol collect`, which strips the protocol lines and writes them to a
//! temporary file, while forwarding all other output directly to the terminal.
//! After the command exits, the wrapper runs `mate _protocol interpret` to turn
//! the collected messages into shell statements and `eval`s them.
//!
//! # Wire format
//!
//! Every protocol line on stdout looks like:
//!
//! ```text
//! _MATE_CMD:<MESSAGE>
//! ```
//!
//! where `<MESSAGE>` is one of:
//!
//! | Message        | Meaning                                      |
//! |----------------|----------------------------------------------|
//! | `CD:<path>`    | Ask the shell to `cd` to `<path>`            |
//!
//! `collect` strips the `_MATE_CMD:` prefix and writes bare messages (e.g.
//! `CD:/some/path`) to the protocol file, one per line.  `interpret` reads that
//! file and emits the corresponding shell statements.
//!
//! # Activation
//!
//! Protocol output is only emitted when the `GIT_MATE_SHELL` environment variable
//! is set (to any value).  The shell wrapper sets this variable before invoking
//! `mate`, so direct invocations never emit protocol lines.

pub mod collect;
pub mod emit;
pub mod interpreter;
pub mod message;

pub use emit::emit_cd;
pub use message::Message;
