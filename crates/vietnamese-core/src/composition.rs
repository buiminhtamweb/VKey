//! Stateful raw-key and rendered-text composition buffer.

use crate::{config::InputMethod, telex, unicode, vni};

#[derive(Debug, Default)]
pub(crate) struct CompositionBuffer {
    raw: String,
    rendered: String,
}

impl CompositionBuffer {
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
        self.rendered = rendered;
    }

    pub(crate) fn clear(&mut self) {
        self.raw.clear();
        self.rendered.clear();
    }
}
