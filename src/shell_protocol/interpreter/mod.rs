pub mod bash;
pub mod pwsh;
pub mod zsh;

pub use bash::interpret as interpret_bash;
pub use pwsh::interpret as interpret_pwsh;
pub use zsh::interpret as interpret_zsh;
