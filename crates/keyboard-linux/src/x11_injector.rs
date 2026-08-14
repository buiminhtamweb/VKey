use crate::{Result, TextInjector, WindowId, X11KeyboardBackend};

/// A short-lived injector view borrowing the same X11 connection as capture.
///
/// Keeping capture and injection on one connection preserves X11 request
/// ordering. The backend filters XTEST devices and its reserved injection
/// keycode from raw observation, so injected events cannot re-enter the
/// Vietnamese core pipeline.
pub struct X11TextInjector<'a> {
    backend: &'a mut X11KeyboardBackend,
}

impl std::fmt::Debug for X11TextInjector<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("X11TextInjector")
            .finish_non_exhaustive()
    }
}

impl X11KeyboardBackend {
    #[must_use]
    pub fn text_injector(&mut self) -> X11TextInjector<'_> {
        X11TextInjector { backend: self }
    }
}

impl TextInjector for X11TextInjector<'_> {
    fn insert_text(&mut self, text: &str) -> Result<()> {
        self.backend.inject_text(text)
    }

    fn delete_previous_graphemes(&mut self, count: usize) -> Result<()> {
        self.backend.delete_graphemes(count)
    }

    #[cfg(target_os = "linux")]
    fn replace_text(&mut self, delete_graphemes: usize, text: &str) -> Result<()> {
        self.backend.replace_text(delete_graphemes, text)
    }

    fn current_target(&self) -> Result<Option<WindowId>> {
        self.backend.focused_window()
    }
}
