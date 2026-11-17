//! What a save before the end came to.
//!
//! Pattern: Value object (portable, so the failure types exist on every system).

/// What a save before the end came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
    /// The program has nothing to save, or cannot be asked (no heartbeat channel).
    Nothing,
    /// The program saved its drafts and its session.
    Saved,
    /// The hook failed.
    Failed,
}

impl Saved {
    /// The stable token (`nothing`, `saved`, `failed`): the parameter `saved` of the message and a word of the log.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::Saved => "saved",
            Self::Failed => "failed",
        }
    }
}
