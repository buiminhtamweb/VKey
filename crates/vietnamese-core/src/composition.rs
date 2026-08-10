//! Stateful raw-key and rendered-text composition buffer.

use unicode_segmentation::UnicodeSegmentation;

use crate::{config::InputMethod, telex, vni};

#[derive(Debug, Default)]
pub(crate) struct CompositionBuffer {
    raw: String,
    rendered: String,
}

impl CompositionBuffer {
    pub(crate) fn rendered(&self) -> &str {
        &self.rendered
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rendered.is_empty()
    }

    pub(crate) fn push(
        &mut self,
        character: char,
        method: InputMethod,
        smart_tone: bool,
        restore_typing: bool,
        spelling_check: bool,
    ) {
        self.raw.push(character);
        let transformed = match method {
            InputMethod::Telex => telex::transform(&self.raw, smart_tone, restore_typing),
            InputMethod::Vni => vni::transform(&self.raw, smart_tone, restore_typing),
        };
        self.rendered = if spelling_check
            && transformed != self.raw
            && !transformed.is_ascii()
            && !crate::spelling::is_valid_vietnamese_syllable(&transformed)
        {
            self.raw.clone()
        } else {
            transformed
        };
    }

    pub(crate) fn backspace(&mut self) {
        let new_len = self
            .rendered
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index);
        self.rendered.truncate(new_len);
        self.raw.clone_from(&self.rendered);
    }

    pub(crate) fn clear(&mut self) {
        self.raw.clear();
        self.rendered.clear();
    }
}
