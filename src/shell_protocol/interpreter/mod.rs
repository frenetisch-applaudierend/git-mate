pub mod bash;
pub mod zsh;

pub use bash::interpret as interpret_bash;
pub use zsh::interpret as interpret_zsh;
