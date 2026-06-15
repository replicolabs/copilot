use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(
        "no Claude credential found: set ANTHROPIC_API_KEY (inherited from the \
         Claude Code session / install) — the agent never prompts for a key"
    )]
    MissingCredential,

    #[error("Anthropic request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Anthropic API returned {status}: {detail}")]
    Api { status: u16, detail: String },

    #[error("Anthropic response contained no text content")]
    EmptyResponse,

    #[error("could not parse a decision from the model output: {detail}")]
    Parse { detail: String, raw: String },

    #[error("reasoning log I/O failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("reasoning log serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
}
