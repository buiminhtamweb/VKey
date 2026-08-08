//! Stateful raw-key and rendered-text composition buffer.

use unicode_segmentation::UnicodeSegmentation;

use crate::{config::InputMethod, telex, vni};

#[derive(Debug, Default)]
pub(crate) struct CompositionBuffer {
    raw: String,
    rendered: String,
    restored: bool,
}

impl CompositionBuffer {
    pub(crate) fn rendered(&self) -> &str {
        &self.rendered
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rendered.is_empty()
    }

    pub(crate) fn push(&mut self, character: char, method: InputMethod, smart_tone: bool) {
        self.raw.push(character);
        self.rendered = if self.restored {
            self.raw.clone()
        } else {
            match method {
                InputMethod::Telex => telex::transform(&self.raw, smart_tone),
                InputMethod::Vni => vni::transform(&self.raw, smart_tone),
            }
        };
    }

    pub(crate) fn can_restore(&self) -> bool {
        !self.restored && self.raw != self.rendered
    }

    pub(crate) fn restore(&mut self) {
        self.rendered.clone_from(&self.raw);
        self.restored = true;
    }

    pub(crate) fn backspace(&mut self) {
        let new_len = self
            .rendered
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index);
        self.rendered.truncate(new_len);
        self.raw.clone_from(&self.rendered);
        self.restored = false;
    }

    pub(crate) fn clear(&mut self) {
        self.raw.clear();
        self.rendered.clear();
        self.restored = false;
    }
}
