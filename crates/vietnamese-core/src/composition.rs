//! Stateful raw-key and rendered-text composition buffer.

use crate::{config::InputMethod, spelling, telex, unicode, vni};

#[derive(Debug, Default)]
pub(crate) struct CompositionBuffer {
    raw: String,
    rendered: String,
}

impl CompositionBuffer {
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    pub(crate) fn rendered(&self) -> &str {
        &self.rendered
    }

    pub(crate) fn push(
        &mut self,
        character: char,
        method: InputMethod,
        smart_tone: bool,
        restore_typing: bool,
    ) {
        self.raw.push(character);
        self.recompute(method, smart_tone, restore_typing);
    }

    #[allow(dead_code)]
    pub(crate) fn pop(
        &mut self,
        method: InputMethod,
        smart_tone: bool,
        restore_typing: bool,
    ) -> Option<char> {
        let popped = self.raw.pop()?;
        if self.raw.is_empty() {
            self.rendered.clear();
        } else {
            self.recompute(method, smart_tone, restore_typing);
        }
        Some(popped)
    }

    fn recompute(&mut self, method: InputMethod, smart_tone: bool, restore_typing: bool) {
        let mut rendered = match method {
            InputMethod::Telex => telex::transform(&self.raw, smart_tone, restore_typing),
            InputMethod::Vni => vni::transform(&self.raw, smart_tone, restore_typing),
        };
        if rendered
            .chars()
            .any(|character| unicode::tone_of(character).is_some())
        {
            unicode::normalize_capitalized_word_case(&mut rendered);
        }
        if restore_typing && spelling::should_restore_raw(&rendered, &self.raw) {
            rendered = self.raw.clone();
        }
        self.rendered = rendered;
    }

    pub(crate) fn clear(&mut self) {
        self.raw.clear();
        self.rendered.clear();
    }
}
