use uuid::Uuid;

use asylum_types::node::HarnessKind;

#[derive(Clone)]
pub struct SubstrateContext {
    pub node_id: Uuid,
    pub harness: HarnessKind,
    pub command: String,
    pub args: Vec<String>,
    pub workspace: Option<String>,
    pub env: Vec<(String, String)>,
    /// The initial prompt to deliver to the harness once its interactive TUI is
    /// ready. Delivered over the PTY as a submitted message (see
    /// `LocalSubstrate::launch`) rather than as a trailing positional argv:
    /// interactive harnesses (claude, codex) pre-fill a positional prompt into
    /// the input box without submitting it, leaving the session idle. `None`
    /// skips delivery (e.g. an empty prompt).
    pub launch_prompt: Option<String>,
}

pub trait SubstrateOutput: Fn(Uuid, &str) + Send + Sync {}
impl<T> SubstrateOutput for T where T: Fn(Uuid, &str) + Send + Sync {}

pub mod local;
pub mod loon;

pub use local::{ExitOutcome, LocalSubstrate};
pub use loon::LoonSubstrate;
