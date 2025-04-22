#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Token budget exceeded: current {current}, budget {budget}")]
    TokenTooMuch { current: usize, budget: usize },
    #[error("Fatal error: {0}")]
    Fatal(anyhow::Error),
}
