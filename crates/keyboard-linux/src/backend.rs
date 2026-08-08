use thiserror::Error;
use vietnamese_core::{EngineAction, KeyEvent};

pub type Result<T> = std::result::Result<T, KeyboardError>;

#[derive(Debug, Error)]
pub enum KeyboardError {
    #[error("DISPLAY is not set; failed to connect to X11 display")]
    MissingDisplay,
    #[error("X11 keyboard backend is only supported on Linux")]
    UnsupportedPlatform,
    #[error("failed to connect to X11 display: {0}")]
    X11Connection(String),
    #[error("X11 protocol error: {0}")]
    X11Protocol(String),
    #[error("XInput2 is unavailable: {0}")]
    XInputUnavailable(String),
    #[error("XKB is unavailable: {0}")]
    XkbUnavailable(String),
    #[error("keyboard backend is not running")]
    NotRunning,
    #[error("the previous captured key event has not been decided")]
    PendingDecision,
    #[error("there is no captured key event awaiting a decision")]
    NoPendingDecision,
    #[error("XTEST is unavailable: {0}")]
    XTestUnavailable(String),
    #[error("X11 has no unused keycode available for Unicode injection")]
    NoSpareKeycode,
    #[error("there is no focused X11 window")]
    NoFocusedWindow,
    #[error("X11 focus changed from window {expected:#x} to {actual:#x} during injection")]
    FocusChanged { expected: u32, actual: u32 },
    #[error("X11 connection lost: {0}")]
    ConnectionLost(String),
}

pub trait KeyboardBackend {
    fn start(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn next_event(&mut self) -> Result<KeyEvent>;
    fn decide(&mut self, decision: KeyboardDecision) -> Result<()>;
    fn is_running(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardDecision {
    PassThrough,
    Consume,
}

impl KeyboardDecision {
    #[must_use]
    pub const fn from_engine_action(action: &EngineAction) -> Self {
        match action {
            EngineAction::PassThrough => Self::PassThrough,
            EngineAction::Consume | EngineAction::Commit(_) | EngineAction::Replace { .. } => {
                Self::Consume
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_actions_map_to_future_backend_decisions() {
        assert_eq!(
            KeyboardDecision::from_engine_action(&EngineAction::PassThrough),
            KeyboardDecision::PassThrough
        );
        assert_eq!(
            KeyboardDecision::from_engine_action(&EngineAction::Consume),
            KeyboardDecision::Consume
        );
        assert_eq!(
            KeyboardDecision::from_engine_action(&EngineAction::Replace {
                delete_graphemes: 1,
                text: "ê".to_owned(),
            }),
            KeyboardDecision::Consume
        );
    }
}
