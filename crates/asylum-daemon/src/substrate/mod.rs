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

/// Down-arrow key sequence (ESC `[` `B`) claude's TUI consumes to move the menu
/// highlight down one option. Verified against claude 2.1.207 (2026-07-13).
pub const MENU_DOWN_ARROW: &[u8] = b"\x1b[B";
/// Carriage return that submits the highlighted menu option (verified as a
/// distinct keystroke, mirroring the text-submit contract).
pub const MENU_SUBMIT: &[u8] = b"\r";

/// The ordered list of DISTINCT PTY writes that select the option at 0-based
/// `option_index` in a claude AskUserQuestion single-select menu. The menu opens
/// with the first option highlighted, so selecting index N means N down-arrow
/// presses followed by a lone carriage return. Each element is delivered as its
/// own paced write: claude's TUI coalesces a bundled burst as a paste and the CR
/// only submits when it arrives as its own keystroke (verified live 2026-07-13 —
/// two raw `\x1b[B` writes + `\r` selected option 3 of 3, a non-default choice).
/// `option_index == 0` yields a lone CR, which legitimately selects the first
/// option (the human's choice), not a silent Enter-takes-default fallback.
pub fn menu_selection_writes(option_index: usize) -> Vec<Vec<u8>> {
    let mut writes = Vec::with_capacity(option_index + 1);
    for _ in 0..option_index {
        writes.push(MENU_DOWN_ARROW.to_vec());
    }
    writes.push(MENU_SUBMIT.to_vec());
    writes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_selection_writes_encodes_down_arrows_then_cr() {
        // First option: no navigation, just submit.
        assert_eq!(menu_selection_writes(0), vec![b"\r".to_vec()]);
        // Second option (non-default): one down arrow, then CR, as distinct writes.
        assert_eq!(
            menu_selection_writes(1),
            vec![b"\x1b[B".to_vec(), b"\r".to_vec()]
        );
        // Third option: two down arrows, then CR.
        assert_eq!(
            menu_selection_writes(2),
            vec![b"\x1b[B".to_vec(), b"\x1b[B".to_vec(), b"\r".to_vec()]
        );
        // Every non-final write is a down arrow; the final write is the submit.
        let writes = menu_selection_writes(5);
        assert_eq!(writes.len(), 6);
        assert!(writes[..5].iter().all(|w| w.as_slice() == MENU_DOWN_ARROW));
        assert_eq!(writes[5].as_slice(), MENU_SUBMIT);
    }
}
