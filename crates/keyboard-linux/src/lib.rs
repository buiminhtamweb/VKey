pub mod backend;
pub mod injector;
pub mod x11;
pub mod x11_injector;

pub use backend::{KeyboardBackend, KeyboardDecision, KeyboardError, Result};
pub use injector::{TextInjector, WindowId, decision_for, execute_engine_action};
pub use x11::X11KeyboardBackend;
pub use x11_injector::X11TextInjector;
